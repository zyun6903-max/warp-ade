use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use tauri::AppHandle;

use crate::agent::parser::ParsedToolCall;
use crate::agent::tool_schema::{tool_definitions_anthropic, tool_definitions_openai};
use crate::providers::chat::{
    build_anthropic_url, build_openai_chat_url, map_http_error, AttemptError,
};
use crate::providers::stream::emit_chunk;
use crate::storage::db::Provider;

#[derive(Debug, Clone)]
pub struct AgentStreamResult {
    pub text: String,
    pub tool_calls: Vec<ParsedToolCall>,
}

pub async fn stream_agent_completion(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    openai_messages: Vec<Value>,
    anthropic_messages: Vec<Value>,
    system: &str,
    extra_openai_tools: &[Value],
    extra_anthropic_tools: &[Value],
    app: &AppHandle,
    session_id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<AgentStreamResult, AttemptError> {
    match provider.api_format.as_str() {
        "anthropic_messages" => {
            stream_anthropic_agent(
                http,
                provider,
                api_key,
                anthropic_messages,
                system,
                extra_anthropic_tools,
                app,
                session_id,
                cancel,
            )
            .await
        }
        _ => {
            stream_openai_agent(
                http,
                provider,
                api_key,
                openai_messages,
                extra_openai_tools,
                app,
                session_id,
                cancel,
            )
            .await
        }
    }
}

pub async fn stream_openai_agent(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    messages: Vec<Value>,
    extra_tools: &[Value],
    app: &AppHandle,
    session_id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<AgentStreamResult, AttemptError> {
    let url = build_openai_chat_url(&provider.base_url);
    let mut tools = tool_definitions_openai();
    tools.extend(extra_tools.iter().cloned());
    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "model": provider.default_model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
            "stream": true,
        }))
        .send()
        .await
        .map_err(map_http_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.map_err(map_http_error)?;
        return Err(classify_http_in_agent(status.as_u16(), &body));
    }

    let mut text = String::new();
    let mut tool_parts: HashMap<usize, OpenAiToolPart> = HashMap::new();
    read_openai_agent_sse(response, app, session_id, &mut text, &mut tool_parts, &cancel).await?;

    if cancel.load(Ordering::Relaxed) && text.is_empty() && tool_parts.is_empty() {
        return Err(AttemptError::Fatal("已取消生成".into()));
    }

    let tool_calls = finalize_openai_tools(tool_parts);
    if text.is_empty() && tool_calls.is_empty() {
        return Err(AttemptError::Fatal(format!(
            "模型返回空内容，请确认模型名称「{}」是否支持 function calling",
            provider.default_model
        )));
    }

    Ok(AgentStreamResult { text, tool_calls })
}

pub async fn stream_anthropic_agent(
    http: &Client,
    provider: &Provider,
    api_key: &str,
    messages: Vec<Value>,
    system: &str,
    extra_tools: &[Value],
    app: &AppHandle,
    session_id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<AgentStreamResult, AttemptError> {
    let url = build_anthropic_url(&provider.base_url);
    let mut tools = tool_definitions_anthropic();
    tools.extend(extra_tools.iter().cloned());
    let response = http
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&serde_json::json!({
            "model": provider.default_model,
            "max_tokens": 4096,
            "system": system,
            "messages": messages,
            "tools": tools,
            "stream": true,
        }))
        .send()
        .await
        .map_err(map_http_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.map_err(map_http_error)?;
        return Err(classify_http_in_agent(status.as_u16(), &body));
    }

    let mut text = String::new();
    let mut blocks: HashMap<usize, AnthropicBlock> = HashMap::new();
    read_anthropic_agent_sse(response, app, session_id, &mut text, &mut blocks, &cancel).await?;

    if cancel.load(Ordering::Relaxed) && text.is_empty() && blocks.is_empty() {
        return Err(AttemptError::Fatal("已取消生成".into()));
    }

    let tool_calls = finalize_anthropic_tools(&blocks);
    if text.is_empty() && tool_calls.is_empty() {
        return Err(AttemptError::Fatal(format!(
            "模型返回空内容，请确认模型名称「{}」是否支持 tool use",
            provider.default_model
        )));
    }

    Ok(AgentStreamResult { text, tool_calls })
}

#[derive(Default)]
struct OpenAiToolPart {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

enum AnthropicBlock {
    Text(String),
    Tool {
        id: String,
        name: String,
        input_json: String,
    },
}

async fn read_openai_agent_sse(
    response: reqwest::Response,
    app: &AppHandle,
    session_id: &str,
    text: &mut String,
    tool_parts: &mut HashMap<usize, OpenAiToolPart>,
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

            let parsed: Value = serde_json::from_str(data)
                .map_err(|err| AttemptError::Fatal(format!("无法解析流式响应: {err}")))?;

            if let Some(error) = parsed.pointer("/error/message").and_then(|v| v.as_str()) {
                return Err(AttemptError::Fatal(format!("模型服务错误: {error}")));
            }

            let delta = &parsed["choices"][0]["delta"];
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                if !content.is_empty() {
                    text.push_str(content);
                    emit_chunk(app, session_id, content);
                }
            }
            if let Some(reasoning) = delta
                .get("reasoning_content")
                .and_then(|v| v.as_str())
            {
                if !reasoning.is_empty() {
                    text.push_str(reasoning);
                    emit_chunk(app, session_id, reasoning);
                }
            }

