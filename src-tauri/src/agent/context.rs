use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::providers::chat::complete_openai;
use crate::storage::db::{Database, MessageView, Provider};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContextSettings {
    pub recent_turns: usize,
    pub token_threshold: usize,
    pub summary_enabled: bool,
    pub agent_max_iterations: usize,
    pub agent_enabled_default: bool,
    pub shell_auto_readonly: bool,
    pub shell_always_confirm: bool,
    pub shell_extra_allowlist: String,
    pub web_search_enabled: bool,
    pub web_search_provider: String,
    pub web_search_max_results: usize,
    pub agent_subagent_max_iterations: usize,
    pub semantic_search_enabled: bool,
    pub semantic_search_model: String,
    pub semantic_search_provider_id: String,
    pub semantic_search_max_results: usize,
    pub workspace_outside_read: String,
    pub workspace_outside_write: String,
}

impl Default for AppContextSettings {
    fn default() -> Self {
        Self {
            recent_turns: 12,
            token_threshold: 60_000,
            summary_enabled: true,
            agent_max_iterations: 25,
            agent_enabled_default: false,
            shell_auto_readonly: true,
            shell_always_confirm: false,
            shell_extra_allowlist: String::new(),
            web_search_enabled: false,
            web_search_provider: "brave".to_string(),
            web_search_max_results: 5,
            agent_subagent_max_iterations: 12,
            semantic_search_enabled: false,
            semantic_search_model: "text-embedding-3-small".to_string(),
            semantic_search_provider_id: String::new(),
            semantic_search_max_results: 8,
            workspace_outside_read: "block".to_string(),
            workspace_outside_write: "block".to_string(),
        }
    }
}

impl AppContextSettings {
    pub fn shell_policy(&self) -> crate::agent::shell_policy::ShellPolicyConfig {
        crate::agent::shell_policy::ShellPolicyConfig {
            auto_readonly: self.shell_auto_readonly,
            always_confirm: self.shell_always_confirm,
            extra_allowlist: self
                .shell_extra_allowlist
                .lines()
                .map(|l| l.trim().to_lowercase())
                .filter(|l| !l.is_empty())
                .collect(),
        }
    }

    pub fn web_search_config(&self) -> crate::search::WebSearchConfig {
        crate::search::WebSearchConfig {
            enabled: self.web_search_enabled,
            provider: self.web_search_provider.clone(),
            max_results: self.web_search_max_results.max(1).min(20),
        }
    }

    pub fn semantic_search_config(&self) -> crate::search::SemanticSearchConfig {
        crate::search::SemanticSearchConfig::from_settings(self)
    }

    pub fn workspace_path_policy(&self) -> crate::agent::workspace_policy::WorkspacePathPolicy {
        crate::agent::workspace_policy::WorkspacePathPolicy::from_settings(
            &self.workspace_outside_read,
            &self.workspace_outside_write,
        )
    }
}

