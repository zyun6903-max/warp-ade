use std::sync::Arc;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::agent::context::{build_context, load_context_settings, maybe_update_summary};
use crate::error::{AppError, AppResult};
use crate::secrets;
use crate::storage::db::{CanonicalMessage, Database, MessagePart, Provider};

use super::stream::{
    build_anthropic_messages_from_context, build_openai_messages_from_context, emit_done, emit_error,
    stream_anthropic_chat, stream_openai_chat,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub content: String,
    pub provider_id: String,
    pub provider_name: String,
    pub failovered: bool,
    pub attempts: usize,
    #[serde(default)]
    pub agent_paused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<ChatResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug)]
pub(crate) enum AttemptError {
    Retryable(String),
    Fatal(String),
}

impl AttemptError {
    pub(crate) fn message(self) -> String {
        match self {
            AttemptError::Retryable(msg) | AttemptError::Fatal(msg) => msg,
        }
    }

    pub(crate) fn is_retryable(&self) -> bool {
        matches!(self, AttemptError::Retryable(_))
    }
}

pub(crate) fn is_retryable_http_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

pub fn order_providers(mut providers: Vec<Provider>, start_id: Option<&str>) -> Vec<Provider> {
    providers.sort_by_key(|p| p.priority);
    if let Some(id) = start_id {
        if let Some(pos) = providers.iter().position(|p| p.id == id) {
            let rotated = providers.split_off(pos);
            providers = rotated.into_iter().chain(providers).collect();
        }
    }
    providers
}

pub async fn send_chat(
    app: &AppHandle,
    db: Arc<Database>,
    http: &Client,
    session_id: &str,
    user_text: &str,
    provider_id: Option<&str>,
    auto_failover: bool,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> AppResult<ChatResponse> {
    let providers = db.get_enabled_providers()?;
    if providers.is_empty() {
        return Err(AppError::from("未配置可用的模型服务，请先在「模型服务」页添加 API Key"));
    }

    let ordered = order_providers(providers, provider_id);
    let primary_id = ordered.first().map(|p| p.id.clone());
    let settings = load_context_settings(&db)?;
    let ctx = build_context(&db, session_id, &settings)?;

    let mut errors: Vec<String> = Vec::new();

    for (index, provider) in ordered.iter().enumerate() {
        let api_key = match secrets::get_api_key(&provider.id) {
            Ok(key) => key,
            Err(err) => {
                errors.push(format!("{}：{err}", provider.name));
                continue;
            }
        };

        let openai_messages = build_openai_messages_from_context(
            &ctx.recent,
            user_text,
            ctx.summary_prefix.as_deref(),
        );
        let anthropic_messages = build_anthropic_messages_from_context(
            &ctx.recent,
            user_text,
            ctx.summary_prefix.as_deref(),
        );

        let result = match provider.api_format.as_str() {
            "anthropic_messages" => {
                stream_anthropic_chat(
                    http,
                    provider,
                    &api_key,
                    anthropic_messages,
                    app,
                    session_id,
                    Arc::clone(&cancel),
                )
                .await
            }
            _ => {
                stream_openai_chat(
                    http,
                    provider,
                    &api_key,
                    openai_messages,
                    app,
                    session_id,
                    Arc::clone(&cancel),
                )
                .await
            }
        };

        match result {
            Ok(content) => {
                let cancelled = cancel.load(std::sync::atomic::Ordering::Relaxed);
                let response = ChatResponse {
                    content: content.clone(),
                    provider_id: provider.id.clone(),
                    provider_name: provider.name.clone(),
                    failovered: primary_id.as_deref() != Some(provider.id.as_str()),
                    attempts: index + 1,
                    agent_paused: false,
                    approval_id: None,
                    pending_action: None,
                    pending_command: None,
                };
                emit_done(app, session_id, &response);

                let db_clone = Arc::clone(&db);
                let session_for_db = session_id.to_string();
                let user_text_owned = user_text.to_string();
                let partial = cancelled;
                let content_owned = content.clone();
                tokio::task::spawn_blocking(move || {
                    append_turn(&db_clone, &session_for_db, &user_text_owned, &content_owned, partial)
                })
                .await
                .map_err(|err| AppError::from(format!("保存消息失败: {err}")))??;

                let db_sum = Arc::clone(&db);
                let http_sum = http.clone();
                let sid_sum = session_id.to_string();
                let settings_sum = settings.clone();
                let prov_sum = provider.clone();
                let key_sum = api_key.clone();
                tokio::spawn(async move {
                    let _ = maybe_update_summary(
                        db_sum,
                        &http_sum,
                        &sid_sum,
                        &settings_sum,
                        &prov_sum,
                        &key_sum,
                    )
                    .await;
                });

                return Ok(response);
            }
            Err(err) => {
                let retryable = err.is_retryable();
                let msg = err.message();
                if msg == "已取消生成" {
                    emit_error(app, session_id, &msg);
                    return Err(AppError::from(msg));
                }
                errors.push(format!("{}：{msg}", provider.name));
                if !auto_failover || !retryable || index + 1 == ordered.len() {
                    emit_error(app, session_id, &msg);
                    return Err(AppError::from(if errors.len() == 1 {
                        msg
                    } else {
                        format!("所有模型服务均失败：{}", errors.join("；"))
                    }));
                }
            }
        }
    }

    let msg = format!("所有模型服务均失败：{}", errors.join("；"));
    emit_error(app, session_id, &msg);
    Err(AppError::from(msg))
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    error: Option<OpenAiError>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiError {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<Value>,
    reasoning_content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicBlock>,
    error: Option<AnthropicError>,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

pub(crate) fn build_openai_chat_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    if base.ends_with("/v1") || base.ends_with("/v2") || base.ends_with("/v3") || base.ends_with("/v4") {
        return format!("{base}/chat/completions");
    }
    format!("{base}/v1/chat/completions")
}

pub(crate) fn build_anthropic_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/messages") {
        return base.to_string();
    }
    if base.ends_with("/v1") {
        return format!("{base}/messages");
    }
    format!("{base}/v1/messages")
}

