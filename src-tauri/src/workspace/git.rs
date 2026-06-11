use std::path::{Path, PathBuf};
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

    if let Some(list) = run_git(cwd, &["ls-files", "--others", "--exclude-standard"]) {
        for rel in list.lines().map(str::trim).filter(|s| !s.is_empty()) {
            let path = cwd.join(rel);
            if !path.is_file() {
                continue;
            }
            files += 1;
            insertions += count_text_lines(&path);
        }
    }

    (files, insertions, deletions)
}

fn count_text_lines(path: &Path) -> u32 {
    match std::fs::read(path) {
        Ok(bytes) if bytes.contains(&0) => 1,
        Ok(bytes) => String::from_utf8_lossy(&bytes).lines().count().max(1) as u32,
        Err(_) => 0,
    }
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

fn rel_path(cwd: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(cwd)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn expand_changes(cwd: &Path, changes: Vec<GitChange>) -> Vec<GitChange> {
    let mut out = Vec::new();
    for change in changes {
        let rel = change.path.trim_end_matches('/');
        let full = cwd.join(rel);
        if full.is_dir() {
            let mut files = Vec::new();
            collect_files_recursive(&full, &mut files);
            files.sort();
            if files.is_empty() {
                out.push(GitChange {
                    path: rel.to_string(),
                    status: change.status.clone(),
                    staged: change.staged,
                });
                continue;
            }
            for file in files {
                if let Some(path) = rel_path(cwd, &file) {
                    out.push(GitChange {
                        path,
                        status: change.status.clone(),
                        staged: change.staged,
                    });
                }
            }
        } else {
            out.push(GitChange {
                path: rel.to_string(),
                status: change.status,
                staged: change.staged,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    out
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
        .map(|raw| expand_changes(path, raw))
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

pub fn commit_changes(workspace_path: &str, message: &str) -> AppResult<()> {
    let msg = message.trim();
    if msg.is_empty() {
        return Err(AppError::from("提交说明不能为空"));
    }
    let path = Path::new(workspace_path);
    if !path.exists() {
        return Err(AppError::from("工作区路径不存在"));
    }
    if !is_git_repo(path) {
        return Err(AppError::from("不是 Git 仓库"));
    }
    run_git_stderr(path, &["add", "-A"]).map_err(AppError::from)?;
    match run_git_stderr(path, &["commit", "-m", msg]) {
        Ok(_) => Ok(()),
        Err(e) if e.contains("nothing to commit") || e.contains("无文件要提交") => {
            Err(AppError::from("没有可提交的变更"))
        }
        Err(e) => Err(AppError::from(e)),
    }
}

pub fn push_branch(workspace_path: &str) -> AppResult<()> {
    let path = Path::new(workspace_path);
    if !path.exists() {
        return Err(AppError::from("工作区路径不存在"));
    }
    if !is_git_repo(path) {
        return Err(AppError::from("不是 Git 仓库"));
    }
    if run_git_stderr(path, &["push"]).is_ok() {
        return Ok(());
    }
    let branch = run_git(path, &["branch", "--show-current"])
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .ok_or_else(|| AppError::from("无法获取当前分支"))?;
    run_git_stderr(path, &["push", "-u", "origin", &branch]).map_err(AppError::from)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffResult {
    pub path: String,
    pub diff: String,
    pub is_new_file: bool,
    pub is_deleted: bool,
}

/// 工作区文件相对 HEAD 的统一 diff（含未跟踪新文件）
pub fn file_diff(workspace_path: &str, file_path: &str) -> AppResult<FileDiffResult> {
    let cwd = Path::new(workspace_path);
    if !cwd.is_dir() {
        return Err(AppError::from("工作区路径无效"));
    }
    if !is_git_repo(cwd) {
        return Err(AppError::from("不是 Git 仓库"));
    }

    let rel = file_path.trim().trim_start_matches("./").trim_end_matches('/');
    if rel.is_empty() {
        return Err(AppError::from("文件路径为空"));
    }

    let full = cwd.join(rel);
    if full.is_dir() {
        return directory_diff(cwd, rel, &full);
    }

    file_diff_single(cwd, rel)
}

fn directory_diff(cwd: &Path, rel: &str, dir: &Path) -> AppResult<FileDiffResult> {
    let mut files = Vec::new();
    collect_files_recursive(dir, &mut files);
    files.sort();

    if files.is_empty() {
        return Err(AppError::from(format!("目录为空: {rel}")));
    }

    let mut combined = String::new();
    let mut is_new_file = false;
    let mut is_deleted = true;

    for file in files {
        let Some(rel_file) = rel_path(cwd, &file) else {
            continue;
        };
        match file_diff_single(cwd, &rel_file) {
            Ok(part) => {
                if !part.diff.is_empty() {
                    combined.push_str(&part.diff);
                    if !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                }
                is_new_file = is_new_file || part.is_new_file;
                is_deleted = is_deleted && part.is_deleted;
            }
            Err(_) => continue,
        }
    }

    if combined.is_empty() {
        return Err(AppError::from(format!("无法生成 diff: {rel}")));
    }

    Ok(FileDiffResult {
        path: rel.to_string(),
        diff: combined,
        is_new_file,
        is_deleted: false,
    })
}

fn file_diff_single(cwd: &Path, rel: &str) -> AppResult<FileDiffResult> {
    let full = cwd.join(rel);
    if full.exists() {
        if let Some(diff) = run_git(cwd, &["diff", "--no-color", "HEAD", "--", rel]) {
            if !diff.is_empty() {
                return Ok(FileDiffResult {
                    path: rel.to_string(),
                    diff,
                    is_new_file: false,
                    is_deleted: false,
                });
            }
        }
        if full.is_file() {
            let content = std::fs::read_to_string(&full)?;
            return Ok(FileDiffResult {
                path: rel.to_string(),
                diff: format_untracked_diff(rel, &content),
                is_new_file: true,
                is_deleted: false,
            });
        }
    }

    if let Some(diff) = run_git(cwd, &["diff", "--no-color", "HEAD", "--", rel]) {
        if !diff.is_empty() {
            return Ok(FileDiffResult {
                path: rel.to_string(),
                diff,
                is_new_file: false,
                is_deleted: !full.exists(),
            });
        }
    }

    Err(AppError::from(format!("无法生成 diff: {rel}")))
}

fn format_untracked_diff(path: &str, content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len().max(1)
    );
    if content.is_empty() {
        out.push_str("\\ No newline at end of file\n");
    } else {
        for line in lines {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        if !content.ends_with('\n') {
            out.push_str("\\ No newline at end of file\n");
        }
    }
    out
}

#[cfg(test)]
mod file_diff_tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn file_diff_for_modified_tracked_file() {
        let tmp = std::env::temp_dir().join(format!("warp-ade-git-diff-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&tmp)
            .status()
            .unwrap();
        std::fs::write(tmp.join("hello.txt"), "hello\n").unwrap();
        Command::new("git")
            .args(["add", "hello.txt"])
            .current_dir(&tmp)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-qm", "init"])
            .current_dir(&tmp)
            .status()
            .unwrap();
        std::fs::write(tmp.join("hello.txt"), "hello world\n").unwrap();

        let result = file_diff(tmp.to_str().unwrap(), "hello.txt").unwrap();
        assert!(result.diff.contains("+hello world"));
        assert!(result.diff.contains("-hello"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_diff_for_untracked_directory() {
        let tmp = std::env::temp_dir().join(format!("warp-ade-git-dir-{}", uuid::Uuid::new_v4()));
        let nested = tmp.join("skills/web-scraper/src");
        std::fs::create_dir_all(&nested).unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&tmp)
            .status()
            .unwrap();
        std::fs::write(nested.join("index.ts"), "export const ok = true;\n").unwrap();

        let result = file_diff(tmp.to_str().unwrap(), "skills/web-scraper").unwrap();
        assert!(result.diff.contains("index.ts"));
        assert!(result.is_new_file);

        let expanded = expand_changes(
            &tmp,
            parse_status_porcelain("?? skills/web-scraper/\n"),
        );
        assert!(expanded.iter().any(|c| c.path.ends_with("index.ts")));

        let _ = std::fs::remove_dir_all(&tmp);
    }
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
