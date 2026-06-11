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
pub struct CodexImportCandidate {
    pub source_path: String,
    pub project_slug: String,
    pub session_id: String,
    pub title: String,
    pub message_count_estimate: usize,
    pub modified_at: i64,
    pub already_imported: bool,
    pub workspace_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    role: Option<String>,
    content: Option<Value>,
    message: Option<Value>,
    payload: Option<Value>,
    cwd: Option<String>,
}

pub fn scan_codex_rollouts(db: &Database) -> AppResult<Vec<CodexImportCandidate>> {
    let imported = db.imported_source_paths()?;
    let home = dirs::home_dir().ok_or_else(|| AppError::from("无法定位用户目录"))?;
    let root = home.join(".codex").join("sessions");
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
        if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        let session_id = name
            .trim_start_matches("rollout-")
            .trim_end_matches(".jsonl")
            .to_string();
        let workspace_path = peek_codex_cwd(path);
        let project_slug = workspace_path
            .as_deref()
            .map(slug_from_path)
            .unwrap_or_else(|| "codex".to_string());
        let modified_at = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
            .unwrap_or(0);
        let message_count_estimate = count_lines(path)?;
        let already_imported = imported.contains(&path_str);
        let title = format!("Codex · {session_id}");

        candidates.push(CodexImportCandidate {
            source_path: path_str,
            project_slug,
            session_id,
            title,
            message_count_estimate,
            modified_at,
            already_imported,
            workspace_path,
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

pub fn search_codex_rollouts(
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
    let root = home.join(".codex").join("sessions");
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
        if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
            continue;
        }

        let Some(matched_text) = find_codex_user_match(path, trimmed)? else {
            continue;
        };

        let path_str = path.to_string_lossy().to_string();
        let session_id = name
            .trim_start_matches("rollout-")
            .trim_end_matches(".jsonl")
            .to_string();
        let workspace_path = peek_codex_cwd(path);
        let project_slug = workspace_path
            .as_deref()
            .map(slug_from_path)
            .unwrap_or_else(|| "codex".to_string());
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

fn find_codex_user_match(path: &Path, query: &str) -> AppResult<Option<String>> {
    use crate::import::search::{line_within_limit, text_matches_query};

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() || !line_within_limit(&line) {
            continue;
        }
        let parsed: CodexLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role = parsed.role.or_else(|| {
            parsed
                .message
                .as_ref()
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                .map(str::to_string)
        });
        let Some(role) = role else { continue };
        if role != "user" {
            continue;
        }

        let content = parsed
            .content
            .or(parsed.message)
            .or(parsed.payload.and_then(|p| p.get("message").cloned()));
        let text = extract_codex_parts(content.as_ref())
            .into_iter()
            .filter_map(|p| p.text)
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            continue;
        }
        if text_matches_query(&text, query) {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

pub fn import_codex_file(db: &Database, source_path: &str) -> AppResult<Session> {
    let path = PathBuf::from(source_path);
    if db.session_exists_by_source(source_path)? {
        return Err(AppError::from("session already imported"));
    }

    let workspace_path = peek_codex_cwd(&path);
    let project_slug = workspace_path
        .as_deref()
        .map(slug_from_path)
        .unwrap_or_else(|| "codex".to_string());
    let project_name = crate::import::projects::project_name_from_path(
        workspace_path.as_deref(),
        Some(&project_slug),
    );
    let project_id = crate::import::projects::ensure_project(
        db,
        &project_name,
        workspace_path.as_deref(),
        Some(&project_slug),
        "codex",
    )?;

    let session = db.create_session(
        &format!(
            "Codex · {}",
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("session")
        ),
        "codex",
        Some(source_path),
        Some(&project_slug),
        workspace_path.as_deref(),
        Some(&project_id),
        None,
    )?;

    let messages = parse_codex_jsonl(&path, &session.id)?;
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

fn slug_from_path(path: &str) -> String {
    path.trim_start_matches('/')
        .replace('/', "-")
        .chars()
        .take(120)
        .collect()
}

fn peek_codex_cwd(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().take(20) {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: CodexLine = serde_json::from_str(&line).ok()?;
        if let Some(cwd) = parsed.cwd.filter(|c| !c.is_empty()) {
            return Some(cwd);
        }
        if parsed.line_type.as_deref() == Some("session_meta") {
            if let Some(cwd) = parsed
                .payload
                .as_ref()
                .and_then(|p| p.get("cwd"))
                .and_then(|v| v.as_str())
            {
                return Some(cwd.to_string());
            }
        }
    }
    None
}

fn count_lines(path: &Path) -> AppResult<usize> {
    let file = File::open(path)?;
    Ok(BufReader::new(file).lines().count())
}

fn parse_codex_jsonl(path: &Path, session_id: &str) -> AppResult<Vec<CanonicalMessage>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut seq = 0i64;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: CodexLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role = parsed.role.or_else(|| {
            parsed
                .message
                .as_ref()
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                .map(str::to_string)
        });

        let content = parsed
            .content
            .or(parsed.message)
            .or(parsed.payload.and_then(|p| p.get("message").cloned()));

        let Some(role) = role else { continue };
        if role == "system" {
            continue;
        }

        let parts = extract_codex_parts(content.as_ref());
        if parts.is_empty() {
            continue;
        }

        messages.push(CanonicalMessage {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            seq,
            role: if role == "tool" {
                "tool".to_string()
            } else {
                role
            },
            parts,
            timestamp: None,
            metadata: serde_json::json!({ "source": "codex" }),
        });
        seq += 1;
    }

    Ok(messages)
}

fn extract_codex_parts(content: Option<&Value>) -> Vec<MessagePart> {
    let Some(content) = content else {
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
                let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("text");
                match block_type {
                    "text" | "input_text" | "output_text" => Some(MessagePart {
                        part_type: "text".to_string(),
                        text: block.get("text").and_then(|v| v.as_str()).map(str::to_string),
                        name: None,
                        input: None,
                    }),
                    "tool_call" | "tool_use" | "function_call" => Some(MessagePart {
                        part_type: "tool_call".to_string(),
                        text: block.get("text").and_then(|v| v.as_str()).map(str::to_string),
                        name: block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        input: block.get("input").cloned(),
                    }),
                    _ => block.get("text").and_then(|v| v.as_str()).map(|text| MessagePart {
                        part_type: "text".to_string(),
                        text: Some(text.to_string()),
                        name: None,
                        input: None,
                    }),
                }
            })
            .collect(),
        Value::Object(_) => content
            .get("content")
            .map(|inner| extract_codex_parts(Some(inner)))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}
