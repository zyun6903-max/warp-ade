mod attachments;
mod platform;
mod search;
mod agent;
mod commands;
mod error;
mod export;
mod import;
mod mcp;
mod providers;
mod secrets;
mod state;
mod storage;
mod workspace;

use tauri::Manager;

use state::AppState;
use storage::db::{app_data_dir, open_db};

fn bootstrap_app(db: &storage::db::Database) -> crate::error::AppResult<()> {
    mcp::ensure_builtin_mcp_servers(db)?;
    if !secrets::has_api_key(search::WEB_SEARCH_KEY_ACCOUNT)? {
        if let Ok(key) = std::env::var("BRAVE_API_KEY") {
            let key = key.trim();
            if !key.is_empty() {
                secrets::store_api_key(search::WEB_SEARCH_KEY_ACCOUNT, key)?;
                let _ = db.set_setting("web_search_provider", "brave");
            }
        } else if let Ok(key) = std::env::var("TAVILY_API_KEY") {
            let key = key.trim();
            if !key.is_empty() {
                secrets::store_api_key(search::WEB_SEARCH_KEY_ACCOUNT, key)?;
                let _ = db.set_setting("web_search_provider", "tavily");
            }
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app_data_dir().expect("failed to resolve app data directory");
            let db = open_db(&data_dir).expect("failed to open database");
            if let Err(e) = bootstrap_app(&db) {
                eprintln!("bootstrap: {e}");
            }
            app.manage(AppState::new(db));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::pick_workspace_directory,
            commands::create_project,
            commands::list_sessions,
            commands::create_session,
            commands::delete_session,
            commands::delete_project,
            commands::rename_session,
            commands::save_session_to_workspace,
            commands::save_project_to_workspace,
            commands::search_sessions,
            commands::get_messages,
            commands::list_providers,
            commands::save_provider,
            commands::delete_provider,
            commands::duplicate_provider,
            commands::reorder_providers,
            commands::has_api_key,
            commands::delete_api_key,
            commands::scan_cursor_imports,
            commands::import_cursor_session,
            commands::scan_claude_imports,
            commands::import_claude_session,
            commands::batch_import_sessions,
            commands::save_chat_attachment,
            commands::get_attachment_data_url,
            commands::scan_codex_imports,
            commands::import_codex_session,
            commands::get_session_by_source,
            commands::get_session,
            commands::search_import_sources,
            commands::test_provider,
            commands::list_provider_usage,
            commands::get_context_settings,
            commands::save_context_settings_cmd,
            commands::list_shell_audit_log,
            commands::list_tool_audit_log,
            commands::execute_agent_shell,
            commands::cancel_chat_generation,
            commands::export_session_markdown,
            commands::continue_from_import,
            commands::send_message,
            commands::get_app_info,
            commands::get_git_file_diff,
            commands::get_project_context,
            commands::get_workspace_info,
            commands::checkout_git_branch,
            commands::commit_git_changes,
            commands::push_git_branch,
            commands::list_all_skills,
            commands::set_skill_enabled,
            commands::delete_user_skill,
            commands::reveal_skill_path,
            commands::get_user_skills_dir,
            commands::list_mcp_servers,
            commands::save_mcp_server,
            commands::delete_mcp_server,
            commands::test_mcp_server,
            commands::import_cursor_mcp_servers,
            commands::has_web_search_key,
            commands::save_web_search_key,
            commands::test_web_search,
            commands::get_semantic_index_status,
            commands::rebuild_semantic_index,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
