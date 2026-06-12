use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use regex::Regex;
use reqwest::Client;
use serde_json::Value;
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::mcp::McpManager;
use crate::search::WebSearchConfig;
use crate::search::SemanticSearchConfig;
use crate::agent::project_context::SkillEntry;
use crate::storage::db::Database;

use super::project_context::{find_skill_by_name, load_skill_body, truncate_chars, MAX_SINGLE_FILE};
use super::parser::ParsedToolCall;
use super::shell_policy::{shell_requires_approval, ShellPolicyConfig};
use super::workspace_policy::{
    evaluate_path_access, resolve_agent_path, PathDecision, PathIntent, WorkspacePathPolicy,
};

#[derive(Debug, Clone)]
pub struct ShellRunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ShellRunResult {
    pub fn formatted(&self) -> String {
        format!(
            "exit={}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.exit_code, self.stdout, self.stderr
        )
    }

    pub fn preview(&self) -> String {
        self.formatted().chars().take(500).collect()
    }
}

#[derive(Debug, Clone)]
pub enum PendingApproval {
    Shell(String),
    WebFetch(String),
    OutsideRead(String),
    OutsideWrite(String),
}

#[derive(Debug, Clone)]
pub enum ToolResult {
    Ok(String),
    NeedsApproval {
        approval_id: String,
        action: PendingApproval,
    },
    Err(String),
}

pub struct ToolContext<'a> {
    pub db: &'a Database,
    pub session_id: &'a str,
    pub workspace: Option<PathBuf>,
    pub shell_policy: ShellPolicyConfig,
    pub mcp: Option<&'a McpManager>,
    pub http: Option<&'a Client>,
    pub web_search: WebSearchConfig,
    pub web_search_api_key: Option<String>,
    pub readonly: bool,
    pub plan_mode: bool,
    pub semantic_search: SemanticSearchConfig,
    pub workspace_policy: WorkspacePathPolicy,
    pub bypass_outside_approval: bool,
    pub skill_catalog: &'a [SkillEntry],
}

fn plan_mode_blocked_tool(name: &str) -> bool {
    matches!(name, "delete_file" | "run_command" | "spawn_task")
}

pub fn plan_mode_write_allowed(rel_path: &str) -> bool {
    let normalized = rel_path.replace('\\', "/");
    let p = normalized.trim_start_matches("./");
    p.starts_with("docs/superpowers/") || p.starts_with("docs/plans/")
}

fn readonly_blocked_tool(name: &str) -> bool {
    matches!(
        name,
        "write_file" | "apply_patch" | "delete_file" | "run_command"
    )
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn arg_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64()).map(|n| n as usize)
}

fn arg_bool(args: &Value, key: &str) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn gate_agent_path(
    ctx: &ToolContext<'_>,
    path: &str,
    intent: PathIntent,
) -> Result<PathBuf, ToolResult> {
    let (target, inside) = resolve_agent_path(ctx.workspace.as_deref(), path)
        .map_err(|e| ToolResult::Err(e.to_string()))?;
    if ctx.bypass_outside_approval {
        return Ok(target);
    }
    if intent == PathIntent::Read && is_managed_attachment(&target) {
        return Ok(target);
    }
    match evaluate_path_access(inside, intent, &ctx.workspace_policy) {
        PathDecision::Allow => Ok(target),
        PathDecision::Deny(msg) => Err(ToolResult::Err(msg.into())),
        PathDecision::NeedsReadApproval => Err(ToolResult::NeedsApproval {
            approval_id: uuid::Uuid::new_v4().to_string(),
            action: PendingApproval::OutsideRead(target.display().to_string()),
        }),
        PathDecision::NeedsWriteApproval => Err(ToolResult::NeedsApproval {
            approval_id: uuid::Uuid::new_v4().to_string(),
            action: PendingApproval::OutsideWrite(target.display().to_string()),
        }),
    }
}

