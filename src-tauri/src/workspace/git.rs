use std::path::Path;
use std::process::Command;

use regex::Regex;
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitChange {
    pub path: String,
    pub status: String,
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub workspace_path: Option<String>,
    pub is_git_repo: bool,
    pub branch: Option<String>,
    pub branches: Vec<String>,
    pub insertions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub unpushed_commits: u32,
    pub changes: Vec<GitChange>,
    pub github_authenticated: bool,
    pub github_auth_message: String,
    pub source: Option<String>,
    pub error: Option<String>,
}

fn run_git(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").current_dir(cwd).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
}

fn run_git_stderr(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists() || run_git(path, &["rev-parse", "--git-dir"]).is_some()
}

fn parse_shortstat(line: &str) -> (u32, u32, u32) {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(\d+) files? changed(?:, (\d+) insertions?\(\+\))?(?:, (\d+) deletions?\(-\))?",
        )
        .expect("shortstat regex")
    });

    let line = line.trim();
    let Some(caps) = re.captures(line) else {
        return (0, 0, 0);
    };

    let files = caps
        .get(1)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let insertions = caps
        .get(2)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    let deletions = caps
        .get(3)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    (files, insertions, deletions)
}

fn diff_stats(cwd: &Path) -> (u32, u32, u32) {
    let mut files = 0u32;
    let mut insertions = 0u32;
    let mut deletions = 0u32;

    if let Some(line) = run_git(cwd, &["diff", "--shortstat"]) {
        if !line.is_empty() {
            let (f, i, d) = parse_shortstat(&line);
            files += f;
            insertions += i;
            deletions += d;
        }
    }
    if let Some(line) = run_git(cwd, &["diff", "--cached", "--shortstat"]) {
        if !line.is_empty() {
            let (f, i, d) = parse_shortstat(&line);
            files += f;
            insertions += i;
            deletions += d;
        }
    }

    (files, insertions, deletions)
}

fn parse_tracking(line: &str) -> (Option<u32>, Option<u32>) {
    let ahead_re = Regex::new(r"ahead (\d+)").ok();
    let behind_re = Regex::new(r"behind (\d+)").ok();

    let ahead = ahead_re
        .as_ref()
        .and_then(|re| re.captures(line))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok());
    let behind = behind_re
        .as_ref()
        .and_then(|re| re.captures(line))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok());

    (ahead, behind)
}

fn list_branches(cwd: &Path) -> Vec<String> {
    run_git(
        cwd,
        &["branch", "--format=%(refname:short)", "--sort=-committerdate"],
    )
    .map(|raw| {
        raw
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

fn parse_status_porcelain(raw: &str) -> Vec<GitChange> {
    raw.lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let index_status = line.get(..2)?.trim_end();
            let path = line.get(3..)?.trim();
            if path.is_empty() {
                return None;
            }

            let staged = index_status.chars().next().is_some_and(|c| c != ' ' && c != '?');
            let worktree = index_status.chars().nth(1).unwrap_or(' ');
            let status = if index_status == "??" {
                "?"
            } else if staged && worktree != ' ' {
                "M"
            } else {
                index_status.trim()
            };

            Some(GitChange {
                path: path.to_string(),
                status: status.to_string(),
                staged,
            })
        })
        .collect()
}

fn github_auth_status() -> (bool, String) {
    match Command::new("gh").args(["auth", "status"]).output() {
        Ok(output) if output.status.success() => (true, "GitHub CLI 已通过身份验证".to_string()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not logged in") || stderr.contains("You are not logged in") {
                (false, "GitHub CLI 未通过身份验证".to_string())
            } else if !stderr.trim().is_empty() {
                (false, stderr.trim().to_string())
            } else {
                (false, "GitHub CLI 未通过身份验证".to_string())
            }
        }
        Err(_) => (false, "未安装 GitHub CLI".to_string()),
    }
}

