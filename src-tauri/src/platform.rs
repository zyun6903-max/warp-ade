use std::path::Path;
use std::process::Command;

use crate::error::{AppError, AppResult};

/// 跨平台执行 Shell 命令（Unix: sh -lc，Windows: PowerShell）。
pub fn run_shell(command: &str, workspace: Option<&Path>) -> Result<std::process::Output, std::io::Error> {
    let mut cmd = shell_command(command);
    if let Some(ws) = workspace {
        cmd.current_dir(ws);
    }
    cmd.output()
}

fn shell_command(command: &str) -> Command {
    #[cfg(unix)]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(command);
        cmd
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(command);
        cmd
    }
}

/// 在系统文件管理器中打开路径。
pub fn reveal_in_file_manager(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::from("路径不存在"));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .status()
            .map_err(|e| AppError::from(format!("无法打开目录: {e}")))?;
    }

    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .status()
            .map_err(|e| AppError::from(format!("无法打开目录: {e}")))?;
    }

    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .status()
            .map_err(|e| AppError::from(format!("无法打开目录: {e}")))?;
    }

    Ok(())
}
