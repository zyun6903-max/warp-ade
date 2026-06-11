use serde::Deserialize;
use tauri::{Emitter, State};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::import::codex::{import_codex_file, scan_codex_rollouts, search_codex_rollouts, CodexImportCandidate};
use crate::import::claude::{
    import_claude_file, scan_claude_transcripts, search_claude_transcripts, ClaudeImportCandidate,
};
use crate::import::cursor::{import_cursor_file, scan_cursor_transcripts, search_cursor_transcripts, CursorImportCandidate};
use crate::import::search::ImportSourceSearchHit;
use crate::providers::chat::{send_chat, ChatResponse};
use crate::providers::test_connection::{test_provider_connection, TestProviderResult};
use crate::state::AppState;
use crate::storage::db::{MessageView, Project, Provider, Session, SessionSearchHit};
use crate::workspace::{
    checkout_branch, commit_changes, ensure_workspace_directory, inspect_workspace, pick_directory,
    push_branch, FileDiffResult, WorkspaceInfo,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub api_format: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub priority: Option<i64>,
    pub enabled: bool,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProviderInput {
    pub provider_id: Option<String>,
    pub base_url: String,
    pub api_format: String,
    pub default_model: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<Project>> {
    state.db.list_projects()
}

#[tauri::command]
pub fn pick_workspace_directory(title: Option<String>) -> AppResult<Option<String>> {
    let title = title.unwrap_or_else(|| "选择文件夹".to_string());
    pick_directory(&title)
}

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    name: String,
    workspace_path: Option<String>,
) -> AppResult<Project> {
    let resolved = workspace_path
        .as_deref()
        .map(ensure_workspace_directory)
        .transpose()?;
    let path_str = resolved.as_ref().map(|p| p.to_string_lossy().into_owned());
    state.db.create_project(&name, path_str.as_deref())
}

#[tauri::command]
pub fn list_sessions(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> AppResult<Vec<Session>> {
    state.db.list_sessions(project_id.as_deref())
}

#[tauri::command]
pub fn create_session(
    state: State<'_, AppState>,
    title: Option<String>,
    project_id: Option<String>,
) -> AppResult<Session> {
    state.db.create_session(
        title.as_deref().unwrap_or("新对话"),
        "native",
        None,
        None,
        None,
        project_id.as_deref(),
        None,
    )
}

#[tauri::command]
pub fn delete_session(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    state.db.delete_session(&session_id)
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, project_id: String) -> AppResult<()> {
    state.db.delete_project(&project_id)
}

#[tauri::command]
pub fn rename_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> AppResult<Session> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(AppError::from("标题不能为空"));
    }
    state.db.update_session_title(&session_id, trimmed)?;
    state
        .db
        .get_session(&session_id)?
        .ok_or_else(|| AppError::from("会话不存在"))
}

#[tauri::command]
pub fn save_session_to_workspace(
    state: State<'_, AppState>,
    session_id: String,
    workspace_path: String,
) -> AppResult<Session> {
    let resolved = ensure_workspace_directory(&workspace_path)?;
    state
        .db
        .save_session_to_workspace(&session_id, &resolved.to_string_lossy())
}

#[tauri::command]
pub fn save_project_to_workspace(
    state: State<'_, AppState>,
    project_id: String,
    workspace_path: String,
) -> AppResult<Project> {
    let resolved = ensure_workspace_directory(&workspace_path)?;
    state
        .db
        .save_project_to_workspace(&project_id, &resolved.to_string_lossy())
}

#[tauri::command]
pub fn search_sessions(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
    project_id: Option<String>,
    source: Option<String>,
) -> AppResult<Vec<SessionSearchHit>> {
    state.db.search_sessions(
        &query,
        limit,
        project_id.as_deref(),
        source.as_deref(),
    )
}

#[tauri::command]
pub fn get_messages(state: State<'_, AppState>, session_id: String) -> AppResult<Vec<MessageView>> {
    state.db.list_messages_for_chat(&session_id)
}

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> AppResult<Vec<Provider>> {
    state.db.list_providers()
}

