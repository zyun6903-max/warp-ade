use crate::error::{AppError, AppResult};

/// 打开文件夹选择器。macOS 使用 AppleScript，跟随系统语言（dev 裸二进制也生效）。
pub fn pick_directory(title: &str) -> AppResult<Option<String>> {
    #[cfg(target_os = "macos")]
    {
        return pick_directory_osascript(title);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let path = rfd::FileDialog::new().set_title(title).pick_folder();
        Ok(path.map(|p| p.to_string_lossy().into_owned()))
    }
}

#[cfg(target_os = "macos")]
fn pick_directory_osascript(prompt: &str) -> AppResult<Option<String>> {
    let escaped = prompt.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("POSIX path of (choose folder with prompt \"{escaped}\")");
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| AppError::from(format!("无法打开文件夹选择器: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        if err.contains("User canceled") || err.contains("-128") {
            return Ok(None);
        }
        return Err(AppError::from(format!("文件夹选择失败: {err}")));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Ok(None)
    } else {
        Ok(Some(path))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn osascript_escape_quotes() {
        let escaped = "选\"择".replace('\\', "\\\\").replace('"', "\\\"");
        assert!(escaped.contains("\\\""));
    }
}
