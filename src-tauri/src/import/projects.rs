use std::path::PathBuf;

use crate::error::AppResult;
use crate::storage::db::Database;

pub fn ensure_project(
    db: &Database,
    name: &str,
    workspace_path: Option<&str>,
    source_slug: Option<&str>,
    source_origin: &str,
) -> AppResult<String> {
    db.get_or_create_project(name, workspace_path, source_slug, source_origin)
}

pub fn normalize_project_slug(slug: &str) -> &str {
    slug.trim_start_matches('-')
}

/// Cursor / Claude encode absolute paths by replacing `/` with `-`, but folder names may
/// still contain hyphens (e.g. `workorder-service`). Blind replacement breaks those
/// paths; resolve segments by checking which merged token groups exist on disk.
pub fn decode_cursor_project_slug(slug: &str) -> Option<String> {
    let slug = normalize_project_slug(slug);
    if !slug.starts_with("Users-") {
        return None;
    }

    let parts: Vec<&str> = slug.split('-').collect();
    if parts.len() < 3 || parts[0] != "Users" {
        return None;
    }

    let mut path = PathBuf::from("/Users");
    path.push(parts[1]);

    let mut idx = 2;
    while idx < parts.len() {
        let mut merged = parts[idx].to_string();
        let mut end = idx;
        let mut found = path.join(&merged).is_dir();

        while !found && end + 1 < parts.len() {
            end += 1;
            merged.push('-');
            merged.push_str(parts[end]);
            found = path.join(&merged).is_dir();
        }

        if !found {
            merged = parts[idx..].join("-");
        }

        path.push(&merged);
        idx = if found { end + 1 } else { parts.len() };
    }

    Some(path.to_string_lossy().to_string())
}

pub fn project_name_from_path(workspace_path: Option<&str>, slug: Option<&str>) -> String {
    if let Some(path) = workspace_path.filter(|p| !p.is_empty()) {
        let trimmed = path.trim_end_matches('/');
        if let Some(name) = trimmed.rsplit('/').next() {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    if let Some(slug) = slug.filter(|s| !s.is_empty() && *s != "unknown") {
        if let Some(path) = decode_cursor_project_slug(slug) {
            return project_name_from_path(Some(&path), None);
        }
        return slug.chars().take(32).collect();
    }
    "未命名项目".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_workorder_service_slug() {
        let decoded = decode_cursor_project_slug("Users-zhangyun-DTC-workorder-service");
        assert_eq!(
            decoded.as_deref(),
            Some("/Users/zhangyun/DTC/workorder-service")
        );
        assert_eq!(
            project_name_from_path(decoded.as_deref(), Some("Users-zhangyun-DTC-workorder-service")),
            "workorder-service"
        );
    }

    #[test]
    fn decode_claude_slug_with_leading_dash() {
        let decoded = decode_cursor_project_slug("-Users-zhangyun-DTC-workorder-service");
        assert_eq!(
            decoded.as_deref(),
            Some("/Users/zhangyun/DTC/workorder-service")
        );
    }

    #[test]
    fn decode_warp_ade_slug() {
        let decoded = decode_cursor_project_slug("Users-zhangyun-Code-warp-ade");
        assert_eq!(decoded.as_deref(), Some("/Users/zhangyun/Code/warp-ade"));
    }
}
