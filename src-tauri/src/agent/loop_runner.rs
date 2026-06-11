use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use reqwest::Client;
use tauri::AppHandle;

use crate::agent::context::{build_context, maybe_update_summary, AppContextSettings};
use crate::agent::events::{emit_agent_phase, emit_tool_event};
use crate::agent::history::{
    build_anthropic_agent_messages, build_openai_agent_messages, AgentLoopTurn, AgentToolCall,
    AgentToolResult,
};
use crate::agent::plan_mode::load_plan_mode_superpowers_skills;
use crate::agent::project_context::{build_agent_system_prompt, build_plan_system_prompt, load_project_context};
use crate::agent::parser::{agent_system_prompt, parse_tool_calls, ParsedToolCall};
use crate::agent::tool_preview::{format_tool_call_preview, format_tool_done_preview};
use crate::agent::tool_schema::{self};
use crate::agent::audit;
use crate::agent::subagent;
use crate::agent::tools::{execute_tool, ToolContext, ToolResult};
use crate::error::{AppError, AppResult};
use crate::providers::agent_stream::stream_agent_completion;
use crate::providers::chat::{append_turn, order_providers, ChatResponse};
use crate::providers::stream::{emit_done, emit_error};
use crate::secrets;
use crate::state::{AppState, PendingAgentPause};
use crate::storage::db::{Database, Provider};

fn audit_tool(db: &Database, session_id: &str, call: &ParsedToolCall, mode: &str, output: Option<&str>) {
    audit::log_tool_audit(
        db,
        session_id,
        &call.name,
        mode,
        &audit::preview_args(&call.arguments),
        output,
    );
}

struct AgentRunInput {
    user_text: String,
    loop_turns: Vec<AgentLoopTurn>,
    provider_id: Option<String>,
    auto_failover: bool,
    plan_mode: bool,
}

fn emit_tool(
    app: &AppHandle,
    session_id: &str,
    call: &ParsedToolCall,
    status: &str,
    preview: &str,
) {
    emit_tool_event(app, session_id, &call.id, &call.name, status, preview);
}

fn chat_response(
    content: String,
    provider: &Provider,
    primary_id: Option<&str>,
    attempts: usize,
) -> ChatResponse {
    ChatResponse {
        content,
        provider_id: provider.id.clone(),
        provider_name: provider.name.clone(),
        failovered: primary_id.is_some_and(|id| id != provider.id),
        attempts,
        agent_paused: false,
        approval_id: None,
        pending_action: None,
        pending_command: None,
    }
}

pub async fn run_agent_turn(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    user_text: &str,
    provider_id: Option<&str>,
    auto_failover: bool,
    cancel: Arc<AtomicBool>,
    settings: &AppContextSettings,
    plan_mode: bool,
) -> AppResult<ChatResponse> {
    emit_agent_phase(app, session_id, "turn-start", "正在处理请求…");
    tokio::task::yield_now().await;
    run_agent_loop(
        app,
        state,
        session_id,
        AgentRunInput {
            user_text: user_text.to_string(),
            loop_turns: Vec::new(),
            provider_id: provider_id.map(str::to_string),
            auto_failover,
            plan_mode,
        },
        cancel,
        settings,
    )
    .await
}

pub async fn resume_agent_after_shell(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    tool_output: String,
    cancel: Arc<AtomicBool>,
    settings: &AppContextSettings,
) -> AppResult<ChatResponse> {
    let pending = state
        .take_pending_agent(session_id)
        .ok_or_else(|| AppError::from("没有待恢复的 Agent 任务"))?;
    resume_agent_with_pending(
        app,
        state,
        session_id,
        pending,
        tool_output,
        cancel,
        settings,
    )
    .await
}

pub async fn resume_agent_with_pending(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    pending: PendingAgentPause,
    tool_output: String,
    cancel: Arc<AtomicBool>,
    settings: &AppContextSettings,
) -> AppResult<ChatResponse> {
    let mut loop_turns = pending.loop_turns;
    loop_turns.push(AgentLoopTurn::Assistant {
        text: pending.assistant_text,
        tool_calls: vec![pending.paused_tool_call.clone()],
    });
    loop_turns.push(AgentLoopTurn::ToolResults(vec![AgentToolResult {
        tool_call_id: pending.paused_tool_call.id.clone(),
        name: pending.paused_tool_call.name.clone(),
        content: tool_output,
    }]));

    run_agent_loop(
        app,
        state,
        session_id,
        AgentRunInput {
            user_text: pending.user_text,
            loop_turns,
            provider_id: pending.provider_id,
            auto_failover: pending.auto_failover,
            plan_mode: pending.plan_mode,
        },
        cancel,
        settings,
    )
    .await
}

