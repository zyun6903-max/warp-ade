mod builtin;

pub use builtin::ensure_builtin_mcp_servers;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRecord {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTestResult {
    pub ok: bool,
    pub tool_count: usize,
    pub message: String,
    pub tools: Vec<String>,
}

pub fn encode_agent_tool_name(server_id: &str, tool_name: &str) -> String {
    let sid = sanitize_segment(server_id);
    let tname = sanitize_segment(tool_name);
    format!("mcp_{sid}__{tname}")
}

fn sanitize_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

struct StdioMcpClient {
    child: Child,
    stdin: Arc<Mutex<std::process::ChildStdin>>,
    stdout: Arc<Mutex<BufReader<std::process::ChildStdout>>>,
    next_id: AtomicU64,
}

impl StdioMcpClient {
    fn spawn(record: &McpServerRecord) -> AppResult<Self> {
        let mut cmd = Command::new(&record.command);
        cmd.args(&record.args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
        for (k, v) in &record.env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::from(format!("启动 MCP 进程失败: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::from("MCP 进程 stdin 不可用"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::from("MCP 进程 stdout 不可用"))?;
        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            next_id: AtomicU64::new(1),
        })
    }

    fn connect_and_init(record: &McpServerRecord) -> AppResult<Self> {
        Self::connect_and_init_with_timeout(record, Duration::from_secs(10))
    }

    fn connect_and_init_with_timeout(
        record: &McpServerRecord,
        timeout: Duration,
    ) -> AppResult<Self> {
        let record = record.clone();
        let server_name = record.name.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Self::spawn(&record).and_then(|mut c| {
                c.initialize()?;
                Ok(c)
            }));
        });
        match rx.recv_timeout(timeout) {
            Ok(Ok(client)) => Ok(client),
            Ok(Err(err)) => Err(err),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(AppError::from(format!(
                "MCP {} 连接超时（{} 秒）",
                server_name,
                timeout.as_secs()
            ))),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(AppError::from(format!("MCP {} 连接失败", server_name)))
            }
        }
    }

    fn initialize(&mut self) -> AppResult<()> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "warp-ade", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        let resp = self.request(req)?;
        if resp.get("error").is_some() {
            return Err(AppError::from(format!("MCP initialize 失败: {resp}")));
        }
        self.notify_initialized()?;
        Ok(())
    }

    fn notify_initialized(&self) -> AppResult<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        write_message(&self.stdin, &msg)
    }

    fn list_tools(&self) -> AppResult<Vec<McpToolInfo>> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        });
        let resp = self.request(req)?;
        if let Some(err) = resp.get("error") {
            return Err(AppError::from(format!("tools/list 失败: {err}")));
        }
        let tools = resp
            .pointer("/result/tools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(tools
            .into_iter()
            .filter_map(|t| {
                Some(McpToolInfo {
                    name: t.get("name")?.as_str()?.to_string(),
                    description: t
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    input_schema: t
                        .get("inputSchema")
                        .or_else(|| t.get("input_schema"))
                        .cloned()
                        .unwrap_or_else(|| json!({"type":"object","properties":{}})),
                })
            })
            .collect())
    }

    fn call_tool(&self, name: &str, arguments: Value) -> AppResult<String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });
        let resp = self.request(req)?;
        if let Some(err) = resp.get("error") {
            return Err(AppError::from(format!("tools/call 失败: {err}")));
        }
        Ok(format_tool_result(
            resp.pointer("/result").cloned().unwrap_or(Value::Null),
        ))
    }

    fn request(&self, req: Value) -> AppResult<Value> {
        write_message(&self.stdin, &req)?;
        read_response(&self.stdout, req.get("id").and_then(|v| v.as_u64()))
    }
}

fn write_message(stdin: &Arc<Mutex<std::process::ChildStdin>>, msg: &Value) -> AppResult<()> {
    let body = serde_json::to_string(msg)?;
    let mut guard = stdin
        .lock()
        .map_err(|_| AppError::from("MCP stdin 锁失败"))?;
    write!(guard, "Content-Length: {}\r\n\r\n{}", body.len(), body)
        .map_err(|e| AppError::from(format!("MCP 写入失败: {e}")))?;
    guard
        .flush()
        .map_err(|e| AppError::from(format!("MCP flush 失败: {e}")))?;
    Ok(())
}

fn read_response(
    stdout: &Arc<Mutex<BufReader<std::process::ChildStdout>>>,
    expect_id: Option<u64>,
) -> AppResult<Value> {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if std::time::Instant::now() > deadline {
            return Err(AppError::from("MCP 响应超时"));
        }
        let msg = read_message(stdout)?;
        if msg.get("method").is_some() && msg.get("id").is_none() {
            continue;
        }
        if let Some(id) = expect_id {
            if msg.get("id").and_then(|v| v.as_u64()) == Some(id) {
                return Ok(msg);
            }
            continue;
        }
        return Ok(msg);
    }
}

