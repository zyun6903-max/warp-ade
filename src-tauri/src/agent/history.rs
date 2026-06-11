use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::agent::parser::ParsedToolCall;
use crate::providers::chat::openai_role;
use crate::storage::db::MessageView;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentLoopTurn {
    Assistant {
        text: String,
        tool_calls: Vec<AgentToolCall>,
    },
    ToolResults(Vec<AgentToolResult>),
}

impl From<AgentToolCall> for ParsedToolCall {
    fn from(c: AgentToolCall) -> Self {
        ParsedToolCall {
            id: c.id,
            name: c.name,
            arguments: c.arguments,
        }
    }
}

impl From<ParsedToolCall> for AgentToolCall {
    fn from(c: ParsedToolCall) -> Self {
        AgentToolCall {
            id: c.id,
            name: c.name,
            arguments: c.arguments,
        }
    }
}

pub fn build_openai_agent_messages(
    db_history: &[MessageView],
    loop_turns: &[AgentLoopTurn],
    user_text: &str,
    system: &str,
) -> Vec<Value> {
    let mut messages = Vec::new();
    if !system.trim().is_empty() {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for msg in db_history {
        let Some(role) = openai_role_for_history(&msg.role) else {
            continue;
        };
        let text = message_text(msg);
        if !text.is_empty() {
            messages.push(json!({ "role": role, "content": text }));
        }
    }
    for turn in loop_turns {
        append_openai_turn(&mut messages, turn);
    }
    messages.push(json!({ "role": "user", "content": user_text }));
    messages
}

pub fn build_anthropic_agent_messages(
    db_history: &[MessageView],
    loop_turns: &[AgentLoopTurn],
    user_text: &str,
) -> Vec<Value> {
    let mut messages = Vec::new();
    for msg in db_history {
        if msg.role == "system" {
            continue;
        }
        let role = if msg.role == "assistant" || msg.role == "user" {
            msg.role.as_str()
        } else {
            continue;
        };
        let text = message_text(msg);
        if !text.is_empty() {
            messages.push(json!({ "role": role, "content": text }));
        }
    }
    for turn in loop_turns {
        append_anthropic_turn(&mut messages, turn);
    }
    messages.push(json!({ "role": "user", "content": user_text }));
    messages
}

fn openai_role_for_history(role: &str) -> Option<&'static str> {
    match role {
        "user" => Some("user"),
        "assistant" => Some("assistant"),
        "system" => Some("system"),
        _ => openai_role(role),
    }
}

fn message_text(msg: &MessageView) -> String {
    msg.parts
        .iter()
        .filter_map(|p| p.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_openai_turn(messages: &mut Vec<Value>, turn: &AgentLoopTurn) {
    match turn {
        AgentLoopTurn::Assistant { text, tool_calls } => {
            if tool_calls.is_empty() {
                if !text.is_empty() {
                    messages.push(json!({ "role": "assistant", "content": text }));
                }
                return;
            }
            let mut msg = json!({
                "role": "assistant",
                "tool_calls": tool_calls.iter().map(|c| {
                    json!({
                        "id": c.id,
                        "type": "function",
                        "function": {
                            "name": c.name,
                            "arguments": c.arguments.to_string()
                        }
                    })
                }).collect::<Vec<_>>()
            });
            if !text.is_empty() {
                msg["content"] = json!(text);
            } else {
                msg["content"] = Value::Null;
            }
            messages.push(msg);
        }
        AgentLoopTurn::ToolResults(results) => {
            for r in results {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": r.tool_call_id,
                    "content": r.content
                }));
            }
        }
    }
}

fn append_anthropic_turn(messages: &mut Vec<Value>, turn: &AgentLoopTurn) {
    match turn {
        AgentLoopTurn::Assistant { text, tool_calls } => {
            let mut blocks = Vec::new();
            if !text.is_empty() {
                blocks.push(json!({ "type": "text", "text": text }));
            }
            for c in tool_calls {
                blocks.push(json!({
                    "type": "tool_use",
                    "id": c.id,
                    "name": c.name,
                    "input": c.arguments
                }));
            }
            if !blocks.is_empty() {
                messages.push(json!({ "role": "assistant", "content": blocks }));
            }
        }
        AgentLoopTurn::ToolResults(results) => {
            let blocks: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "type": "tool_result",
                        "tool_use_id": r.tool_call_id,
                        "content": r.content
                    })
                })
                .collect();
            messages.push(json!({ "role": "user", "content": blocks }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_tool_result_message_shape() {
        let turns = vec![
            AgentLoopTurn::Assistant {
                text: "读取中".into(),
                tool_calls: vec![AgentToolCall {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    arguments: json!({ "path": "README.md" }),
                }],
            },
            AgentLoopTurn::ToolResults(vec![AgentToolResult {
                tool_call_id: "call_1".into(),
                name: "read_file".into(),
                content: "hello".into(),
            }]),
        ];
        let msgs = build_openai_agent_messages(&[], &turns, "继续", "system");
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[2]["role"], "tool");
    }
}