async fn run_agent_loop(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    input: AgentRunInput,
    cancel: Arc<AtomicBool>,
    settings: &AppContextSettings,
) -> AppResult<ChatResponse> {
    let db = Arc::clone(&state.db);
    let http = state.http.clone();

    emit_agent_phase(app, session_id, "boot", "正在启动 Agent…");
    tokio::task::yield_now().await;

    let providers = db.get_enabled_providers()?;
    if providers.is_empty() {
        return Err(AppError::from("未配置可用的模型服务"));
    }

    let ordered = order_providers(providers, input.provider_id.as_deref());
    let primary_id = ordered.first().map(|p| p.id.clone());
    let workspace = db
        .resolve_session_workspace(session_id)?
        .map(std::path::PathBuf::from);

    let project_ctx = if let Some(ws) = workspace.clone() {
        emit_agent_phase(app, session_id, "project-ctx", "正在加载项目上下文…");
        tokio::task::yield_now().await;
        tokio::task::spawn_blocking(move || load_project_context(&ws).ok())
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    let mut loop_turns = input.loop_turns;
    let user_text = input.user_text;
    let auto_failover = input.auto_failover;
    let resume_provider_id = input.provider_id.clone();
    let mut errors: Vec<String> = Vec::new();
    let shell_policy = settings.shell_policy();

    let plan_skills = if input.plan_mode {
        emit_agent_phase(app, session_id, "plan-skills", "正在加载 Plan 技能…");
        tokio::task::yield_now().await;
        tokio::task::spawn_blocking(load_plan_mode_superpowers_skills)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mcp_servers: Vec<_> = db
        .list_mcp_servers()?
        .iter()
        .filter_map(|row| crate::mcp::McpServerRecord::from_row(row).ok())
        .collect();
    if !input.plan_mode && mcp_servers.iter().any(|s| s.enabled) {
        emit_agent_phase(app, session_id, "mcp", "正在连接 MCP 服务…");
        tokio::task::yield_now().await;
    }
    let _ = state.mcp.refresh_from_db(&mcp_servers);
    let mcp_openai = if input.plan_mode {
        Vec::new()
    } else {
        state.mcp.openai_tool_definitions()
    };
    let mcp_anthropic = if input.plan_mode {
        Vec::new()
    } else {
        state.mcp.anthropic_tool_definitions()
    };
    let plan_base_openai = if input.plan_mode {
        Some(tool_schema::plan_mode_openai_tools())
    } else {
        None
    };

    'providers: for (index, provider) in ordered.iter().enumerate() {
        let api_key = match secrets::get_api_key(&provider.id) {
            Ok(key) => key,
            Err(err) => {
                errors.push(format!("{}：{err}", provider.name));
                continue;
            }
        };

        for _iteration in 0..settings.agent_max_iterations {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::from("已取消生成"));
            }

            emit_agent_phase(app, session_id, &format!("ctx-{_iteration}"), "正在准备对话上下文…");
            tokio::task::yield_now().await;

            let ctx = build_context(&db, session_id, settings)?;
            let system = if input.plan_mode {
                build_plan_system_prompt(
                    ctx.summary_prefix.as_deref(),
                    project_ctx.as_ref(),
                    &plan_skills,
                )
            } else {
                build_agent_system_prompt(
                    agent_system_prompt(),
                    ctx.summary_prefix.as_deref(),
                    project_ctx.as_ref(),
                )
            };

            let user_content = if loop_turns.is_empty() {
                user_text.clone()
            } else {
                "请根据上述工具结果继续完成任务，不要向用户提问；完成后交付成品、核心逻辑与 Mermaid 流程图。".to_string()
            };

            let openai = build_openai_agent_messages(
                &ctx.recent,
                &loop_turns,
                &user_content,
                &system,
            );
            let anthropic =
                build_anthropic_agent_messages(&ctx.recent, &loop_turns, &user_content);

            let phase_msg = if loop_turns.is_empty() {
                "正在分析请求…"
            } else {
                "正在根据工具结果继续…"
            };
            emit_agent_phase(
                app,
                session_id,
                &format!("phase-{_iteration}"),
                phase_msg,
            );
            tokio::task::yield_now().await;

            let result = stream_agent_completion(
                &http,
                provider,
                &api_key,
                openai,
                anthropic,
                &system,
                plan_base_openai.as_deref(),
                &mcp_openai,
                &mcp_anthropic,
                app,
                session_id,
                Arc::clone(&cancel),
            )
            .await;

            let model_out = match result {
                Ok(out) => out,
                Err(err) => {
                    let retryable = err.is_retryable();
                    let msg = err.message();
                    if msg == "已取消生成" {
                        return Err(AppError::from(msg));
                    }
                    errors.push(format!("{}：{msg}", provider.name));
                    if !auto_failover || !retryable || index + 1 >= ordered.len() {
                        emit_error(app, session_id, &msg);
                        return Err(AppError::from(if errors.len() == 1 {
                            msg
                        } else {
                            format!("所有模型服务均失败：{}", errors.join("；"))
                        }));
                    }
                    let next = &ordered[index + 1];
                    emit_agent_phase(
                        app,
                        session_id,
                        "failover",
                        &format!("{} 不可用，正在切换至 {}…", provider.name, next.name),
                    );
                    continue 'providers;
                }
            };

            let usage_input = format!("{user_content}\n{system}");
            let usage_output = model_out.text.clone();
            let db_usage = db.clone();
            let pid = provider.id.clone();
            let model_name = provider.default_model.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = crate::providers::usage::record_chat_usage(
                    &db_usage,
                    &pid,
                    &model_name,
                    &usage_input,
                    &usage_output,
                );
            })
            .await;

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
                let response =
                    chat_response(display.clone(), provider, primary_id.as_deref(), index + 1);
                emit_done(app, session_id, &response);
                persist_turn(&db, session_id, &user_text, &display, cancel.load(Ordering::Relaxed))
                    .await?;

                spawn_summary(
                    db.clone(),
                    http.clone(),
                    session_id.to_string(),
                    settings.clone(),
                    provider.clone(),
                    api_key,
                );

                return Ok(response);
            }

            let web_search_api_key = secrets::get_api_key(crate::search::WEB_SEARCH_KEY_ACCOUNT).ok();
            let tool_ctx = ToolContext {
                db: &db,
                session_id,
                workspace: workspace.clone(),
                shell_policy: shell_policy.clone(),
                mcp: if input.plan_mode {
                    None
                } else {
                    Some(&state.mcp)
                },
                http: Some(&http),
                web_search: settings.web_search_config(),
                web_search_api_key,
                readonly: false,
                plan_mode: input.plan_mode,
                semantic_search: settings.semantic_search_config(),
                workspace_policy: settings.workspace_path_policy(),
                bypass_outside_approval: false,
            };

            let mut batch_calls: Vec<AgentToolCall> = Vec::new();
            let mut batch_results: Vec<AgentToolResult> = Vec::new();
            let mut pause: Option<(AgentToolCall, crate::agent::tools::PendingApproval, String)> =
                None;

            for call in &calls {
                if input.plan_mode && call.name == "spawn_task" {
                    emit_tool(app, session_id, call, "error", "plan blocked");
                    batch_calls.push(AgentToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    });
                    batch_results.push(AgentToolResult {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        content: "Plan 模式不允许 spawn_task".into(),
                    });
                    continue;
                }
                if call.name == "spawn_task" {
                    emit_tool(app, session_id, call, "start", "subagent");
                    match subagent::parse_spawn_task_args(&call.arguments) {
                        Ok(sub_input) => {
                            let preview = sub_input.description.chars().take(120).collect::<String>();
                            emit_tool_event(
                                app,
                                session_id,
                                &call.id,
                                &format!("spawn_task:{}", sub_input.subagent_type),
                                "start",
                                &preview,
                            );
                            match subagent::run_subagent(
                                app,
                                state,
                                session_id,
                                sub_input,
                                provider,
                                &api_key,
                                workspace.clone(),
                                Arc::clone(&cancel),
                                settings,
                            )
                            .await
                            {
                                Ok(out) => {
                                    emit_tool(app, session_id, call, "done", &out);
                                    audit_tool(&db, session_id, call, "ok", Some(&out));
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
                                Err(err) => {
                                    emit_tool(app, session_id, call, "error", &err);
                                    audit_tool(&db, session_id, call, "error", Some(&err));
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
                        Err(err) => {
                            emit_tool(app, session_id, call, "error", &err);
                            audit_tool(&db, session_id, call, "error", Some(&err));
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
                    continue;
                }

                let args_preview = format_tool_call_preview(&call.name, &call.arguments);
                emit_tool(app, session_id, call, "start", &args_preview);
                match execute_tool(call, &tool_ctx) {
                    ToolResult::Ok(out) => {
                        let done_preview = format_tool_done_preview(&call.name, &call.arguments, &out);
                        emit_tool(app, session_id, call, "done", &done_preview);
                        audit_tool(&db, session_id, call, "ok", Some(&out));
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
                    ToolResult::NeedsApproval { approval_id, action } => {
                        let preview = match &action {
                            crate::agent::tools::PendingApproval::Shell(c) => c.clone(),
                            crate::agent::tools::PendingApproval::WebFetch(u) => u.clone(),
                            crate::agent::tools::PendingApproval::OutsideRead(p) => p.clone(),
                            crate::agent::tools::PendingApproval::OutsideWrite(p) => p.clone(),
                        };
                        emit_tool(app, session_id, call, "approval", &preview);
                        audit_tool(&db, session_id, call, "pending", Some(&preview));
                        pause = Some((
                            AgentToolCall {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            },
                            action,
                            approval_id,
                        ));
                        break;
                    }
                    ToolResult::Err(err) => {
                        emit_tool(app, session_id, call, "error", &err);
                        audit_tool(&db, session_id, call, "error", Some(&err));
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

            if !batch_calls.is_empty() {
                loop_turns.push(AgentLoopTurn::Assistant {
                    text: display.clone(),
                    tool_calls: batch_calls,
                });
                loop_turns.push(AgentLoopTurn::ToolResults(batch_results));
                // 继续下一轮 LLM
            }

            if let Some((paused_call, action, approval_id)) = pause {
                let (approval_action, approval_payload, content) = match &action {
                    crate::agent::tools::PendingApproval::Shell(command) => (
                        "shell",
                        command.clone(),
                        format!("需要确认执行 Shell 命令：\n```\n{command}\n```"),
                    ),
                    crate::agent::tools::PendingApproval::WebFetch(url) => (
                        "web_fetch",
                        url.clone(),
                        format!("需要确认抓取 URL：\n{url}"),
                    ),
                    crate::agent::tools::PendingApproval::OutsideRead(path) => (
                        "outside_read",
                        path.clone(),
                        format!("需要确认读取工作区外文件：\n{path}"),
                    ),
                    crate::agent::tools::PendingApproval::OutsideWrite(path) => (
                        "outside_write",
                        path.clone(),
                        format!("需要确认写入工作区外路径：\n{path}"),
                    ),
                };
                state.store_pending_agent(
                    session_id,
                    PendingAgentPause {
                        user_text: user_text.clone(),
                        loop_turns: loop_turns.clone(),
                        assistant_text: display.clone(),
                        paused_tool_call: paused_call,
                        approval_action: approval_action.to_string(),
                        approval_payload: approval_payload.clone(),
                        provider_id: resume_provider_id.clone(),
                        auto_failover,
                        plan_mode: input.plan_mode,
                    },
                );
                let response = ChatResponse {
                    content,
                    provider_id: provider.id.clone(),
                    provider_name: provider.name.clone(),
                    failovered: false,
                    attempts: index + 1,
                    agent_paused: true,
                    approval_id: Some(approval_id),
                    pending_action: Some(approval_action.to_string()),
                    pending_command: Some(approval_payload),
                };
                emit_done(app, session_id, &response);
                return Ok(response);
            }

            continue;
        }

        return Err(AppError::from(format!(
            "Agent 达到最大迭代次数（{}）",
            settings.agent_max_iterations
        )));
    }

    Err(AppError::from(format!("Agent 执行失败：{}", errors.join("；"))))
}

async fn persist_turn(
    db: &Arc<Database>,
    session_id: &str,
    user_text: &str,
    content: &str,
    partial: bool,
) -> AppResult<()> {
    let db_clone = Arc::clone(db);
    let sid = session_id.to_string();
    let ut = user_text.to_string();
    let c = content.to_string();
    tokio::task::spawn_blocking(move || append_turn(&db_clone, &sid, &ut, &c, partial))
        .await
        .map_err(|e| AppError::from(format!("保存失败: {e}")))??;
    Ok(())
}

fn spawn_summary(
    db: Arc<Database>,
    http: Client,
    session_id: String,
    settings: AppContextSettings,
    provider: Provider,
    api_key: String,
) {
    tokio::spawn(async move {
        let _ = maybe_update_summary(db, &http, &session_id, &settings, &provider, &api_key).await;
    });
}