#[tauri::command]
pub fn save_provider(state: State<'_, AppState>, input: SaveProviderInput) -> AppResult<Provider> {
    let is_new = input.id.is_none();
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let key_to_store = input.api_key.filter(|k| !k.trim().is_empty());

    if let Some(key) = key_to_store.as_ref() {
        crate::secrets::store_api_key(&id, key.trim())?;
        if !crate::secrets::has_api_key(&id)? {
            return Err("API Key 保存失败，请重试".into());
        }
    } else if is_new {
        return Err("新建服务必须填写 API Key".into());
    }

    let has_key = crate::secrets::has_api_key(&id)?;

    let priority = if let Some(priority) = input.priority {
        priority
    } else if is_new {
        state.db.next_provider_priority()?
    } else {
        state
            .db
            .list_providers()?
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.priority)
            .unwrap_or(1)
    };

    let provider = Provider {
        id: id.clone(),
        name: input.name,
        base_url: input.base_url,
        api_format: input.api_format,
        models: input.models,
        default_model: input.default_model,
        priority,
        enabled: input.enabled,
        has_key,
    };
    state.db.upsert_provider(&provider)?;
    Ok(provider)
}

#[tauri::command]
pub fn delete_provider(state: State<'_, AppState>, provider_id: String) -> AppResult<()> {
    state.db.delete_provider(&provider_id)
}

#[tauri::command]
pub fn duplicate_provider(state: State<'_, AppState>, provider_id: String) -> AppResult<Provider> {
    let source = state
        .db
        .list_providers()?
        .into_iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| AppError::from("找不到模型服务"))?;

    let id = Uuid::new_v4().to_string();
    if crate::secrets::has_api_key(&source.id)? {
        let key = crate::secrets::get_api_key(&source.id)?;
        crate::secrets::store_api_key(&id, key.trim())?;
    }

    let has_key = crate::secrets::has_api_key(&id)?;
    let priority = state.db.next_provider_priority()?;

    let provider = Provider {
        id,
        name: format!("{} 副本", source.name),
        base_url: source.base_url,
        api_format: source.api_format,
        models: source.models.clone(),
        default_model: source.default_model,
        priority,
        enabled: source.enabled,
        has_key,
    };
    state.db.upsert_provider(&provider)?;
    Ok(provider)
}

#[tauri::command]
pub fn has_api_key(provider_id: String) -> AppResult<bool> {
    Ok(crate::secrets::has_api_key(&provider_id)?)
}

#[tauri::command]
pub fn delete_api_key(provider_id: String) -> AppResult<()> {
    crate::secrets::delete_api_key(&provider_id)?;
    Ok(())
}

#[tauri::command]
pub fn scan_cursor_imports(state: State<'_, AppState>) -> AppResult<Vec<CursorImportCandidate>> {
    scan_cursor_transcripts(&state.db)
}

#[tauri::command]
pub fn import_cursor_session(
    state: State<'_, AppState>,
    source_path: String,
) -> AppResult<Session> {
    import_cursor_file(&state.db, &source_path)
}

#[tauri::command]
pub fn scan_codex_imports(state: State<'_, AppState>) -> AppResult<Vec<CodexImportCandidate>> {
    scan_codex_rollouts(&state.db)
}

#[tauri::command]
pub fn import_codex_session(
    state: State<'_, AppState>,
    source_path: String,
) -> AppResult<Session> {
    import_codex_file(&state.db, &source_path)
}

#[tauri::command]
pub fn scan_claude_imports(state: State<'_, AppState>) -> AppResult<Vec<ClaudeImportCandidate>> {
    scan_claude_transcripts(&state.db)
}

