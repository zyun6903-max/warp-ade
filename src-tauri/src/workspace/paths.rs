use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// 规范化工作区路径；若目录不存在则递归创建。
pub fn ensure_workspace_directory(path: &str) -> AppResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::from("请填写工作区路径"));
    }

    let pb = PathBuf::from(trimmed);
    if pb.is_file() {
        return Err(AppError::from("路径指向文件，请选择目录"));
    }

    if !pb.exists() {
        std::fs::create_dir_all(&pb)
            .map_err(|e| AppError::from(format!("无法创建工作目录: {e}")))?;
    } else if !pb.is_dir() {
        return Err(AppError::from("路径不是有效目录"));
    }

    canonicalize_lossy(&pb)
}

fn canonicalize_lossy(path: &Path) -> AppResult<PathBuf> {
    if let Ok(canon) = path.canonicalize() {
        return Ok(canon);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Ok(parent_canon) = parent.canonicalize() {
            if let Some(name) = path.file_name() {
                return Ok(parent_canon.join(name));
            }
        }
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn creates_missing_directory() {
        let base = std::env::temp_dir().join(format!("warp-ade-ws-{}", uuid::Uuid::new_v4()));
        let nested = base.join("nested").join("project");
        let result = ensure_workspace_directory(nested.to_str().unwrap()).unwrap();
        assert!(result.is_dir());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn rejects_file_path() {
        let base = std::env::temp_dir().join(format!("warp-ade-file-{}", uuid::Uuid::new_v4()));
        fs::write(&base, "x").unwrap();
        let err = ensure_workspace_directory(base.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("文件"));
        let _ = fs::remove_file(base);
    }
}
