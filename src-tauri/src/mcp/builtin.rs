use chrono::Utc;

use crate::error::AppResult;
use crate::mcp::McpServerRecord;
use crate::storage::db::Database;

/// 内置 MCP：开箱即用，不在设置页展示；失败时静默跳过（Agent 仍可用原生工具）。
const BUILTIN_SERVERS: &[(&str, &str, &str, &[&str])] = &[
    (
        "builtin-mcp-memory",
        "Memory",
        "npx",
        &["-y", "@modelcontextprotocol/server-memory"],
    ),
    (
        "builtin-mcp-sequential-thinking",
        "Sequential Thinking",
        "npx",
        &["-y", "@modelcontextprotocol/server-sequential-thinking"],
    ),
    (
        "builtin-mcp-time",
        "Time",
        "npx",
        &["-y", "@modelcontextprotocol/server-time"],
    ),
];

pub fn ensure_builtin_mcp_servers(db: &Database) -> AppResult<()> {
    let now = Utc::now().timestamp();
    for (id, name, command, args) in BUILTIN_SERVERS {
        let record = McpServerRecord {
            id: (*id).to_string(),
            name: (*name).to_string(),
            command: (*command).to_string(),
            args: args.iter().map(|s| (*s).to_string()).collect(),
            env: std::collections::HashMap::new(),
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        db.upsert_mcp_server(&record.to_row())?;
    }
    Ok(())
}
