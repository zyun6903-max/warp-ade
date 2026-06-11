use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::storage::db::{CanonicalMessage, Database, MessagePart, Session};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeImportCandidate {
    pub source_path: String,
    pub project_slug: String,
    pub session_id: String,
    pub title: String,
    pub message_count_estimate: usize,
    pub modified_at: i64,
    pub already_imported: bool,
}

#[derive(Debug, Deserialize)]
struct ClaudeLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    uuid: Option<String>,
    #[serde(rename = "parentUuid")]
    parent_uuid: Option<String>,
    message: Option<ClaudeMessage>,
    timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    role: Option<String>,
    content: Option<Value>,
}

pub fn scan_claude_transcripts(db: &Database) -> AppResult<Vec<ClaudeImportCandidate>> {
    let imported = db.imported_source_paths()?;
    let home = dirs::home_dir().ok_or_else(|| AppError::from("无法定位用户目录"))?;
    let root = home.join(".claude").join("projects");
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::new();
    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".jsonl") {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        let project_slug = extract_project_slug(path);
        let session_id = name.trim_end_matches(".jsonl").to_string();
        let modified_at = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
            .unwrap_or(0);
        let message_count_estimate = count_lines(path)?;
        let already_imported = imported.contains(&path_str);
        let title = format!("Claude · {session_id}");

        candidates.push(ClaudeImportCandidate {
            source_path: path_str,
            project_slug,
            session_id,
            title,
            message_count_estimate,
            modified_at,
            already_imported,
        });
    }

    candidates.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(crate::import::dedupe::dedupe_import_candidates(
        candidates,
        |c| c.source_path.clone(),
        |c| format!("{}::{}", c.project_slug, c.session_id),
        |c| c.modified_at,
    ))
}

pub fn search_claude_transcripts(
    db: &Database,
    query: &str,
    limit: Option<usize>,
) -> AppResult<Vec<crate::import::search::ImportSourceSearchHit>> {
    use crate::import::search::{file_modified_epoch, preview_snippet, ImportSourceSearchHit};

    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let limit = limit.unwrap_or(50).clamp(1, 200);
    let imported = db.imported_source_paths()?;
    let home = dirs::home_dir().ok_or_else(|| AppError::from("无法定位用户目录"))?;
    let root = home.join(".claude").join("projects");
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut hits = Vec::new();
    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if hits.len() >= limit {
            break;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".jsonl") {
            continue;
        }

        let Some(matched_text) = find_claude_user_match(path, trimmed)? else {
            continue;
        };

        let path_str = path.to_string_lossy().to_string();
        let project_slug = extract_project_slug(path);
        let workspace_path = crate::import::projects::decode_cursor_project_slug(&project_slug);
        let session_id = name.trim_end_matches(".jsonl").to_string();
        let modified_at = file_modified_epoch(path);
        let already_imported = imported.contains(&path_str);

        hits.push(ImportSourceSearchHit {
            source_path: path_str,
            project_slug,
            session_id,
            modified_at,
            already_imported,
            workspace_path,
            matched_preview: preview_snippet(&matched_text, trimmed, 240),
        });
    }

    hits.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    hits.truncate(limit);
    Ok(hits)
}