pub(crate) fn openai_role(role: &str) -> Option<&'static str> {
    match role {
        "user" => Some("user"),
        "assistant" => Some("assistant"),
        "system" => Some("system"),
        "tool" => Some("assistant"),
        _ => None,
    }
}

fn extract_openai_text(message: &OpenAiMessage) -> String {
    if let Some(content) = message.content.as_ref() {
        match content {
            Value::String(text) if !text.is_empty() => return text.clone(),
            Value::Array(parts) => {
                let text = parts
                    .iter()
                    .filter_map(|part| {
                        part.get("text")
                            .and_then(|t| t.as_str())
                            .or_else(|| part.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    return text;
                }
            }
            _ => {}
        }
    }

    message
        .reasoning_content
        .clone()
        .or_else(|| message.reasoning.clone())
        .unwrap_or_default()
}

pub(crate) fn map_http_error(err: reqwest::Error) -> AttemptError {
    if err.is_timeout() {
        AttemptError::Retryable("请求超时：请检查 Base URL、模型名称，或网络连接".to_string())
    } else if err.is_connect() {
        AttemptError::Retryable("无法连接模型服务：请检查 Base URL 是否正确".to_string())
    } else {
        AttemptError::Fatal(err.to_string())
    }
}

pub async fn complete_openai(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    history: &[crate::storage::db::MessageView],
    user_text: &str,
) -> Result<String, AttemptError> {
    let mut messages = Vec::new();
    for msg in history {
        let Some(role) = openai_role(&msg.role) else {
            continue;
        };
        let text = msg
            .parts
            .iter()
            .filter_map(|p| p.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            messages.push(json!({ "role": role, "content": text }));
        }
    }
    messages.push(json!({ "role": "user", "content": user_text }));

    let url = build_openai_chat_url(&provider.base_url);
    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&json!({
            "model": provider.default_model,
            "messages": messages,
            "stream": false,
        }))
        .send()
        .await
        .map_err(map_http_error)?;

    let status = response.status();
    let body = response.text().await.map_err(map_http_error)?;

    if !status.is_success() {
        return Err(if is_retryable_http_status(status.as_u16()) {
            AttemptError::Retryable(format!(
                "模型服务错误 ({status}): {}",
                summarize_error_body(&body)
            ))
        } else {
            AttemptError::Fatal(format!(
                "模型服务错误 ({status}): {}",
                summarize_error_body(&body)
            ))
        });
    }

    let parsed: OpenAiResponse = serde_json::from_str(&body).map_err(|err| {
        AttemptError::Fatal(format!("无法解析模型响应: {err}; body={}", truncate(&body, 300)))
    })?;

    if let Some(error) = parsed.error {
        return Err(AttemptError::Fatal(format!(
            "模型服务错误: {}",
            error.message.unwrap_or_else(|| "unknown".to_string())
        )));
    }

    if let Some(message) = parsed.message {
        return Err(AttemptError::Fatal(format!("模型服务错误: {message}")));
    }

    let content = parsed
        .choices
        .first()
        .map(|choice| extract_openai_text(&choice.message))
        .unwrap_or_default();

    if content.is_empty() {
        return Err(AttemptError::Fatal(format!(
            "模型返回空内容，请确认模型名称「{}」是否正确",
            provider.default_model
        )));
    }

    Ok(content)
}

pub(crate) async fn complete_anthropic(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    history: &[crate::storage::db::MessageView],
    user_text: &str,
) -> Result<String, AttemptError> {
    let mut messages = Vec::new();
    for msg in history {
        if msg.role == "system" {
            continue;
        }
        let role = if msg.role == "assistant" || msg.role == "user" {
            msg.role.as_str()
        } else {
            continue;
        };
        let text = msg
            .parts
            .iter()
            .filter_map(|p| p.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            messages.push(json!({ "role": role, "content": text }));
        }
    }
    messages.push(json!({ "role": "user", "content": user_text }));

    let url = build_anthropic_url(&provider.base_url);
    let response = http
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": provider.default_model,
            "max_tokens": 4096,
            "messages": messages,
            "stream": false,
        }))
        .send()
        .await
        .map_err(map_http_error)?;

    let status = response.status();
    let body = response.text().await.map_err(map_http_error)?;

    if !status.is_success() {
        return Err(if is_retryable_http_status(status.as_u16()) {
            AttemptError::Retryable(format!(
                "模型服务错误 ({status}): {}",
                summarize_error_body(&body)
            ))
        } else {
            AttemptError::Fatal(format!(
                "模型服务错误 ({status}): {}",
                summarize_error_body(&body)
            ))
        });
    }

    let parsed: AnthropicResponse = serde_json::from_str(&body).map_err(|err| {
        AttemptError::Fatal(format!("无法解析模型响应: {err}; body={}", truncate(&body, 300)))
    })?;

    if let Some(error) = parsed.error {
        return Err(AttemptError::Fatal(format!(
            "模型服务错误: {}",
            error.message.unwrap_or_else(|| "unknown".to_string())
        )));
    }

    let content = parsed
        .content
        .iter()
        .filter(|b| b.block_type == "text")
        .filter_map(|b| b.text.clone())
        .collect::<Vec<_>>()
        .join("\n");

    if content.is_empty() {
        return Err(AttemptError::Fatal(format!(
            "模型返回空内容，请确认模型名称「{}」是否正确",
            provider.default_model
        )));
    }

    Ok(content)
}