pub fn inspect_workspace(workspace_path: Option<&str>, source: Option<&str>) -> WorkspaceInfo {
    let Some(path_str) = workspace_path.filter(|p| !p.trim().is_empty()) else {
        let (gh_ok, gh_msg) = github_auth_status();
        return WorkspaceInfo {
            workspace_path: None,
            is_git_repo: false,
            branch: None,
            branches: vec![],
            insertions: 0,
            deletions: 0,
            changed_files: 0,
            ahead: None,
            behind: None,
            unpushed_commits: 0,
            changes: vec![],
            github_authenticated: gh_ok,
            github_auth_message: gh_msg,
            source: source.map(str::to_string),
            error: Some("未配置工作区路径".to_string()),
        };
    };

    let path = Path::new(path_str);
    if !path.exists() {
        let (gh_ok, gh_msg) = github_auth_status();
        return WorkspaceInfo {
            workspace_path: Some(path_str.to_string()),
            is_git_repo: false,
            branch: None,
            branches: vec![],
            insertions: 0,
            deletions: 0,
            changed_files: 0,
            ahead: None,
            behind: None,
            unpushed_commits: 0,
            changes: vec![],
            github_authenticated: gh_ok,
            github_auth_message: gh_msg,
            source: source.map(str::to_string),
            error: Some("工作区路径不存在".to_string()),
        };
    }

    if !is_git_repo(path) {
        let (gh_ok, gh_msg) = github_auth_status();
        return WorkspaceInfo {
            workspace_path: Some(path_str.to_string()),
            is_git_repo: false,
            branch: None,
            branches: vec![],
            insertions: 0,
            deletions: 0,
            changed_files: 0,
            ahead: None,
            behind: None,
            unpushed_commits: 0,
            changes: vec![],
            github_authenticated: gh_ok,
            github_auth_message: gh_msg,
            source: source.map(str::to_string),
            error: Some("不是 Git 仓库".to_string()),
        };
    }

    let branch = run_git(path, &["branch", "--show-current"]);
    let branches = list_branches(path);
    let (files, insertions, deletions) = diff_stats(path);

    let status_sb = run_git(path, &["status", "-sb"]).unwrap_or_default();
    let tracking_line = status_sb.lines().next().unwrap_or("");
    let (ahead, behind) = parse_tracking(tracking_line);

    let unpushed_commits = ahead.unwrap_or(0);

    let changes = run_git(path, &["status", "--porcelain"])
        .map(|raw| parse_status_porcelain(&raw))
        .unwrap_or_default();

    let changed_files = if files > 0 {
        files
    } else {
        changes.len() as u32
    };

    let (gh_ok, gh_msg) = github_auth_status();

    WorkspaceInfo {
        workspace_path: Some(path_str.to_string()),
        is_git_repo: true,
        branch,
        branches,
        insertions,
        deletions,
        changed_files,
        ahead,
        behind,
        unpushed_commits,
        changes,
        github_authenticated: gh_ok,
        github_auth_message: gh_msg,
        source: source.map(str::to_string),
        error: None,
    }
}

pub fn checkout_branch(workspace_path: &str, branch: &str) -> AppResult<()> {
    let path = Path::new(workspace_path);
    if !path.exists() {
        return Err(AppError::from("工作区路径不存在"));
    }
    run_git_stderr(path, &["checkout", branch]).map_err(AppError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_shortstat_line() {
        let (f, i, d) = parse_shortstat(" 3 files changed, 10 insertions(+), 2 deletions(-)");
        assert_eq!(f, 3);
        assert_eq!(i, 10);
        assert_eq!(d, 2);
    }

    #[test]
    fn parse_tracking_line() {
        let (a, b) = parse_tracking("## main...origin/main [ahead 2, behind 1]");
        assert_eq!(a, Some(2));
        assert_eq!(b, Some(1));
    }

    #[test]
    fn parse_porcelain() {
        let changes = parse_status_porcelain(" M src/main.rs\n?? new.txt");
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].path, "src/main.rs");
    }
}