fn find_claude_user_match(path: &Path, query: &str) -> AppResult<Option<String>> {
    use crate::import::search::{line_within_limit, text_matches_query};

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() || !line_within_limit(&line) {
            continue;
        }
        let parsed: ClaudeLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let role = parsed
            .message
            .as_ref()
            .and_then(|m| m.role.as_deref())
            .or(parsed.line_type.as_deref())
            .unwrap_or("assistant");
        if role != "user" {
            continue;
        }
        let text = claude_line_text(&parsed);
        if text.is_empty() {
            continue;
        }
        if text_matches_query(&text, query) {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

fn claude_line_text(parsed: &ClaudeLine) -> String {
    extract_claude_parts(parsed.message.as_ref())
        .into_iter()
        .filter_map(|p| p.text)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn import_claude_file(db: &Database, source_path: &str) -> AppResult<Session> {
    let path = PathBuf::from(source_path);
    if db.session_exists_by_source(source_path)? {
        return Err(AppError::from("session already imported"));
    }

    let project_slug = extract_project_slug(&path);
    let workspace_path = crate::import::projects::decode_cursor_project_slug(&project_slug);
    let project_name = crate::import::projects::project_name_from_path(
        workspace_path.as_deref(),
        Some(&project_slug),
    );
    let project_id = crate::import::projects::ensure_project(
        db,
        &project_name,
        workspace_path.as_deref(),
        Some(&project_slug),
        "claude_code",
    )?;
    let session = db.create_session(
        &format!(
            "Claude · {}",
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("session")
        ),
        "claude_code",
        Some(source_path),
        Some(&project_slug),
        workspace_path.as_deref(),
        Some(&project_id),
        None,
    )?;

    let messages = parse_claude_jsonl(&path, &session.id)?;
    for message in messages {
        db.insert_message(&message)?;
    }

    if let Some(first_user) = db
        .list_messages(&session.id)?
        .into_iter()
        .find(|m| m.role == "user")
    {
        let title = first_user.preview.chars().take(60).collect::<String>();
        if !title.is_empty() {
            db.update_session_title(&session.id, &title)?;
        }
    }

    db.get_session(&session.id)?
        .ok_or_else(|| AppError::from("failed to reload imported session"))
}

fn extract_project_slug(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|w| {
            if w[0] == "projects" {
                Some(w[1].clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn count_lines(path: &Path) -> AppResult<usize> {
    let file = File::open(path)?;
    Ok(BufReader::new(file).lines().count())
}

fn parse_claude_jsonl(path: &Path, session_id: &str) -> AppResult<Vec<CanonicalMessage>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut raw_lines = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: ClaudeLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        raw_lines.push(parsed);
    }

    let ordered = order_claude_lines(raw_lines);
    let mut messages = Vec::new();
    let mut seq = 0i64;

    for parsed in ordered {
        let role = parsed
            .message
            .as_ref()
            .and_then(|m| m.role.clone())
            .or_else(|| parsed.line_type.clone())
            .unwrap_or_else(|| "assistant".to_string());

        if role == "system" || role == "summary" {
            continue;
        }

        let parts = extract_claude_parts(parsed.message.as_ref());
        if parts.is_empty() {
            continue;
        }

        messages.push(CanonicalMessage {
            id: parsed.uuid.unwrap_or_else(|| Uuid::new_v4().to_string()),
            session_id: session_id.to_string(),
            seq,
            role: normalize_role(&role),
            parts,
            timestamp: parsed.timestamp,
            metadata: serde_json::json!({ "source": "claude_code" }),
        });
        seq += 1;
    }

    Ok(messages)
}

fn order_claude_lines(lines: Vec<ClaudeLine>) -> Vec<ClaudeLine> {
    if lines.is_empty() {
        return lines;
    }

    let mut by_uuid: HashMap<String, ClaudeLine> = HashMap::new();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut roots = Vec::new();

    for line in lines {
        let uuid = line
            .uuid
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        if let Some(parent) = line.parent_uuid.clone() {
            children.entry(parent).or_default().push(uuid.clone());
        } else {
            roots.push(uuid.clone());
        }
        by_uuid.insert(uuid, line);
    }

    if roots.is_empty() {
        return by_uuid.into_values().collect();
    }

    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<String> = roots;
    stack.reverse();

    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(line) = by_uuid.remove(&id) {
            ordered.push(line);
        }
        if let Some(mut kids) = children.remove(&id) {
            kids.reverse();
            stack.extend(kids);
        }
    }

    for line in by_uuid.into_values() {
        ordered.push(line);
    }

    ordered
}

fn normalize_role(role: &str) -> String {
    match role {
        "user" => "user".to_string(),
        "assistant" => "assistant".to_string(),
        "tool" | "tool_result" => "tool".to_string(),
        _ => "assistant".to_string(),
    }
}

fn extract_claude_parts(message: Option<&ClaudeMessage>) -> Vec<MessagePart> {
    let Some(message) = message else {
        return Vec::new();
    };
    let Some(content) = message.content.as_ref() else {
        return Vec::new();
    };

    match content {
        Value::String(text) if !text.is_empty() => vec![MessagePart {
            part_type: "text".to_string(),
            text: Some(text.clone()),
            name: None,
            input: None,
        }],
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match block_type {
                    "text" => Some(MessagePart {
                        part_type: "text".to_string(),
                        text: block.get("text").and_then(|v| v.as_str()).map(str::to_string),
                        name: None,
                        input: None,
                    }),
                    "tool_use" => Some(MessagePart {
                        part_type: "tool_call".to_string(),
                        text: block.get("text").and_then(|v| v.as_str()).map(str::to_string),
                        name: block.get("name").and_then(|v| v.as_str()).map(str::to_string),
                        input: block.get("input").cloned(),
                    }),
                    "tool_result" => Some(MessagePart {
                        part_type: "tool_result".to_string(),
                        text: block
                            .get("content")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        name: None,
                        input: None,
                    }),
                    "thinking" => Some(MessagePart {
                        part_type: "thinking".to_string(),
                        text: block.get("thinking").and_then(|v| v.as_str()).map(str::to_string),
                        name: None,
                        input: None,
                    }),
                    _ => None,
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_parent_child_chain() {
        let lines = vec![
            ClaudeLine {
                line_type: Some("assistant".to_string()),
                uuid: Some("b".to_string()),
                parent_uuid: Some("a".to_string()),
                message: None,
                timestamp: None,
            },
            ClaudeLine {
                line_type: Some("user".to_string()),
                uuid: Some("a".to_string()),
                parent_uuid: None,
                message: None,
                timestamp: None,
            },
        ];

        let ordered = order_claude_lines(lines);
        assert_eq!(ordered[0].uuid.as_deref(), Some("a"));
        assert_eq!(ordered[1].uuid.as_deref(), Some("b"));
    }
}