fn is_managed_attachment(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/.warp-ade/attachments/") || s.contains("\\.warp-ade\\attachments\\") {
        return true;
    }
    if let Ok(data_dir) = crate::storage::db::app_data_dir() {
        if path.starts_with(data_dir.join("attachments")) {
            return true;
        }
    }
    false
}

pub fn execute_paused_tool_call(
    call: &crate::agent::history::AgentToolCall,
    ctx: &ToolContext<'_>,
) -> ToolResult {
    execute_tool(
        &ParsedToolCall {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        },
        ctx,
    )
}

pub fn execute_tool(call: &ParsedToolCall, ctx: &ToolContext<'_>) -> ToolResult {
    if ctx.readonly && readonly_blocked_tool(&call.name) {
        return ToolResult::Err("只读模式不允许此工具".into());
    }
    if ctx.plan_mode && plan_mode_blocked_tool(&call.name) {
        return ToolResult::Err("Plan 模式不允许此工具，请切换 Agent 模式执行".into());
    }
    if ctx.plan_mode && matches!(call.name.as_str(), "write_file" | "apply_patch") {
        let path = match arg_str(&call.arguments, "path") {
            Some(p) => p,
            None => return ToolResult::Err(format!("{} 需要 path 参数", call.name)),
        };
        if !plan_mode_write_allowed(&path) {
            return ToolResult::Err(
                "Plan 模式仅允许写入 docs/superpowers/ 或 docs/plans/ 下的设计/计划文档".into(),
            );
        }
    }
    if call.name.starts_with("mcp_") {
        return match ctx.mcp {
            Some(mcp) => match mcp.call_agent_tool(&call.name, call.arguments.clone()) {
                Ok(out) => ToolResult::Ok(out),
                Err(e) => ToolResult::Err(e.to_string()),
            },
            None => ToolResult::Err("MCP 服务未连接".into()),
        };
    }

    match call.name.as_str() {
        "read_file" => {
            let Some(path) = arg_str(&call.arguments, "path") else {
                return ToolResult::Err("read_file 需要 path 参数".into());
            };
            let offset = arg_usize(&call.arguments, "offset").unwrap_or(1);
            let limit = arg_usize(&call.arguments, "limit");
            let target = match gate_agent_path(ctx, &path, PathIntent::Read) {
                Ok(p) => p,
                Err(tr) => return tr,
            };
            if crate::attachments::is_image_path(&target) {
                match crate::attachments::read_image_for_agent(&target) {
                    Ok(out) => return ToolResult::Ok(out),
                    Err(e) => return ToolResult::Err(e.to_string()),
                }
            }
            match std::fs::read_to_string(&target)
                .map_err(|e| AppError::from(format!("读取失败: {e}")))
            {
                Ok(content) => ToolResult::Ok(format_file_content(&content, offset, limit)),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "list_directory" => {
            let rel = arg_str(&call.arguments, "path").unwrap_or_else(|| ".".to_string());
            let target = match gate_agent_path(ctx, &rel, PathIntent::Read) {
                Ok(p) => p,
                Err(tr) => return tr,
            };
            match list_dir(&target) {
                Ok(listing) => ToolResult::Ok(listing),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "grep_project" => {
            let pattern = match arg_str(&call.arguments, "pattern") {
                Some(p) => p,
                None => return ToolResult::Err("grep_project 需要 pattern".into()),
            };
            let sub = arg_str(&call.arguments, "path").unwrap_or_else(|| ".".to_string());
            let case_insensitive = arg_bool(&call.arguments, "case_insensitive");
            let context = arg_usize(&call.arguments, "context").unwrap_or(0);
            let max_results = arg_usize(&call.arguments, "max_results").unwrap_or(50);
            let root = match gate_agent_path(ctx, &sub, PathIntent::Read) {
                Ok(p) => p,
                Err(tr) => return tr,
            };
            let opts = GrepOptions {
                pattern,
                case_insensitive,
                context,
                max_results,
            };
            match grep_project(&root, &opts) {
                Ok(hits) => ToolResult::Ok(hits),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "search_history" => {
            let query = match arg_str(&call.arguments, "query") {
                Some(q) => q,
                None => return ToolResult::Err("search_history 需要 query".into()),
            };
            match ctx.db.search_sessions(&query, Some(10), None, None) {
                Ok(hits) => {
                    if hits.is_empty() {
                        return ToolResult::Ok("未找到匹配的历史消息".into());
                    }
                    let lines = hits
                        .iter()
                        .take(10)
                        .map(|h| {
                            format!(
                                "- [{}] {}: {}",
                                h.session.title,
                                h.matched_seq,
                                h.matched_preview
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    ToolResult::Ok(lines)
                }
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "use_skill" => {
            let name = match arg_str(&call.arguments, "name") {
                Some(n) => n,
                None => return ToolResult::Err("use_skill 需要 name 参数".into()),
            };
            let Some(skill) = find_skill_by_name(ctx.skill_catalog, &name) else {
                let names: Vec<_> = ctx
                    .skill_catalog
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect();
                return ToolResult::Err(if names.is_empty() {
                    format!("未找到 Skill「{name}」，当前无可用 Skills")
                } else {
                    format!(
                        "未找到 Skill「{name}」，可用: {}",
                        names.join(", ")
                    )
                });
            };
            match load_skill_body(&skill.path) {
                Ok(body) => {
                    let trimmed = truncate_chars(&body, MAX_SINGLE_FILE);
                    ToolResult::Ok(format!(
                        "# Skill: {}\n\n{}\n\n---\n路径: `{}`",
                        skill.name,
                        trimmed.trim(),
                        skill.path
                    ))
                }
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "glob_files" => {
            let pattern = match arg_str(&call.arguments, "pattern") {
                Some(p) => p,
                None => return ToolResult::Err("glob_files 需要 pattern".into()),
            };
            let sub = arg_str(&call.arguments, "path").unwrap_or_else(|| ".".to_string());
            let Some(ws) = ctx.workspace.as_ref() else {
                return ToolResult::Err("glob_files 需要绑定工作区".into());
            };
            let base = match gate_agent_path(ctx, &sub, PathIntent::Read) {
                Ok(p) => p,
                Err(tr) => return tr,
            };
            if !base.starts_with(ws) {
                return ToolResult::Err("glob 起始目录须在工作区内".into());
            }
            match glob_files(ws, &base, &pattern) {
                Ok(listing) => ToolResult::Ok(listing),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "run_command" => {
            let command = match arg_str(&call.arguments, "command") {
                Some(c) => c,
                None => return ToolResult::Err("run_command 需要 command".into()),
            };
            if shell_requires_approval(&command, &ctx.shell_policy) {
                return ToolResult::NeedsApproval {
                    approval_id: uuid::Uuid::new_v4().to_string(),
                    action: PendingApproval::Shell(command),
                };
            }
            run_shell_with_log(
                ctx.db,
                ctx.session_id,
                &command,
                ctx.workspace.as_deref(),
                "auto",
            )
        }
        "write_file" => {
            let path = match arg_str(&call.arguments, "path") {
                Some(p) => p,
                None => return ToolResult::Err("write_file 需要 path".into()),
            };
            let content = match call.arguments.get("content").and_then(|v| v.as_str()) {
                Some(c) => c,
                None => return ToolResult::Err("write_file 需要 content".into()),
            };
            let target = match gate_agent_path(ctx, &path, PathIntent::Write) {
                Ok(p) => p,
                Err(tr) => return tr,
            };
            match write_file(&target, content) {
                Ok(msg) => ToolResult::Ok(msg),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "apply_patch" => {
            let path = match arg_str(&call.arguments, "path") {
                Some(p) => p,
                None => return ToolResult::Err("apply_patch 需要 path".into()),
            };
            let old_string = match call.arguments.get("old_string").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return ToolResult::Err("apply_patch 需要 old_string".into()),
            };
            let new_string = match call.arguments.get("new_string").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return ToolResult::Err("apply_patch 需要 new_string".into()),
            };
            let replace_all = call
                .arguments
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let target = match gate_agent_path(ctx, &path, PathIntent::Write) {
                Ok(p) => p,
                Err(tr) => return tr,
            };
            match apply_patch(&target, old_string, new_string, replace_all) {
                Ok(msg) => ToolResult::Ok(msg),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "delete_file" => {
            let path = match arg_str(&call.arguments, "path") {
                Some(p) => p,
                None => return ToolResult::Err("delete_file 需要 path".into()),
            };
            let target = match gate_agent_path(ctx, &path, PathIntent::Write) {
                Ok(p) => p,
                Err(tr) => return tr,
            };
            match delete_file(&target) {
                Ok(msg) => ToolResult::Ok(msg),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "web_fetch" => {
            let url = match arg_str(&call.arguments, "url") {
                Some(u) => u,
                None => return ToolResult::Err("web_fetch 需要 url".into()),
            };
            if !is_allowed_fetch_url(&url) {
                return ToolResult::Err("仅支持 http/https URL".into());
            }
            ToolResult::NeedsApproval {
                approval_id: uuid::Uuid::new_v4().to_string(),
                action: PendingApproval::WebFetch(url),
            }
        }
        "web_search" => {
            let query = match arg_str(&call.arguments, "query") {
                Some(q) => q,
                None => return ToolResult::Err("web_search 需要 query".into()),
            };
            let Some(http) = ctx.http else {
                return ToolResult::Err("HTTP 客户端不可用".into());
            };
            let mut cfg = ctx.web_search.clone();
            if let Some(max) = arg_usize(&call.arguments, "max_results") {
                cfg.max_results = max.min(20).max(1);
            }
            let api_key = match ctx.web_search_api_key.as_deref() {
                Some(k) if !k.trim().is_empty() => k,
                _ => return ToolResult::Err("未配置 Web 搜索 API Key（可设置 BRAVE_API_KEY 或 TAVILY_API_KEY 环境变量）".into()),
            };
            match crate::search::search_blocking(http, &cfg, api_key, &query) {
                Ok(out) => ToolResult::Ok(out),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        "codebase_search" => {
            let query = match arg_str(&call.arguments, "query") {
                Some(q) => q,
                None => return ToolResult::Err("codebase_search 需要 query".into()),
            };
            let Some(ws) = ctx.workspace.as_ref() else {
                return ToolResult::Err("当前会话未绑定工作区".into());
            };
            let Some(http) = ctx.http else {
                return ToolResult::Err("HTTP 客户端不可用".into());
            };
            let mut cfg = ctx.semantic_search.clone();
            if let Some(max) = arg_usize(&call.arguments, "max_results") {
                cfg.max_results = max.min(20).max(1);
            }
            match crate::search::semantic_search_blocking(http, ctx.db, ws, &cfg, &query) {
                Ok(out) => ToolResult::Ok(out),
                Err(e) => ToolResult::Err(e.to_string()),
            }
        }
        other => ToolResult::Err(format!("未知工具: {other}")),
    }
}

pub fn run_shell_command(command: &str, workspace: Option<&Path>) -> Result<ShellRunResult, String> {
    let output = crate::platform::run_shell(command, workspace);

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let exit_code = out.status.code().unwrap_or(-1);
            Ok(ShellRunResult {
                exit_code,
                stdout,
                stderr,
            })
        }
        Err(e) => Err(format!("执行失败: {e}")),
    }
}

pub fn run_shell_with_log(
    db: &Database,
    session_id: &str,
    command: &str,
    workspace: Option<&Path>,
    mode: &str,
) -> ToolResult {
    match run_shell_command(command, workspace) {
        Ok(result) => {
            let _ = db.insert_shell_log(
                Some(session_id),
                command,
                mode,
                Some(result.exit_code),
                Some(&result.preview()),
            );
            ToolResult::Ok(result.formatted())
        }
        Err(e) => {
            let _ = db.insert_shell_log(
                Some(session_id),
                command,
                mode,
                None,
                Some(&e),
            );
            ToolResult::Err(e)
        }
    }
}

pub fn log_shell_rejection(db: &Database, session_id: &str, command: &str) {
    let _ = db.insert_shell_log(
        Some(session_id),
        command,
        "rejected",
        None,
        Some("用户拒绝执行"),
    );
}

pub async fn fetch_web(http: &Client, url: &str) -> AppResult<String> {
    if !is_allowed_fetch_url(url) {
        return Err(AppError::from("仅支持 http/https URL"));
    }
    let response = http
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| AppError::from(format!("请求失败: {e}")))?;
    if !response.status().is_success() {
        return Err(AppError::from(format!("HTTP {}", response.status())));
    }
    let text = response
        .text()
        .await
        .map_err(|e| AppError::from(format!("读取响应失败: {e}")))?;
    const MAX: usize = 50_000;
    if text.chars().count() > MAX {
        Ok(format!(
            "{}…\n（已截断，共 {} 字符）",
            text.chars().take(MAX).collect::<String>(),
            text.chars().count()
        ))
    } else {
        Ok(text)
    }
}

fn is_allowed_fetch_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn format_file_content(content: &str, offset: usize, limit: Option<usize>) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    if total == 0 {
        return "（空文件）".into();
    }
    let start_line = offset.max(1);
    let start_idx = start_line - 1;
    if start_idx >= total {
        return format!("（文件共 {total} 行，offset {start_line} 超出范围）");
    }
    let end_idx = limit
        .map(|l| (start_idx + l).min(total))
        .unwrap_or(total)
        .min(total);
    let body: String = lines[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{}|{line}", start_idx + i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    if end_idx < total {
        format!("{body}\n（显示 {start_line}–{end_idx} 行，共 {total} 行）")
    } else if start_line > 1 {
        format!("{body}\n（从第 {start_line} 行到末尾，共 {total} 行）")
    } else {
        body
    }
}

fn delete_file(path: &Path) -> AppResult<String> {
    if !path.exists() {
        return Err(AppError::from(format!("文件不存在: {}", path.display())));
    }
    if path.is_dir() {
        return Err(AppError::from("delete_file 仅支持文件，不能删除目录"));
    }
    std::fs::remove_file(path)?;
    Ok(format!("已删除 {}", path.display()))
}

struct GrepOptions {
    pattern: String,
    case_insensitive: bool,
    context: usize,
    max_results: usize,
}

fn grep_project(root: &Path, opts: &GrepOptions) -> AppResult<String> {
    if let Ok(hits) = grep_with_rg(root, opts) {
        return Ok(hits);
    }
    grep_walkdir(root, opts)
}

fn grep_with_rg(root: &Path, opts: &GrepOptions) -> AppResult<String> {
    let mut cmd = Command::new("rg");
    cmd.arg("--line-number").arg("--no-heading").arg("--color=never");
    if opts.case_insensitive {
        cmd.arg("-i");
    }
    if opts.context > 0 {
        cmd.arg("-C").arg(opts.context.to_string());
    }
    cmd.arg(&opts.pattern).arg(root);
    let output = cmd.output().map_err(|e| AppError::from(format!("rg 不可用: {e}")))?;
    if !output.status.success() && output.status.code() != Some(1) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::from(format!("rg 失败: {stderr}")));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().take(opts.max_results).collect();
    if lines.is_empty() {
        Ok("无匹配".into())
    } else {
        Ok(lines.join("\n"))
    }
}

fn grep_walkdir(root: &Path, opts: &GrepOptions) -> AppResult<String> {
    let re = Regex::new(&opts.pattern).ok();
    let mut hits = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .take(8000)
    {
        let path = entry.path();
        if path.extension().is_some_and(|ext| {
            matches!(
                ext.to_str(),
                Some("png" | "jpg" | "jpeg" | "gif" | "ico" | "woff" | "woff2" | "pdf" | "zip")
            )
        }) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let file_lines: Vec<&str> = content.lines().collect();
        for (i, line) in file_lines.iter().enumerate() {
            let matched = re
                .as_ref()
                .map(|r| r.is_match(line))
                .unwrap_or_else(|| {
                    if opts.case_insensitive {
                        line.to_lowercase().contains(&opts.pattern.to_lowercase())
                    } else {
                        line.contains(&opts.pattern)
                    }
                });
            if matched {
                let rel = path.strip_prefix(root).unwrap_or(path);
                hits.push(format!("{}:{}: {}", rel.display(), i + 1, line.trim()));
                if hits.len() >= opts.max_results {
                    return Ok(hits.join("\n"));
                }
            }
        }
    }
    if hits.is_empty() {
        Ok("无匹配".into())
    } else {
        Ok(hits.join("\n"))
    }
}

fn write_file(path: &Path, content: &str) -> AppResult<String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)?;
    Ok(format!("已写入 {}（{} 字节）", path.display(), content.len()))
}

fn apply_patch(
    path: &Path,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> AppResult<String> {
    if !path.is_file() {
        return Err(AppError::from(format!("文件不存在: {}", path.display())));
    }
    let content = std::fs::read_to_string(path)?;
    let count = content.matches(old_string).count();
    if count == 0 {
        return Err(AppError::from("未找到 old_string，请确认内容与文件一致"));
    }
    if count > 1 && !replace_all {
        return Err(AppError::from(format!(
            "old_string 出现 {count} 次，请提供更多上下文或设置 replace_all: true"
        )));
    }
    let updated = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };
    std::fs::write(path, &updated)?;
    Ok(format!(
        "已更新 {}（替换 {} 处）",
        path.display(),
        if replace_all { count } else { 1 }
    ))
}

fn glob_files(workspace: &Path, base: &Path, pattern: &str) -> AppResult<String> {
    if !base.starts_with(workspace) {
        return Err(AppError::from("路径超出工作区范围"));
    }
    let glob_path = base.join(pattern);
    let glob_str = glob_path.to_string_lossy();
    let mut matches = Vec::new();
    for entry in glob::glob(&glob_str).map_err(|e| AppError::from(format!("glob 模式无效: {e}")))?
    {
        let path = entry.map_err(|e| AppError::from(format!("glob 遍历失败: {e}")))?;
        if !path.starts_with(workspace) || !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(workspace)
            .unwrap_or(&path)
            .display()
            .to_string();
        matches.push(rel);
        if matches.len() >= 100 {
            break;
        }
    }
    matches.sort();
    if matches.is_empty() {
        Ok("无匹配文件".into())
    } else {
        Ok(matches.join("\n"))
    }
}

fn list_dir(path: &Path) -> AppResult<String> {
    if !path.is_dir() {
        return Err(AppError::from("不是目录"));
    }
    let mut names: Vec<String> = std::fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() {
                format!("{name}/")
            } else {
                name
            }
        })
        .collect();
    names.sort();
    Ok(names.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn format_file_with_offset() {
        let content = "a\nb\nc\nd\n";
        let out = format_file_content(content, 2, Some(2));
        assert!(out.contains("2|b"));
        assert!(out.contains("3|c"));
    }

    #[test]
    fn plan_mode_write_allowed_paths() {
        assert!(plan_mode_write_allowed("docs/plans/foo.md"));
        assert!(plan_mode_write_allowed("docs/superpowers/plans/bar.md"));
        assert!(plan_mode_write_allowed("./docs/superpowers/specs/x.md"));
        assert!(!plan_mode_write_allowed("src/main.rs"));
    }

    #[test]
    fn write_and_patch_file() {
        let dir = std::env::temp_dir().join(format!("warp-ade-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hello.txt");
        write_file(&file, "hello world").unwrap();
        apply_patch(&file, "world", "warp-ade", false).unwrap();
        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "hello warp-ade");
        let _ = fs::remove_dir_all(&dir);
    }
}