#[derive(Debug, Clone)]
pub struct BuiltContext {
    pub recent: Vec<MessageView>,
    pub summary_prefix: Option<String>,
    pub estimated_tokens: usize,
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

fn message_text(msg: &MessageView) -> String {
    msg.parts
        .iter()
        .filter_map(|p| p.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn estimate_messages_tokens(messages: &[MessageView]) -> usize {
    messages.iter().map(|m| estimate_tokens(&message_text(m))).sum()
}

pub fn build_context(
    db: &Database,
    session_id: &str,
    settings: &AppContextSettings,
) -> AppResult<BuiltContext> {
    let all = db.list_messages(session_id)?;
    let estimated_tokens = estimate_messages_tokens(&all);
    let summary_node = db.get_latest_context_node(session_id)?;
    let summary_prefix = summary_node.map(|n| {
        format!(
            "以下是较早对话的滚动摘要（seq {}–{}）：\n{}",
            n.covers_seq_start, n.covers_seq_end, n.summary
        )
    });

    let keep = settings.recent_turns.saturating_mul(2).max(4);
    let recent = if all.len() <= keep {
        all
    } else {
        all[all.len() - keep..].to_vec()
    };

    Ok(BuiltContext {
        recent,
        summary_prefix,
        estimated_tokens,
    })
}

pub fn load_context_settings(db: &Database) -> AppResult<AppContextSettings> {
    let mut s = AppContextSettings::default();
    if let Some(v) = db.get_setting("context_recent_turns")? {
        s.recent_turns = v.parse().unwrap_or(s.recent_turns);
    }
    if let Some(v) = db.get_setting("context_token_threshold")? {
        s.token_threshold = v.parse().unwrap_or(s.token_threshold);
    }
    if let Some(v) = db.get_setting("context_summary_enabled")? {
        s.summary_enabled = v == "true" || v == "1";
    }
    if let Some(v) = db.get_setting("agent_max_iterations")? {
        s.agent_max_iterations = v.parse().unwrap_or(s.agent_max_iterations);
    }
    if let Some(v) = db.get_setting("agent_enabled_default")? {
        s.agent_enabled_default = v == "true" || v == "1";
    }
    if let Some(v) = db.get_setting("shell_auto_readonly")? {
        s.shell_auto_readonly = v == "true" || v == "1";
    }
    if let Some(v) = db.get_setting("shell_always_confirm")? {
        s.shell_always_confirm = v == "true" || v == "1";
    }
    if let Some(v) = db.get_setting("shell_extra_allowlist")? {
        s.shell_extra_allowlist = v;
    }
    if let Some(v) = db.get_setting("web_search_enabled")? {
        s.web_search_enabled = v == "true" || v == "1";
    }
    if let Some(v) = db.get_setting("web_search_provider")? {
        if !v.trim().is_empty() {
            s.web_search_provider = v;
        }
    }
    if let Some(v) = db.get_setting("web_search_max_results")? {
        s.web_search_max_results = v.parse().unwrap_or(s.web_search_max_results);
    }
    if let Some(v) = db.get_setting("agent_subagent_max_iterations")? {
        s.agent_subagent_max_iterations = v.parse().unwrap_or(s.agent_subagent_max_iterations);
    }
    if let Some(v) = db.get_setting("semantic_search_enabled")? {
        s.semantic_search_enabled = v == "true" || v == "1";
    }
    if let Some(v) = db.get_setting("semantic_search_model")? {
        if !v.trim().is_empty() {
            s.semantic_search_model = v;
        }
    }
    if let Some(v) = db.get_setting("semantic_search_provider_id")? {
        s.semantic_search_provider_id = v;
    }
    if let Some(v) = db.get_setting("semantic_search_max_results")? {
        s.semantic_search_max_results = v.parse().unwrap_or(s.semantic_search_max_results);
    }
    if let Some(v) = db.get_setting("workspace_outside_read")? {
        if !v.trim().is_empty() {
            s.workspace_outside_read = v;
        }
    }
    if let Some(v) = db.get_setting("workspace_outside_write")? {
        if !v.trim().is_empty() {
            s.workspace_outside_write = v;
        }
    }
    Ok(s)
}

pub fn save_context_settings(db: &Database, settings: &AppContextSettings) -> AppResult<()> {
    db.set_setting("context_recent_turns", &settings.recent_turns.to_string())?;
    db.set_setting("context_token_threshold", &settings.token_threshold.to_string())?;
    db.set_setting(
        "context_summary_enabled",
        if settings.summary_enabled { "true" } else { "false" },
    )?;
    db.set_setting("agent_max_iterations", &settings.agent_max_iterations.to_string())?;
    db.set_setting(
        "agent_enabled_default",
        if settings.agent_enabled_default { "true" } else { "false" },
    )?;
    db.set_setting(
        "shell_auto_readonly",
        if settings.shell_auto_readonly { "true" } else { "false" },
    )?;
    db.set_setting(
        "shell_always_confirm",
        if settings.shell_always_confirm { "true" } else { "false" },
    )?;
    db.set_setting("shell_extra_allowlist", &settings.shell_extra_allowlist)?;
    db.set_setting(
        "web_search_enabled",
        if settings.web_search_enabled { "true" } else { "false" },
    )?;
    db.set_setting("web_search_provider", &settings.web_search_provider)?;
    db.set_setting(
        "web_search_max_results",
        &settings.web_search_max_results.max(1).min(20).to_string(),
    )?;
    db.set_setting(
        "agent_subagent_max_iterations",
        &settings.agent_subagent_max_iterations.max(1).min(30).to_string(),
    )?;
    db.set_setting(
        "semantic_search_enabled",
        if settings.semantic_search_enabled {
            "true"
        } else {
            "false"
        },
    )?;
    db.set_setting("semantic_search_model", &settings.semantic_search_model)?;
    db.set_setting(
        "semantic_search_provider_id",
        &settings.semantic_search_provider_id,
    )?;
    db.set_setting(
        "semantic_search_max_results",
        &settings
            .semantic_search_max_results
            .max(1)
            .min(20)
            .to_string(),
    )?;
    db.set_setting("workspace_outside_read", &settings.workspace_outside_read)?;
    db.set_setting("workspace_outside_write", &settings.workspace_outside_write)?;
    Ok(())
}

pub async fn maybe_update_summary(
    db: Arc<Database>,
    http: &Client,
    session_id: &str,
    settings: &AppContextSettings,
    provider: &Provider,
    api_key: &str,
) -> AppResult<()> {
    if !settings.summary_enabled {
        return Ok(());
    }

    let all = db.list_messages(&session_id)?;
    if all.is_empty() {
        return Ok(());
    }

    let total_tokens = estimate_messages_tokens(&all);
    if total_tokens < settings.token_threshold {
        return Ok(());
    }

    let keep = settings.recent_turns.saturating_mul(2).max(4);
    if all.len() <= keep {
        return Ok(());
    }

    let old = &all[..all.len() - keep];
    let seq_start = old.first().map(|m| m.seq).unwrap_or(0);
    let seq_end = old.last().map(|m| m.seq).unwrap_or(0);

    if let Some(node) = db.get_latest_context_node(session_id)? {
        if node.covers_seq_end >= seq_end {
            return Ok(());
        }
    }

    let transcript = old
        .iter()
        .map(|m| format!("[{}] {}", m.role, message_text(m)))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "请将以下对话历史压缩为简洁的中文摘要，保留关键决策、文件路径、错误与结论。不超过 800 字。\n\n{transcript}"
    );

    let summary = complete_openai(http, provider, api_key, &[], &prompt)
        .await
        .map_err(|e| AppError::from(e.message()))?;

    let token_count = estimate_tokens(&summary) as i64;
    db.insert_context_node(
        session_id,
        &summary,
        token_count,
        seq_start,
        seq_end,
    )?;
    Ok(())
}