fn read_message(stdout: &Arc<Mutex<BufReader<std::process::ChildStdout>>>) -> AppResult<Value> {
    let mut guard = stdout
        .lock()
        .map_err(|_| AppError::from("MCP stdout 锁失败"))?;
    let mut content_length = None;
    loop {
        let mut line = String::new();
        guard
            .read_line(&mut line)
            .map_err(|e| AppError::from(format!("MCP 读取头失败: {e}")))?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                rest.trim()
                    .parse::<usize>()
                    .map_err(|_| AppError::from("无效的 Content-Length"))?,
            );
        }
    }
    let len = content_length.ok_or_else(|| AppError::from("MCP 消息缺少 Content-Length"))?;
    let mut buf = vec![0u8; len];
    guard
        .read_exact(&mut buf)
        .map_err(|e| AppError::from(format!("MCP 读取 body 失败: {e}")))?;
    serde_json::from_slice(&buf).map_err(|e| AppError::from(format!("MCP JSON 解析失败: {e}")))
}

fn format_tool_result(result: Value) -> String {
    if let Some(text) = result.get("content").and_then(|c| c.as_array()) {
        let parts: Vec<String> = text
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
            })
            .collect();
        if !parts.is_empty() {
            return parts.join("\n");
        }
    }
    if let Some(s) = result.as_str() {
        return s.to_string();
    }
    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
}

#[derive(Clone)]
struct CachedMcpTools {
    server_id: String,
    server_name: String,
    tools: Vec<McpToolInfo>,
}

