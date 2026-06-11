use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

use super::chat::{
    build_anthropic_url, build_openai_chat_url, map_http_error, AttemptError, ChatResponse,
    ChatStreamEvent, is_retryable_http_status, summarize_error_body,
};
use crate::storage::db::{MessageView, Provider};

pub async fn stream_openai_chat(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    messages: Vec<Value>,
    app: &AppHandle,
    session_id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<String, AttemptError> {
    let url = build_openai_chat_url(&provider.base_url);
    let started = Instant::now();
    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "model": provider.default_model,
            "messages": messages,
            "stream": true,
        }))
        .send()
        .await
        .map_err(map_http_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.map_err(map_http_error)?;
        return Err(classify_http(status.as_u16(), &body));
    }

    let mut full = String::new();
    read_openai_sse(response, app, session_id, &mut full, &cancel).await?;

    if cancel.load(Ordering::Relaxed) && full.is_empty() {
        return Err(AttemptError::Fatal("已取消生成".into()));
    }

    if full.is_empty() {
        return Err(AttemptError::Fatal(format!(
            "模型返回空内容，请确认模型名称「{}」是否正确",
            provider.default_model
        )));
    }

    let _ = started.elapsed();
    Ok(full)
}

pub async fn stream_anthropic_chat(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    messages: Vec<Value>,
    app: &AppHandle,
    session_id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<String, AttemptError> {
    let url = build_anthropic_url(&provider.base_url);
    let response = http
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": provider.default_model,
            "max_tokens": 4096,
            "messages": messages,
            "stream": true,
        }))
        .send()
        .await
        .map_err(map_http_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.map_err(map_http_error)?;
        return Err(classify_http(status.as_u16(), &body));
    }

    let mut full = String::new();
    read_anthropic_sse(response, app, session_id, &mut full, &cancel).await?;

    if cancel.load(Ordering::Relaxed) && full.is_empty() {
        return Err(AttemptError::Fatal("已取消生成".into()));
    }

    if full.is_empty() {
        return Err(AttemptError::Fatal(format!(
            "模型返回空内容，请确认模型名称「{}」是否正确",
            provider.default_model
        )));
    }

    Ok(full)
}

pub fn emit_chunk(app: &AppHandle, session_id: &str, chunk: &str) {
    let _ = app.emit(
        "chat-stream",
        ChatStreamEvent {
            session_id: session_id.to_string(),
            chunk: Some(chunk.to_string()),
            done: None,
            error: None,
        },
    );
}

pub fn emit_done(app: &AppHandle, session_id: &str, response: &ChatResponse) {
    let _ = app.emit(
        "chat-stream",
        ChatStreamEvent {
            session_id: session_id.to_string(),
            chunk: None,
            done: Some(response.clone()),
            error: None,
        },
    );
}

pub fn emit_error(app: &AppHandle, session_id: &str, error: &str) {
    let _ = app.emit(
        "chat-stream",
        ChatStreamEvent {
            session_id: session_id.to_string(),
            chunk: None,
            done: None,
            error: Some(error.to_string()),
        },
    );
}

fn classify_http(status: u16, body: &str) -> AttemptError {
    let msg = format!("模型服务错误 ({status}): {}", summarize_error_body(body));
    if is_retryable_http_status(status) {
        AttemptError::Retryable(msg)
    } else {
        AttemptError::Fatal(msg)
    }
}

async fn read_openai_sse(
    response: reqwest::Response,
    app: &AppHandle,
    session_id: &str,
    full: &mut String,
    cancel: &AtomicBool,
) -> Result<(), AttemptError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(item) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let chunk = item.map_err(map_http_error)?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer.drain(..=pos);

            if !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            let parsed: Value = serde_json::from_str(data).map_err(|err| {
                AttemptError::Fatal(format!("无法解析流式响应: {err}"))
            })?;

            if let Some(error) = parsed.pointer("/error/message").and_then(|v| v.as_str()) {
                return Err(AttemptError::Fatal(format!("模型服务错误: {error}")));
            }

            let delta = parsed
                .pointer("/choices/0/delta/content")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    parsed
                        .pointer("/choices/0/delta/reasoning_content")
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("");

            if !delta.is_empty() {
                full.push_str(delta);
                emit_chunk(app, session_id, delta);
            }
        }
    }

    Ok(())
}

async fn read_anthropic_sse(
    response: reqwest::Response,
    app: &AppHandle,
    session_id: &str,
    full: &mut String,
    cancel: &AtomicBool,
) -> Result<(), AttemptError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut current_event = String::new();

    while let Some(item) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let chunk = item.map_err(map_http_error)?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end_matches('\r').to_string();
            buffer.drain(..=pos);

            if line.is_empty() {
                current_event.clear();
                continue;
            }

            if let Some(event) = line.strip_prefix("event:") {
                current_event = event.trim().to_string();
                continue;
            }

            if !line.starts_with("data:") {
                continue;
            }

            let data = line.trim_start_matches("data:").trim();
            if data.is_empty() {
                continue;
            }

            let parsed: Value = serde_json::from_str(data).map_err(|err| {
                AttemptError::Fatal(format!("无法解析流式响应: {err}"))
            })?;

            if current_event == "error" || parsed.get("type").and_then(|v| v.as_str()) == Some("error") {
                let msg = parsed
                    .pointer("/error/message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown stream error");
                return Err(AttemptError::Fatal(format!("模型服务错误: {msg}")));
            }

            let event_type = parsed
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or(current_event.as_str());

            if event_type == "content_block_delta" {
                if let Some(text) = parsed.pointer("/delta/text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        full.push_str(text);
                        emit_chunk(app, session_id, text);
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn build_openai_messages(
    history: &[MessageView],
    user_text: &str,
) -> Vec<Value> {
    build_openai_messages_from_context(history, user_text, None)
}

pub fn build_openai_messages_from_context(
    history: &[MessageView],
    user_text: &str,
    system_extra: Option<&str>,
) -> Vec<Value> {
    use super::chat::openai_role;
    use serde_json::json;

    let mut messages = Vec::new();
    if let Some(extra) = system_extra.filter(|s| !s.trim().is_empty()) {
        messages.push(json!({ "role": "system", "content": extra }));
    }
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
    messages
}

pub fn build_anthropic_messages(
    history: &[MessageView],
    user_text: &str,
) -> Vec<Value> {
    build_anthropic_messages_from_context(history, user_text, None)
}

pub fn build_anthropic_messages_from_context(
    history: &[MessageView],
    user_text: &str,
    system_extra: Option<&str>,
) -> Vec<Value> {
    use serde_json::json;

    let mut messages = Vec::new();
    if let Some(extra) = system_extra.filter(|s| !s.trim().is_empty()) {
        messages.push(json!({ "role": "user", "content": extra }));
        messages.push(json!({
            "role": "assistant",
            "content": "好的，我会遵循以上系统说明。"
        }));
    }
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
    messages
}
