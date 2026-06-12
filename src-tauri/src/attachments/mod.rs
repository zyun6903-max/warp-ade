use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Serialize;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::storage::db::app_data_dir;

pub const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;
const MAX_IMAGE_BASE64_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachment {
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub kind: String,
    pub mime: String,
    pub size: u64,
}

pub fn save_attachment(
    session_id: &str,
    workspace_path: Option<&str>,
    file_name: &str,
    data: &[u8],
) -> AppResult<ChatAttachment> {
    if data.is_empty() {
        return Err(AppError::from("附件为空"));
    }
    if data.len() > MAX_ATTACHMENT_BYTES {
        return Err(AppError::from(format!(
            "附件过大（最大 {} MB）",
            MAX_ATTACHMENT_BYTES / 1024 / 1024
        )));
    }

    let safe_name = sanitize_filename(file_name);
    let id = Uuid::new_v4().to_string();
    let stored_name = format!("{id}_{safe_name}");
    let dir = attachment_dir(session_id, workspace_path)?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&stored_name);
    std::fs::write(&path, data)?;

    let mime = guess_mime(&safe_name);
    let kind = attachment_kind(&safe_name, &mime);

    Ok(ChatAttachment {
        id,
        path: path.to_string_lossy().to_string(),
        file_name: safe_name,
        kind,
        mime,
        size: data.len() as u64,
    })
}

pub fn attachment_dir(session_id: &str, workspace_path: Option<&str>) -> AppResult<PathBuf> {
    if let Some(ws) = workspace_path.filter(|s| !s.trim().is_empty()) {
        let ws = PathBuf::from(ws);
        if ws.is_dir() {
            return Ok(ws.join(".warp-ade").join("attachments").join(session_id));
        }
    }
    Ok(app_data_dir()?
        .join("attachments")
        .join(session_id))
}

pub fn read_image_for_agent(path: &Path) -> AppResult<String> {
    if !path.is_file() {
        return Err(AppError::from(format!("图片不存在: {}", path.display())));
    }
    let meta = std::fs::metadata(path)?;
    let size = meta.len() as usize;
    let mime = guess_mime(
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.png"),
    );
    let mut out = format!(
        "图片: {}\nMIME: {mime}\n大小: {size} 字节\n",
        path.display()
    );
    if size <= MAX_IMAGE_BASE64_BYTES {
        let bytes = std::fs::read(path)?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        out.push_str(&format!(
            "\nBase64（供视觉模型或进一步分析）:\ndata:{mime};base64,{b64}"
        ));
    } else {
        out.push_str("\n（图片较大，未嵌入 base64；路径可用于本地工具处理）");
    }
    Ok(out)
}

pub fn attachment_data_url(path: &str) -> AppResult<Option<String>> {
    let path = PathBuf::from(path);
    if !path.is_file() || !is_image_path(&path) {
        return Ok(None);
    }
    let size = std::fs::metadata(&path)?.len() as usize;
    if size > MAX_IMAGE_BASE64_BYTES {
        return Ok(None);
    }
    let mime = guess_mime(
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.png"),
    );
    let bytes = std::fs::read(&path)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:{mime};base64,{b64}")))
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico"
            )
        })
        .unwrap_or(false)
}

fn attachment_kind(file_name: &str, mime: &str) -> String {
    if mime.starts_with("image/") || is_image_path(Path::new(file_name)) {
        "image".into()
    } else if mime.starts_with("text/")
        || matches!(
            Path::new(file_name)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase()),
            Some(ext) if matches!(
                ext.as_str(),
                "md" | "txt" | "json" | "yaml" | "yml" | "toml" | "rs" | "ts" | "tsx" | "js"
                    | "jsx" | "py" | "go" | "sql" | "csv" | "xml" | "html" | "css"
            )
        )
    {
        "text".into()
    } else {
        "binary".into()
    }
}

fn guess_mime(file_name: &str) -> String {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "html" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment.bin");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "attachment.bin".into()
    } else {
        cleaned.chars().take(120).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_image_kind() {
        assert_eq!(
            attachment_kind("photo.png", "image/png"),
            "image".to_string()
        );
    }

    #[test]
    fn sanitizes_unsafe_filename() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
    }
}