            if let Some(calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for call in calls {
                    let index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let part = tool_parts.entry(index).or_default();
                    if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                        part.id = Some(id.to_string());
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(|v| v.as_str()) {
                        part.name = Some(name.to_string());
                    }
                    if let Some(args) = call.pointer("/function/arguments").and_then(|v| v.as_str())
                    {
                        part.arguments.push_str(args);
                    }
                }
            }
        }
    }

    Ok(())
}

fn finalize_openai_tools(parts: HashMap<usize, OpenAiToolPart>) -> Vec<ParsedToolCall> {
    let mut indices: Vec<_> = parts.keys().copied().collect();
    indices.sort();
    indices
        .into_iter()
        .filter_map(|i| {
            let part = parts.get(&i)?;
            let name = part.name.clone()?;
            let id = part
                .id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let arguments = if part.arguments.trim().is_empty() {
                Value::Object(Default::default())
            } else {
                serde_json::from_str(&part.arguments)
                    .unwrap_or_else(|_| serde_json::json!({ "raw": part.arguments }))
            };
            Some(ParsedToolCall { id, name, arguments })
        })
        .collect()
}

async fn read_anthropic_agent_sse(
    response: reqwest::Response,
    app: &AppHandle,
    session_id: &str,
    text: &mut String,
    blocks: &mut HashMap<usize, AnthropicBlock>,
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

            let parsed: Value = serde_json::from_str(data)
                .map_err(|err| AttemptError::Fatal(format!("无法解析流式响应: {err}")))?;

            let event_type = parsed
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or(current_event.as_str());

            if event_type == "error" {
                let msg = parsed
                    .pointer("/error/message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown stream error");
                return Err(AttemptError::Fatal(format!("模型服务错误: {msg}")));
            }

            if event_type == "content_block_start" {
                let index = parsed.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let block = &parsed["content_block"];
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if block_type == "tool_use" {
                    blocks.insert(
                        index,
                        AnthropicBlock::Tool {
                            id: block
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            name: block
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            input_json: String::new(),
                        },
                    );
                } else if block_type == "text" {
                    blocks.insert(index, AnthropicBlock::Text(String::new()));
                }
            }

            if event_type == "content_block_delta" {
                let index = parsed.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let delta = &parsed["delta"];
                let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if delta_type == "text_delta" {
                    if let Some(chunk_text) = delta.get("text").and_then(|v| v.as_str()) {
                        if !chunk_text.is_empty() {
                            text.push_str(chunk_text);
                            emit_chunk(app, session_id, chunk_text);
                            if let Some(AnthropicBlock::Text(existing)) = blocks.get_mut(&index) {
                                existing.push_str(chunk_text);
                            }
                        }
                    }
                } else if delta_type == "input_json_delta" {
                    if let Some(partial) = delta.get("partial_json").and_then(|v| v.as_str()) {
                        if let Some(AnthropicBlock::Tool { input_json, .. }) =
                            blocks.get_mut(&index)
                        {
                            input_json.push_str(partial);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn finalize_anthropic_tools(blocks: &HashMap<usize, AnthropicBlock>) -> Vec<ParsedToolCall> {
    let mut indices: Vec<_> = blocks.keys().copied().collect();
    indices.sort();
    indices
        .into_iter()
        .filter_map(|i| {
            let AnthropicBlock::Tool {
                id,
                name,
                input_json,
            } = blocks.get(&i)?
            else {
                return None;
            };
            if name.is_empty() {
                return None;
            }
            let arguments = if input_json.trim().is_empty() {
                Value::Object(Default::default())
            } else {
                serde_json::from_str(input_json)
                    .unwrap_or_else(|_| serde_json::json!({ "raw": input_json }))
            };
            Some(ParsedToolCall {
                id: if id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    id.clone()
                },
                name: name.clone(),
                arguments,
            })
        })
        .collect()
}

fn classify_http_in_agent(status: u16, body: &str) -> AttemptError {
    use crate::providers::chat::{is_retryable_http_status, summarize_error_body};
    let msg = format!("模型服务错误 ({status}): {}", summarize_error_body(body));
    if is_retryable_http_status(status) {
        AttemptError::Retryable(msg)
    } else {
        AttemptError::Fatal(msg)
    }
}
