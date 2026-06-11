use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolEvent {
    pub session_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub status: String,
    pub preview: String,
}

pub const PHASE_TOOL: &str = "__phase__";

pub fn emit_agent_phase(app: &AppHandle, session_id: &str, phase_id: &str, message: &str) {
    emit_tool_event(
        app,
        session_id,
        phase_id,
        PHASE_TOOL,
        "running",
        message,
    );
}

pub fn emit_tool_event(
    app: &AppHandle,
    session_id: &str,
    call_id: &str,
    tool_name: &str,
    status: &str,
    preview: &str,
) {
    let _ = app.emit(
        "agent-tool",
        AgentToolEvent {
            session_id: session_id.to_string(),
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            status: status.to_string(),
            preview: preview.chars().take(1200).collect(),
        },
    );
}
