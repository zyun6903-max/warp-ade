use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutsideReadPolicy {
    Block,
    Confirm,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutsideWritePolicy {
    Block,
    Confirm,
}

#[derive(Debug, Clone)]
pub struct WorkspacePathPolicy {
    pub outside_read: OutsideReadPolicy,
    pub outside_write: OutsideWritePolicy,
}

impl Default for WorkspacePathPolicy {
    fn default() -> Self {
        Self {
            outside_read: OutsideReadPolicy::Block,
            outside_write: OutsideWritePolicy::Block,
        }
    }
}

impl WorkspacePathPolicy {
    pub fn from_settings(read: &str, write: &str) -> Self {
        Self {
            outside_read: parse_read_policy(read),
            outside_write: parse_write_policy(write),
        }
    }
}

fn parse_read_policy(value: &str) -> OutsideReadPolicy {
    match value.trim().to_lowercase().as_str() {
        "allow" => OutsideReadPolicy::Allow,
        "confirm" => OutsideReadPolicy::Confirm,
        _ => OutsideReadPolicy::Block,
    }
}

fn parse_write_policy(value: &str) -> OutsideWritePolicy {
    match value.trim().to_lowercase().as_str() {
        "confirm" => OutsideWritePolicy::Confirm,
        _ => OutsideWritePolicy::Block,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathIntent {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDecision {
    Allow,
    Deny(&'static str),
    NeedsReadApproval,
    NeedsWriteApproval,
}

pub fn resolve_agent_path(workspace: Option<&Path>, input: &str) -> AppResult<(PathBuf, bool)> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AppError::from("路径不能为空"));
    }

    let path = Path::new(input);
    let target = if path.is_absolute() {
        PathBuf::from(input)
    } else if let Some(ws) = workspace {
        join_relative(ws, input)?
    } else {
        return Err(AppError::from("相对路径需要绑定工作区"));
    };

    let inside = workspace.is_some_and(|ws| target.starts_with(ws));
    Ok((target, inside))
}

fn join_relative(workspace: &Path, input: &str) -> AppResult<PathBuf> {
    let mut target = workspace.to_path_buf();
    for comp in Path::new(input).components() {
        match comp {
            Component::Normal(p) => target.push(p),
            Component::CurDir => {}
            Component::ParentDir => {
                if !target.pop() {
                    return Err(AppError::from("路径无效"));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::from("相对路径不能为绝对路径"));
            }
        }
    }
    Ok(target)
}

pub fn evaluate_path_access(
    inside: bool,
    intent: PathIntent,
    policy: &WorkspacePathPolicy,
) -> PathDecision {
    if inside {
        return PathDecision::Allow;
    }
    match intent {
        PathIntent::Read => match policy.outside_read {
            OutsideReadPolicy::Block => PathDecision::Deny("工作区外读取已禁用（可在设置中调整）"),
            OutsideReadPolicy::Confirm => PathDecision::NeedsReadApproval,
            OutsideReadPolicy::Allow => PathDecision::Allow,
        },
        PathIntent::Write => match policy.outside_write {
            OutsideWritePolicy::Block => PathDecision::Deny("工作区外写入已禁用（可在设置中调整）"),
            OutsideWritePolicy::Confirm => PathDecision::NeedsWriteApproval,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn inside_relative_path() {
        let ws = PathBuf::from("/project");
        let (p, inside) = resolve_agent_path(Some(&ws), "src/main.rs").unwrap();
        assert!(inside);
        assert_eq!(p, PathBuf::from("/project/src/main.rs"));
    }

    #[test]
    fn outside_absolute_path() {
        let ws = PathBuf::from("/project");
        let (p, inside) = resolve_agent_path(Some(&ws), "/etc/hosts").unwrap();
        assert!(!inside);
        assert_eq!(p, PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn parent_dir_can_escape_workspace() {
        let ws = PathBuf::from("/project/app");
        let (p, inside) = resolve_agent_path(Some(&ws), "../outside.txt").unwrap();
        assert!(!inside);
        assert_eq!(p, PathBuf::from("/project/outside.txt"));
    }

    #[test]
    fn block_outside_read_by_default() {
        let policy = WorkspacePathPolicy::default();
        assert_eq!(
            evaluate_path_access(false, PathIntent::Read, &policy),
            PathDecision::Deny("工作区外读取已禁用（可在设置中调整）")
        );
    }

    #[test]
    fn confirm_outside_write() {
        let policy = WorkspacePathPolicy {
            outside_read: OutsideReadPolicy::Allow,
            outside_write: OutsideWritePolicy::Confirm,
        };
        assert_eq!(
            evaluate_path_access(false, PathIntent::Write, &policy),
            PathDecision::NeedsWriteApproval
        );
    }
}
