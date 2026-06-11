use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::storage::db::{CanonicalMessage, Database, MessagePart, Session};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorImportCandidate {
    pub source_path: String,
    pub project_slug: String,
    pub session_id: String,
    pub title: String,
    pub message_count_estimate: usize,
    pub modified_at: i64,
    pub already_imported: bool,
}

#[derive(Debug, Deserialize)]
struct CursorLine {
    role: Option<String>,
    message: Option<CursorMessage>,
}

#[derive(Debug, Deserialize)]
struct CursorMessage {
    content: Option<Vec<CursorContent>>,
}

#[derive(Debug, Deserialize)]
struct CursorContent {
    #[serde(rename = "type")]
    content_type: Option<String>,
    text: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

pub fn scan_cursor_transcripts(db: &Database) -> AppResult<Vec<CursorImportCandidate>> {
    let imported = db.imported_source_paths()?;
    let home = dirs::home_dir().ok_or_else(|| AppError::from("cannot resolve home directory"))?;
    let root = home.join(".cursor").join("projects");
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
        if !path_str.contains("agent-transcripts") {
            continue;
        }

        let project_slug = extract_project_slug(path);
        let session_id = name.trim_end_matches(".jsonl").to_string();
        let modified_at = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64)
            .unwrap_or(0);
        let message_count_estimate = count_lines(path)?;
        let already_imported = imported.contains(&path_str);
        let title = format!("Cursor · {session_id}");

        candidates.push(CursorImportCandidate {
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

pub fn search_cursor_transcripts(
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
    let home = dirs::home_dir().ok_or_else(|| AppError::from("cannot resolve home directory"))?;
    let root = home.join(".cursor").join("projects");
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
        let path_str = path.to_string_lossy().to_string();
        if !path_str.contains("agent-transcripts") {
            continue;
        }

        let Some(matched_text) = find_cursor_user_match(path, trimmed)? else {
            continue;
        };

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

fn find_cursor_user_match(path: &Path, query: &str) -> AppResult<Option<String>> {
    use crate::import::search::{line_within_limit, text_matches_query};

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() || !line_within_limit(&line) {
            continue;
        }
        let parsed: CursorLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(role) = parsed.role.as_deref() else {
            continue;
        };
        if role != "user" {
            continue;
        }
        let text = cursor_line_text(&parsed);
        if text.is_empty() {
            continue;
        }
        if text_matches_query(&text, query) {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

fn cursor_line_text(parsed: &CursorLine) -> String {
    let mut parts = Vec::new();
    if let Some(message) = parsed.message.as_ref() {
        if let Some(content) = message.content.as_ref() {
            for block in content {
                if block.content_type.as_deref() == Some("text") {
                    if let Some(text) = block.text.as_deref() {
                        if !text.is_empty() {
                            parts.push(text);
                        }
                    }
                }
            }
        }
    }
    parts.join(" ")
}

pub fn import_cursor_file(db: &Database, source_path: &str) -> AppResult<Session> {
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
        "cursor",
    )?;
    let session = db.create_session(
        &format!("Cursor · {}", path.file_stem().and_then(|s| s.to_str()).unwrap_or("session")),
        "cursor",
        Some(source_path),
        Some(&project_slug),
        workspace_path.as_deref(),
        Some(&project_id),
        None,
    )?;

    let messages = parse_cursor_jsonl(&path, &session.id)?;
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

fn parse_cursor_jsonl(path: &Path, session_id: &str) -> AppResult<Vec<CanonicalMessage>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut seq = 0i64;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: CursorLine = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(role) = parsed.role else { continue };
        let mut parts = Vec::new();
        if let Some(message) = parsed.message {
            if let Some(content) = message.content {
                for block in content {
                    match block.content_type.as_deref() {
                        Some("text") => parts.push(MessagePart {
                            part_type: "text".to_string(),
                            text: block.text,
                            name: None,
                            input: None,
                        }),
                        Some("tool_use") => parts.push(MessagePart {
                            part_type: "tool_call".to_string(),
                            text: block.text,
                            name: block.name,
                            input: block.input,
                        }),
                        _ => {}
                    }
                }
            }
        }
        if parts.is_empty() {
            continue;
        }
        messages.push(CanonicalMessage {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            seq,
            role,
            parts,
            timestamp: None,
            metadata: serde_json::json!({
                "source": "cursor",
                "subagent": path.to_string_lossy().contains("subagents"),
            }),
        });
        seq += 1;
    }
    Ok(messages)
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::storage::db::open_db;

    #[test]
    fn cursor_search_claim_no_panic() {
        let dir = std::env::temp_dir().join("warp-ade-search-test");
        std::fs::create_dir_all(&dir).ok();
        let db = open_db(&dir).expect("open db");
        let hits = search_cursor_transcripts(&db, "索赔", Some(50)).expect("search");
        assert!(hits.len() <= 50);
    }
}