pub(crate) fn summarize_error_body(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = value
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("message").and_then(|v| v.as_str()))
        {
            return msg.to_string();
        }
    }
    truncate(body, 300)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect::<String>() + "…"
}

pub fn append_turn(
    db: &Database,
    session_id: &str,
    user_text: &str,
    assistant_text: &str,
    partial: bool,
) -> AppResult<()> {
    let existing = db.list_messages(session_id)?;
    let mut seq = existing.last().map(|m| m.seq + 1).unwrap_or(0);

    db.insert_message(&CanonicalMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        seq,
        role: "user".to_string(),
        parts: vec![MessagePart {
            part_type: "text".to_string(),
            text: Some(user_text.to_string()),
            name: None,
            input: None,
        }],
        timestamp: None,
        metadata: json!({ "source": "native" }),
    })?;
    seq += 1;

    let mut meta = json!({ "source": "native" });
    if partial {
        meta["partial"] = json!(true);
    }

    db.insert_message(&CanonicalMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        seq,
        role: "assistant".to_string(),
        parts: vec![MessagePart {
            part_type: "text".to_string(),
            text: Some(assistant_text.to_string()),
            name: None,
            input: None,
        }],
        timestamp: None,
        metadata: meta,
    })?;
    Ok(())
}

pub fn append_user_and_assistant(
    db: &Database,
    session_id: &str,
    user_text: &str,
    assistant_text: &str,
) -> AppResult<()> {
    append_turn(db, session_id, user_text, assistant_text, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_url_builder() {
        assert_eq!(
            build_openai_chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_openai_chat_url("https://open.bigmodel.cn/api/paas/v4"),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        assert_eq!(
            build_openai_chat_url("https://api.example.com"),
            "https://api.example.com/v1/chat/completions"
        );
    }
}