#[tauri::command]
pub fn import_claude_session(
    state: State<'_, AppState>,
    source_path: String,
) -> AppResult<Session> {
    import_claude_file(&state.db, &source_path)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportResult {
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgressEvent {
    pub current: usize,
    pub total: usize,
    pub source_path: String,
    pub status: String,
    pub done: bool,
}

#[tauri::command]
pub fn batch_import_sessions(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source: String,
    source_paths: Vec<String>,
) -> AppResult<BatchImportResult> {
    let total = source_paths.len();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    for (index, path) in source_paths.into_iter().enumerate() {
        let current = index + 1;
        let _ = app.emit(
            "import-progress",
            ImportProgressEvent {
                current,
                total,
                source_path: path.clone(),
                status: "importing".to_string(),
                done: false,
            },
        );
        let result = match source.as_str() {
            "cursor" => import_cursor_file(&state.db, &path),
            "claude" => import_claude_file(&state.db, &path),
            "codex" => import_codex_file(&state.db, &path),
            other => return Err(AppError::from(format!("不支持的导入来源: {other}"))),
        };
        match result {
            Ok(_) => {
                imported += 1;
                let _ = app.emit(
                    "import-progress",
                    ImportProgressEvent {
                        current,
                        total,
                        source_path: path,
                        status: "imported".to_string(),
                        done: false,
                    },
                );
            }
            Err(_) => {
                skipped += 1;
                let _ = app.emit(
                    "import-progress",
                    ImportProgressEvent {
                        current,
                        total,
                        source_path: path,
                        status: "skipped".to_string(),
                        done: false,
                    },
                );
            }
        }
    }
    let _ = app.emit(
        "import-progress",
        ImportProgressEvent {
            current: total,
            total,
            source_path: String::new(),
            status: "complete".to_string(),
            done: true,
        },
    );
    Ok(BatchImportResult { imported, skipped })
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAttachmentInput {
    pub session_id: String,
    pub workspace_path: Option<String>,
    pub file_name: String,
    pub data_base64: String,
}

#[tauri::command]
pub fn save_chat_attachment(input: SaveAttachmentInput) -> AppResult<crate::attachments::ChatAttachment> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(input.data_base64.trim())
        .map_err(|e| AppError::from(format!("附件解码失败: {e}")))?;
    crate::attachments::save_attachment(
        &input.session_id,
        input.workspace_path.as_deref(),
        &input.file_name,
        &data,
    )
}

#[tauri::command]
pub fn get_session(state: State<'_, AppState>, session_id: String) -> AppResult<Option<Session>> {
    state.db.get_session(&session_id)
}

#[tauri::command]
pub fn get_session_by_source(
    state: State<'_, AppState>,
    source_path: String,
) -> AppResult<Option<Session>> {
    state.db.get_session_by_source_path(&source_path)
}

#[tauri::command]
pub async fn search_import_sources(
    state: State<'_, AppState>,
    source: String,
    query: String,
    limit: Option<usize>,
) -> AppResult<Vec<ImportSourceSearchHit>> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || match source.as_str() {
        "cursor" => search_cursor_transcripts(&db, &query, limit),
        "claude" | "claude_code" => search_claude_transcripts(&db, &query, limit),
        "codex" => search_codex_rollouts(&db, &query, limit),
        _ => Err(AppError::from(format!("unsupported import source: {source}"))),
    })
    .await
    .map_err(|err| AppError::from(format!("search task failed: {err}")))?
}

#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppState>,
    input: TestProviderInput,
) -> AppResult<TestProviderResult> {
    let api_key = if let Some(key) = input.api_key.filter(|k| !k.trim().is_empty()) {
        key.trim().to_string()
    } else if let Some(id) = input.provider_id.as_ref() {
        crate::secrets::get_api_key(id)?
    } else {
        return Err(AppError::from("请填写 API Key 或选择已保存的服务"));
    };

    let model = input
        .model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| input.default_model.clone());

    let provider = Provider {
        id: input.provider_id.clone().unwrap_or_else(|| "test".to_string()),
        name: "test".to_string(),
        base_url: input.base_url,
        api_format: input.api_format,
        models: vec![model.clone()],
        default_model: model.clone(),
        priority: 1,
        enabled: true,
        has_key: true,
    };

    let result = test_provider_connection(&state.http, &provider, &api_key, &model).await;

    if result.ok {
        if let Some(id) = input.provider_id {
            let db = state.db.clone();
            let pid = id.clone();
            let m = model.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                crate::providers::usage::record_test_usage(&db, &pid, &m)
            })
            .await;
        }
    }

    Ok(result)
}

#[tauri::command]
pub fn list_provider_usage(state: State<'_, AppState>) -> AppResult<Vec<crate::storage::db::ProviderUsageRow>> {
    crate::providers::usage::list_usage_stats(&state.db)
}

