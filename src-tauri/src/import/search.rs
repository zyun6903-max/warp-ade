use std::path::Path;
use std::time::SystemTime;

use serde::Serialize;

const MAX_LINE_BYTES: usize = 512 * 1024;
const MAX_MATCH_TEXT_CHARS: usize = 16_384;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceSearchHit {
    pub source_path: String,
    pub project_slug: String,
    pub session_id: String,
    pub modified_at: i64,
    pub already_imported: bool,
    pub workspace_path: Option<String>,
    pub matched_preview: String,
}

pub fn file_modified_epoch(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn text_matches_query(text: &str, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() || text.is_empty() {
        return false;
    }
    let sample = truncate_chars(text, MAX_MATCH_TEXT_CHARS);
    if sample.contains(q) {
        return true;
    }
    sample.to_lowercase().contains(&q.to_lowercase())
}

pub fn preview_snippet(text: &str, query: &str, max_len: usize) -> String {
    let q = query.trim();
    let compact: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let sample = truncate_chars(&compact, MAX_MATCH_TEXT_CHARS);
    if sample.is_empty() {
        return String::new();
    }

    let max_len = max_len.max(1);
    let chars: Vec<char> = sample.chars().collect();
    let mut pos = 0usize;
    if !q.is_empty() {
        for i in 0..chars.len() {
            let rest: String = chars[i..].iter().collect();
            if text_matches_query(&rest, q) {
                pos = i;
                break;
            }
        }
    }

    let q_len = q.chars().count();
    let start = pos.saturating_sub(40);
    let end = (pos + q_len + 120).min(chars.len());
    let take_len = end.saturating_sub(start).min(max_len);
    let snippet: String = chars.iter().skip(start).take(take_len).collect();
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < chars.len() { "…" } else { "" };
    format!("{prefix}{snippet}{suffix}")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

pub fn line_within_limit(line: &str) -> bool {
    line.len() <= MAX_LINE_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_chinese_substring() {
        assert!(text_matches_query("工单索赔流程怎么处理", "索赔"));
        assert!(!text_matches_query("工单流程", "索赔"));
    }

    #[test]
    fn snippet_highlights_context() {
        let text = "这是一段关于工单索赔流程的长文本说明";
        let snippet = preview_snippet(text, "索赔", 80);
        assert!(snippet.contains("索赔"));
    }

    #[test]
    fn preview_snippet_edge_cases_do_not_panic() {
        let cases = [
            ("", "索赔"),
            ("a", "索赔"),
            ("索赔", "索赔"),
            ("İ索赔", "i"),
            (&"索".repeat(20_000), "索赔"),
        ];
        for (text, q) in cases {
            let _ = preview_snippet(text, q, 240);
            let _ = text_matches_query(text, q);
        }
    }
}
