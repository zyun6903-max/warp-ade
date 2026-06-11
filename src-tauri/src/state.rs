use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::agent::history::{AgentLoopTurn, AgentToolCall};
use crate::mcp::McpManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAgentPause {
    pub user_text: String,
    pub loop_turns: Vec<AgentLoopTurn>,
    pub assistant_text: String,
    pub paused_tool_call: AgentToolCall,
    pub approval_action: String,
    pub approval_payload: String,
    pub provider_id: Option<String>,
    pub auto_failover: bool,
    pub plan_mode: bool,
}

pub struct AppState {
    pub db: Arc<crate::storage::db::Database>,
    pub http: Client,
    pub chat_cancel: Arc<AtomicBool>,
    pub mcp: McpManager,
    pending_agent: Mutex<HashMap<String, PendingAgentPause>>,
}

impl AppState {
    pub fn new(db: crate::storage::db::Database) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(20))
            .user_agent("warp-ade/0.1")
            .build()
            .expect("failed to build HTTP client");
        Self {
            db: Arc::new(db),
            http,
            chat_cancel: Arc::new(AtomicBool::new(false)),
            mcp: McpManager::new(),
            pending_agent: Mutex::new(HashMap::new()),
        }
    }

    pub fn reset_chat_cancel(&self) {
        self.chat_cancel.store(false, Ordering::SeqCst);
    }

    pub fn request_chat_cancel(&self) {
        self.chat_cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_chat_cancelled(&self) -> bool {
        self.chat_cancel.load(Ordering::Relaxed)
    }

    pub fn store_pending_agent(&self, session_id: &str, pause: PendingAgentPause) {
        if let Ok(mut map) = self.pending_agent.lock() {
            map.insert(session_id.to_string(), pause);
        }
    }

    pub fn take_pending_agent(&self, session_id: &str) -> Option<PendingAgentPause> {
        self.pending_agent
            .lock()
            .ok()
            .and_then(|mut map| map.remove(session_id))
    }
}
