use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tauri::AppHandle;

use crate::agent::context::AppContextSettings;
use crate::agent::history::{
    build_anthropic_agent_messages, build_openai_agent_messages, AgentLoopTurn, AgentToolCall,
    AgentToolResult,
};
use crate::agent::project_context::{build_agent_system_prompt, load_project_context};
use crate::agent::parser::parse_tool_calls;
use crate::agent::tool_schema;
use crate::agent::tools::{execute_tool, ToolContext, ToolResult};
use crate::providers::agent_stream::stream_agent_completion;
use crate::secrets;
use crate::state::AppState;
use crate::storage::db::Provider;

#[derive(Debug, Clone)]
pub struct SubagentInput {
    pub description: String,
    pub subagent_type: String,
    pub readonly: bool,
}

pub fn parse_spawn_task_args(args: &Value) -> Result<SubagentInput, String> {
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "spawn_task 需要 description".to_string())?
        .to_string();

    let subagent_type = args
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("general")
        .to_string();

    let mut readonly = args
        .get("readonly")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if subagent_type == "explore" {
        readonly = true;
    }

    Ok(SubagentInput {
        description,
        subagent_type,
        readonly,
    })
}

pub fn subagent_system_prompt(subagent_type: &str, readonly: bool) -> String {
    let role = match subagent_type {
        "explore" => {
            "你是探索型子 Agent，负责只读搜索与阅读代码库，返回简洁调研报告（含文件路径与结论）。"
        }
        _ => "你是通用子 Agent，负责独立完成分配的子任务，完成后返回简洁摘要。",
    };
    let readonly_note = if readonly {
        "\n- 只读模式：禁止 write_file、apply_patch、delete_file、run_command"
    } else {
        ""
    };
    format!(
        "{role}

约束：
- 自主完成子任务，不要向用户提问
- 不能 spawn 子 Agent；不能触发需用户确认的操作（web_fetch、危险 shell）{readonly_note}
- 完成后用中文给出结论，控制在 1500 字以内"
    )
}

pub async fn run_subagent(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    input: SubagentInput,
    provider: &Provider,
    api_key: &str,
    workspace: Option<PathBuf>,
    cancel: Arc<AtomicBool>,
    settings: &AppContextSettings,
) -> Result<String, String> {
    let http = state.http.clone();
    let db = state.db.clone();
    let shell_policy = settings.shell_policy();
    let web_search_api_key = secrets::get_api_key(crate::search::WEB_SEARCH_KEY_ACCOUNT).ok();
    let max_iterations = settings.agent_subagent_max_iterations.max(1).min(30);
    let base_system = subagent_system_prompt(&input.subagent_type, input.readonly);
    let system = workspace
        .as_ref()
        .and_then(|ws| load_project_context(ws).ok())
        .map(|bundle| build_agent_system_prompt(&base_system, None, Some(&bundle)))
        .unwrap_or(base_system);

    let mcp_servers: Vec<_> = db
        .list_mcp_servers()
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|row| crate::mcp::McpServerRecord::from_row(row).ok())
        .collect();
    let _ = state.mcp.refresh_from_db(&mcp_servers);

    let exclude = ["spawn_task"];
    let mut mcp_openai = tool_schema::tool_definitions_openai_excluding(&exclude);
    mcp_openai.extend(state.mcp.openai_tool_definitions());
    let mut mcp_anthropic = tool_schema::tool_definitions_anthropic_excluding(&exclude);
    mcp_anthropic.extend(state.mcp.anthropic_tool_definitions());

    let mut loop_turns: Vec<AgentLoopTurn> = Vec::new();
    let task_text = input.description.clone();

    for _iteration in 0..max_iterations {
        if cancel.load(Ordering::Relaxed) {
            return Err("已取消生成".into());
        }

        let user_content = if loop_turns.is_empty() {
            task_text.clone()
        } else {
            "请根据上述工具结果继续。".to_string()
        };

        let openai = build_openai_agent_messages(&[], &loop_turns, &user_content, &system);
        let anthropic = build_anthropic_agent_messages(&[], &loop_turns, &user_content);

        let model_out = stream_agent_completion(
            &http,
            provider,
            api_key,
            openai,
            anthropic,
            &system,
            None,
            &mcp_openai,
            &mcp_anthropic,
            app,
            session_id,
            Arc::clone(&cancel),
        )
        .await
        .map_err(|e| e.message())?;

        let mut calls = model_out.tool_calls;
        let mut display = model_out.text;
        if calls.is_empty() {
            let (visible, legacy) = parse_tool_calls(&display);
            if !legacy.is_empty() {
                calls = legacy;
                display = visible;
            }
        }

        if calls.is_empty() {
            if display.trim().is_empty() {
                return Ok("（子 Agent 未返回文本）".into());
            }
            return Ok(display);
        }

        let tool_ctx = ToolContext {
            db: &db,
            session_id,
            workspace: workspace.clone(),
            shell_policy: shell_policy.clone(),
            mcp: Some(&state.mcp),
            http: Some(&http),
            web_search: settings.web_search_config(),
            web_search_api_key: web_search_api_key.clone(),
            readonly: input.readonly,
            plan_mode: false,
            semantic_search: settings.semantic_search_config(),
            workspace_policy: settings.workspace_path_policy(),
            bypass_outside_approval: false,
        };

        let mut batch_calls: Vec<AgentToolCall> = Vec::new();
        let mut batch_results: Vec<AgentToolResult> = Vec::new();

        for call in &calls {
            if call.name == "spawn_task" {
                batch_calls.push(AgentToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
                batch_results.push(AgentToolResult {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: "子 Agent 不能再次 spawn 子任务".into(),
                });
                continue;
            }

            match execute_tool(call, &tool_ctx) {
                ToolResult::Ok(out) => {
                    batch_calls.push(AgentToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    });
                    batch_results.push(AgentToolResult {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: out,
                    });
                }
                ToolResult::NeedsApproval { .. } => {
                    batch_calls.push(AgentToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    });
                    batch_results.push(AgentToolResult {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: "子 Agent 无法请求用户确认，该操作已跳过".into(),
                    });
                }
                ToolResult::Err(err) => {
                    batch_calls.push(AgentToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    });
                    batch_results.push(AgentToolResult {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: err,
                    });
                }
            }
        }

        loop_turns.push(AgentLoopTurn::Assistant {
            text: display,
            tool_calls: batch_calls,
        });
        loop_turns.push(AgentLoopTurn::ToolResults(batch_results));
    }

    Err(format!("子 Agent 达到最大迭代次数（{max_iterations}）"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explore_forces_readonly() {
        let input = parse_spawn_task_args(&json!({
            "description": "find auth module",
            "subagent_type": "explore"
        }))
        .unwrap();
        assert!(input.readonly);
        assert_eq!(input.subagent_type, "explore");
    }

    #[test]
    fn requires_description() {
        assert!(parse_spawn_task_args(&json!({})).is_err());
    }
}