pub struct McpManager {
    cache: Mutex<Vec<CachedMcpTools>>,
    clients: Mutex<std::collections::HashMap<String, Arc<StdioMcpClient>>>,
    agent_tool_map: Mutex<std::collections::HashMap<String, (String, String)>>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(Vec::new()),
            clients: Mutex::new(std::collections::HashMap::new()),
            agent_tool_map: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn invalidate(&self) {
        if let Ok(mut clients) = self.clients.lock() {
            clients.clear();
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
        if let Ok(mut map) = self.agent_tool_map.lock() {
            map.clear();
        }
    }

    pub fn refresh_from_db(&self, servers: &[McpServerRecord]) -> AppResult<()> {
        self.invalidate();
        let mut cache = Vec::new();
        let mut clients = std::collections::HashMap::new();
        let mut agent_tool_map = std::collections::HashMap::new();
        for server in servers.iter().filter(|s| s.enabled) {
            match StdioMcpClient::connect_and_init(server) {
                Ok(client) => {
                    let tools = client.list_tools().unwrap_or_default();
                    for tool in &tools {
                        let agent_name = encode_agent_tool_name(&server.id, &tool.name);
                        agent_tool_map.insert(
                            agent_name,
                            (server.id.clone(), tool.name.clone()),
                        );
                    }
                    cache.push(CachedMcpTools {
                        server_id: server.id.clone(),
                        server_name: server.name.clone(),
                        tools: tools.clone(),
                    });
                    clients.insert(server.id.clone(), Arc::new(client));
                }
                Err(e) => {
                    eprintln!("MCP {} 连接失败: {e}", server.name);
                }
            }
        }
        *self.cache.lock().map_err(|_| AppError::from("MCP cache 锁失败"))? = cache;
        *self.clients.lock().map_err(|_| AppError::from("MCP clients 锁失败"))? = clients;
        *self
            .agent_tool_map
            .lock()
            .map_err(|_| AppError::from("MCP map 锁失败"))? = agent_tool_map;
        Ok(())
    }

    pub fn openai_tool_definitions(&self) -> Vec<Value> {
        let Ok(guard) = self.cache.lock() else {
            return Vec::new();
        };
        let mut defs = Vec::new();
        for entry in guard.iter() {
            for tool in &entry.tools {
                let agent_name = encode_agent_tool_name(&entry.server_id, &tool.name);
                let desc = format!(
                    "[MCP:{}] {}",
                    entry.server_name,
                    tool.description.as_deref().unwrap_or(&tool.name)
                );
                defs.push(json!({
                    "type": "function",
                    "function": {
                        "name": agent_name,
                        "description": desc,
                        "parameters": tool.input_schema
                    }
                }));
            }
        }
        defs
    }

    pub fn anthropic_tool_definitions(&self) -> Vec<Value> {
        self.openai_tool_definitions()
            .into_iter()
            .filter_map(|t| {
                let f = t.get("function")?;
                Some(json!({
                    "name": f.get("name")?,
                    "description": f.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                    "input_schema": f.get("parameters")?.clone()
                }))
            })
            .collect()
    }

    pub fn call_agent_tool(&self, agent_tool_name: &str, arguments: Value) -> AppResult<String> {
        let (server_id, original_name) = self
            .agent_tool_map
            .lock()
            .map_err(|_| AppError::from("MCP map 锁失败"))?
            .get(agent_tool_name)
            .cloned()
            .ok_or_else(|| AppError::from(format!("未知的 MCP 工具: {agent_tool_name}")))?;
        let clients = self
            .clients
            .lock()
            .map_err(|_| AppError::from("MCP clients 锁失败"))?;
        let client = clients
            .get(&server_id)
            .ok_or_else(|| AppError::from(format!("MCP 服务未连接: {server_id}")))?;
        client.call_tool(&original_name, arguments)
    }

    pub fn test_server(record: &McpServerRecord) -> McpTestResult {
        match StdioMcpClient::connect_and_init(record) {
            Ok(client) => match client.list_tools() {
                Ok(tools) => {
                    let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
                    McpTestResult {
                        ok: true,
                        tool_count: tools.len(),
                        message: format!("连接成功，发现 {} 个工具", tools.len()),
                        tools: names,
                    }
                }
                Err(e) => McpTestResult {
                    ok: false,
                    tool_count: 0,
                    message: e.to_string(),
                    tools: vec![],
                },
            },
            Err(e) => McpTestResult {
                ok: false,
                tool_count: 0,
                message: e.to_string(),
                tools: vec![],
            },
        }
    }

    pub fn cached_tool_count(&self) -> usize {
        self.cache
            .lock()
            .map(|c| c.iter().map(|e| e.tools.len()).sum())
            .unwrap_or(0)
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServerRecord {
    pub fn from_row(row: &crate::storage::db::McpServerRow) -> AppResult<Self> {
        let args: Vec<String> = serde_json::from_str(&row.args_json)
            .map_err(|e| AppError::from(format!("args JSON 无效: {e}")))?;
        let env: std::collections::HashMap<String, String> =
            serde_json::from_str(&row.env_json)
                .map_err(|e| AppError::from(format!("env JSON 无效: {e}")))?;
        Ok(Self {
            id: row.id.clone(),
            name: row.name.clone(),
            command: row.command.clone(),
            args,
            env,
            enabled: row.enabled,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    pub fn to_row(&self) -> crate::storage::db::McpServerRow {
        crate::storage::db::McpServerRow {
            id: self.id.clone(),
            name: self.name.clone(),
            command: self.command.clone(),
            args_json: serde_json::to_string(&self.args).unwrap_or_else(|_| "[]".into()),
            env_json: serde_json::to_string(&self.env).unwrap_or_else(|_| "{}".into()),
            enabled: self.enabled,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub fn import_cursor_mcp_json() -> AppResult<Vec<McpServerRecord>> {
    let home = dirs::home_dir().ok_or_else(|| AppError::from("无法定位用户目录"))?;
    let path = home.join(".cursor").join("mcp.json");
    if !path.is_file() {
        return Err(AppError::from(format!(
            "未找到 {}",
            path.display()
        )));
    }
    let raw = std::fs::read_to_string(&path)?;
    parse_mcp_json_file(&raw)
}

pub fn parse_mcp_json_file(raw: &str) -> AppResult<Vec<McpServerRecord>> {
    let v: Value = serde_json::from_str(raw)?;
    let servers_obj = v
        .get("mcpServers")
        .or_else(|| v.get("servers"))
        .and_then(|s| s.as_object())
        .ok_or_else(|| AppError::from("JSON 中缺少 mcpServers 对象"))?;
    let now = chrono::Utc::now().timestamp();
    let mut out = Vec::new();
    for (name, cfg) in servers_obj {
        let command = cfg
            .get("command")
            .and_then(|c| c.as_str())
            .ok_or_else(|| AppError::from(format!("{name} 缺少 command")))?
            .to_string();
        let args: Vec<String> = cfg
            .get("args")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let env: std::collections::HashMap<String, String> = cfg
            .get("env")
            .and_then(|e| e.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        out.push(McpServerRecord {
            id: Uuid::new_v4().to_string(),
            name: name.clone(),
            command,
            args,
            env,
            enabled: true,
            created_at: now,
            updated_at: now,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_tool_names_with_prefix() {
        let name = encode_agent_tool_name("abc-123", "read_file");
        assert!(name.starts_with("mcp_"));
        assert!(name.contains("__read_file"));
    }
}