#[tauri::command]
pub fn reorder_providers(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<Vec<Provider>> {
    state.db.reorder_providers(&ids)?;
    state.db.list_providers()
}

#[tauri::command]
pub fn cancel_chat_generation(state: State<'_, AppState>) -> AppResult<()> {
    state.request_chat_cancel();
    Ok(())
}

#[tauri::command]
pub fn export_session_markdown(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<String> {
    crate::export::session_to_markdown(&state.db, &session_id)
}

#[tauri::command]
pub fn continue_from_import(
    state: State<'_, AppState>,
    imported_session_id: String,
) -> AppResult<Session> {
    let imported = state
        .db
        .get_session(&imported_session_id)?
        .ok_or_else(|| AppError::from("找不到导入会话"))?;
    state.db.create_session(
        &format!("延续 · {}", imported.title),
        "native",
        imported.source_path.as_deref(),
        imported.project_slug.as_deref(),
        imported.workspace_path.as_deref(),
        imported.project_id.as_deref(),
        Some(&imported_session_id),
    )
}

#[tauri::command]
pub async fn send_message(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    content: String,
    provider_id: Option<String>,
    auto_failover: Option<bool>,
    agent_mode: Option<bool>,
    plan_mode: Option<bool>,
) -> AppResult<ChatResponse> {
    let auto_failover = auto_failover.unwrap_or(true);
    state.reset_chat_cancel();
    let settings = crate::agent::load_context_settings(&state.db)?;
    let plan = plan_mode.unwrap_or(false);
    let use_agent = agent_mode.unwrap_or(settings.agent_enabled_default) || plan;
    if use_agent {
        return crate::agent::run_agent_turn(
            &app,
            &state,
            &session_id,
            &content,
            provider_id.as_deref(),
            auto_failover,
            state.chat_cancel.clone(),
            &settings,
            plan,
        )
        .await;
    }
    send_chat(
        &app,
        state.db.clone(),
        &state.http,
        &session_id,
        &content,
        provider_id.as_deref(),
        auto_failover,
        state.chat_cancel.clone(),
    )
    .await
}

#[tauri::command]
pub fn get_context_settings(state: State<'_, AppState>) -> AppResult<crate::agent::AppContextSettings> {
    crate::agent::load_context_settings(&state.db)
}

#[tauri::command]
pub fn save_context_settings_cmd(
    state: State<'_, AppState>,
    settings: crate::agent::AppContextSettings,
) -> AppResult<()> {
    crate::agent::save_context_settings(&state.db, &settings)
}

#[tauri::command]
pub fn list_shell_audit_log(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> AppResult<Vec<crate::storage::db::ShellLogEntry>> {
    state.db.list_shell_logs(limit.unwrap_or(50).min(200))
}

#[tauri::command]
pub async fn execute_agent_shell(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    command: String,
    approved: bool,
    action: Option<String>,
) -> AppResult<ChatResponse> {
    use crate::agent::tools::{execute_paused_tool_call, ToolContext, ToolResult};

    state.reset_chat_cancel();
    let settings = crate::agent::load_context_settings(&state.db)?;
    let action = action.unwrap_or_else(|| "shell".to_string());

    let pending = state
        .take_pending_agent(&session_id)
        .ok_or_else(|| AppError::from("没有待恢复的 Agent 任务"))?;

    let workspace = state
        .db
        .resolve_session_workspace(&session_id)?
        .map(std::path::PathBuf::from);
    let skill_catalog: Vec<crate::agent::project_context::SkillEntry> = workspace
        .as_ref()
        .and_then(|ws| crate::agent::project_context::load_project_context(ws).ok())
        .map(|c| c.skills)
        .unwrap_or_default();
    let web_search_api_key =
        crate::secrets::get_api_key(crate::search::WEB_SEARCH_KEY_ACCOUNT).ok();

    let tool_output = if approved {
        match action.as_str() {
            "outside_read" | "outside_write" => {
                let tool_ctx = ToolContext {
                    db: &state.db,
                    session_id: &session_id,
                    workspace: workspace.clone(),
                    shell_policy: settings.shell_policy(),
                    mcp: Some(&state.mcp),
                    http: Some(&state.http),
                    web_search: settings.web_search_config(),
                    web_search_api_key,
                    readonly: false,
                    plan_mode: pending.plan_mode,
                    semantic_search: settings.semantic_search_config(),
                    workspace_policy: settings.workspace_path_policy(),
                    bypass_outside_approval: true,
                    skill_catalog: &skill_catalog,
                };
                match execute_paused_tool_call(&pending.paused_tool_call, &tool_ctx) {
                    ToolResult::Ok(out) => out,
                    ToolResult::Err(e) => e,
                    ToolResult::NeedsApproval { .. } => {
                        return Err(AppError::from("工作区外操作仍需确认"));
                    }
                }
            }
            "web_fetch" => {
                crate::agent::tools::fetch_web(&state.http, &command)
                    .await
                    .map_err(AppError::from)?
            }
            _ => {
                match crate::agent::tools::run_shell_with_log(
                    &state.db,
                    &session_id,
                    &command,
                    workspace.as_deref(),
                    "approved",
                ) {
                    ToolResult::Ok(out) => out,
                    ToolResult::Err(e) => return Err(AppError::from(e)),
                    ToolResult::NeedsApproval { .. } => {
                        return Err(AppError::from("命令仍需确认"));
                    }
                }
            }
        }
    } else {
        match action.as_str() {
            "web_fetch" => format!("用户拒绝抓取 URL：\n{command}"),
            "outside_read" => format!("用户拒绝读取工作区外文件：\n{command}"),
            "outside_write" => format!("用户拒绝写入工作区外路径：\n{command}"),
            _ => {
                crate::agent::tools::log_shell_rejection(&state.db, &session_id, &command);
                format!("用户拒绝执行命令：\n{command}")
            }
        }
    };

    let tool_name = match action.as_str() {
        "web_fetch" => "web_fetch".to_string(),
        "outside_read" | "outside_write" => pending.paused_tool_call.name.clone(),
        _ => "run_command".to_string(),
    };
    let mode = if approved { "approved" } else { "rejected" };
    crate::agent::audit::log_tool_audit(
        &state.db,
        &session_id,
        &tool_name,
        mode,
        &command,
        Some(&tool_output),
    );

    crate::agent::resume_agent_with_pending(
        &app,
        &state,
        &session_id,
        pending,
        tool_output,
        state.chat_cancel.clone(),
        &settings,
    )
    .await
}

#[tauri::command]
pub fn list_tool_audit_log(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> AppResult<Vec<crate::storage::db::ToolAuditEntry>> {
    state.db.list_tool_audit_logs(limit.unwrap_or(50).min(200))
}

#[tauri::command]
pub fn get_app_info() -> AppResult<serde_json::Value> {
    Ok(serde_json::json!({
        "name": "warp-ade",
        "version": env!("CARGO_PKG_VERSION"),
        "dataDir": crate::storage::db::app_data_dir().ok().map(|p| p.to_string_lossy().to_string()),
    }))
}

#[tauri::command]
pub fn get_project_context(
    state: State<'_, AppState>,
    session_id: String,
) -> AppResult<Option<crate::agent::project_context::ProjectContextBundle>> {
    let ws = state.db.resolve_session_workspace(&session_id)?;
    match ws {
        Some(path) if !path.trim().is_empty() => {
            Ok(Some(crate::agent::project_context::load_project_context(
                std::path::Path::new(&path),
            )?))
        }
        _ => Ok(None),
    }
}

#[tauri::command]
pub fn get_git_file_diff(workspace_path: String, file_path: String) -> AppResult<FileDiffResult> {
    crate::workspace::file_diff(&workspace_path, &file_path)
}

#[tauri::command]
pub fn get_workspace_info(
    workspace_path: Option<String>,
    source: Option<String>,
) -> AppResult<WorkspaceInfo> {
    Ok(inspect_workspace(
        workspace_path.as_deref(),
        source.as_deref(),
    ))
}

#[tauri::command]
pub fn checkout_git_branch(workspace_path: String, branch: String) -> AppResult<()> {
    checkout_branch(&workspace_path, &branch)
}

#[tauri::command]
pub fn commit_git_changes(workspace_path: String, message: String) -> AppResult<()> {
    commit_changes(&workspace_path, &message)
}

#[tauri::command]
pub fn push_git_branch(workspace_path: String) -> AppResult<()> {
    push_branch(&workspace_path)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMcpServerInput {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub enabled: bool,
}

#[tauri::command]
pub fn list_all_skills(workspace_path: Option<String>) -> AppResult<Vec<crate::agent::skills_registry::SkillListItem>> {
    let workspace = workspace_path
        .filter(|p| !p.trim().is_empty())
        .map(std::path::PathBuf::from);
    crate::agent::skills_registry::list_skill_items(workspace.as_deref())
}

#[tauri::command]
pub fn set_skill_enabled(skill_path: String, enabled: bool) -> AppResult<()> {
    crate::agent::skills_registry::set_skill_enabled(&skill_path, enabled)
}

#[tauri::command]
pub fn delete_user_skill(skill_path: String) -> AppResult<()> {
    crate::agent::skills_registry::delete_user_skill_dir(&skill_path)
}

#[tauri::command]
pub fn reveal_skill_path(skill_path: String) -> AppResult<()> {
    crate::agent::skills_registry::reveal_in_file_manager(&skill_path)
}

#[tauri::command]
pub fn get_user_skills_dir() -> AppResult<String> {
    Ok(crate::agent::skills_registry::user_skills_root()?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
pub fn list_mcp_servers(state: State<'_, AppState>) -> AppResult<Vec<crate::mcp::McpServerRecord>> {
    state
        .db
        .list_mcp_servers()?
        .iter()
        .map(crate::mcp::McpServerRecord::from_row)
        .collect()
}

#[tauri::command]
pub fn save_mcp_server(
    state: State<'_, AppState>,
    input: SaveMcpServerInput,
) -> AppResult<crate::mcp::McpServerRecord> {
    let now = chrono::Utc::now().timestamp();
    let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let created_at = state
        .db
        .list_mcp_servers()?
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.created_at)
        .unwrap_or(now);
    let record = crate::mcp::McpServerRecord {
        id,
        name: input.name,
        command: input.command,
        args: input.args,
        env: input.env,
        enabled: input.enabled,
        created_at,
        updated_at: now,
    };
    state.db.upsert_mcp_server(&record.to_row())?;
    state.mcp.invalidate();
    Ok(record)
}

#[tauri::command]
pub fn delete_mcp_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_mcp_server(&id)?;
    state.mcp.invalidate();
    Ok(())
}

#[tauri::command]
pub fn test_mcp_server(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<crate::mcp::McpTestResult> {
    let row = state
        .db
        .list_mcp_servers()?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| AppError::from("找不到 MCP 服务"))?;
    let record = crate::mcp::McpServerRecord::from_row(&row)?;
    Ok(crate::mcp::McpManager::test_server(&record))
}

#[tauri::command]
pub fn import_cursor_mcp_servers(state: State<'_, AppState>) -> AppResult<usize> {
    let imported = crate::mcp::import_cursor_mcp_json()?;
    let mut count = 0;
    for mut server in imported {
        server.id = uuid::Uuid::new_v4().to_string();
        state.db.upsert_mcp_server(&server.to_row())?;
        count += 1;
    }
    state.mcp.invalidate();
    Ok(count)
}

#[tauri::command]
pub fn has_web_search_key() -> AppResult<bool> {
    Ok(crate::secrets::has_api_key(crate::search::WEB_SEARCH_KEY_ACCOUNT)?)
}

#[tauri::command]
pub fn save_web_search_key(key: Option<String>) -> AppResult<()> {
    if let Some(k) = key.filter(|s| !s.trim().is_empty()) {
        crate::secrets::store_api_key(crate::search::WEB_SEARCH_KEY_ACCOUNT, k.trim())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn test_web_search(state: State<'_, AppState>) -> AppResult<String> {
    let settings = crate::agent::load_context_settings(&state.db)?;
    let cfg = settings.web_search_config();
    if !cfg.enabled {
        return Err(AppError::from("请先在设置中启用 Web 搜索"));
    }
    let api_key = crate::secrets::get_api_key(crate::search::WEB_SEARCH_KEY_ACCOUNT)
        .map_err(|_| AppError::from("未配置 Web 搜索 API Key"))?;
    let preview = crate::search::search_async(&state.http, &cfg, &api_key, "Rust programming language")
        .await?;
    Ok(format!(
        "搜索成功，结果预览：\n{}",
        preview.chars().take(500).collect::<String>()
    ))
}

#[tauri::command]
pub fn get_semantic_index_status(
    state: State<'_, AppState>,
    workspace_path: Option<String>,
) -> AppResult<crate::search::CodeIndexStatus> {
    let settings = crate::agent::load_context_settings(&state.db)?;
    let config = settings.semantic_search_config();
    crate::search::index_status(&state.db, &config, workspace_path)
}

#[tauri::command]
pub async fn rebuild_semantic_index(
    state: State<'_, AppState>,
    workspace_path: String,
) -> AppResult<crate::search::CodeIndexStatus> {
    let settings = crate::agent::load_context_settings(&state.db)?;
    let config = settings.semantic_search_config();
    let path = std::path::PathBuf::from(&workspace_path);
    if !path.is_dir() {
        return Err(AppError::from("工作区路径无效"));
    }
    crate::search::rebuild_index_async(&state.http, &state.db, &path, &config).await
}
