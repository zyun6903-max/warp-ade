pub mod git;
pub mod paths;
pub mod picker;

pub use git::{
    checkout_branch, commit_changes, file_diff, inspect_workspace, push_branch, FileDiffResult,
    WorkspaceInfo,
};
pub use paths::ensure_workspace_directory;
pub use picker::pick_directory;
