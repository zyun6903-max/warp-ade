use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::storage::db::app_data_dir;

const DISABLED_FILE: &str = "skills-disabled.json";

fn disabled_file_path() -> AppResult<PathBuf> {
    let base = app_data_dir()?;
    Ok(base.join(DISABLED_FILE))
}

pub fn load_disabled_paths() -> AppResult<HashSet<String>> {
    let path = disabled_file_path()?;
    if !path.is_file() {
        return Ok(HashSet::new());
    }
    let raw = fs::read_to_string(&path)?;
    let list: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
    Ok(list.into_iter().collect())
}

fn save_disabled_paths(paths: &HashSet<String>) -> AppResult<()> {
    let path = disabled_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut list: Vec<_> = paths.iter().cloned().collect();
    list.sort();
    fs::write(&path, serde_json::to_string_pretty(&list)?)?;
    Ok(())
}

pub fn set_skill_enabled(skill_path: &str, enabled: bool) -> AppResult<()> {
    let mut disabled = load_disabled_paths()?;
    if enabled {
        disabled.remove(skill_path);
    } else {
        disabled.insert(skill_path.to_string());
    }
    save_disabled_paths(&disabled)
}

pub fn is_skill_enabled(skill_path: &str) -> AppResult<bool> {
    Ok(!load_disabled_paths()?.contains(skill_path))
}

pub fn apply_enabled_filter(
    skills: Vec<crate::agent::project_context::SkillEntry>,
) -> AppResult<Vec<crate::agent::project_context::SkillEntry>> {
    let disabled = load_disabled_paths()?;
    Ok(skills
        .into_iter()
        .filter(|s| !disabled.contains(&s.path))
        .collect())
}

pub fn scan_all_skills(
    workspace: Option<&Path>,
) -> AppResult<Vec<crate::agent::project_context::SkillEntry>> {
    use crate::agent::project_context::{collect_skills, collect_user_skills};

    let mut skills = Vec::new();
    if let Some(ws) = workspace.filter(|p| p.is_dir()) {
        skills.extend(collect_skills(ws)?);
    }
    skills.extend(collect_user_skills()?);
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListItem {
    pub name: String,
    pub description: String,
    pub path: String,
    pub source: String,
    pub chars: usize,
    pub enabled: bool,
}

pub fn list_skill_items(workspace: Option<&Path>) -> AppResult<Vec<SkillListItem>> {
    let disabled = load_disabled_paths()?;
    Ok(scan_all_skills(workspace)?
        .into_iter()
        .map(|s| SkillListItem {
            enabled: !disabled.contains(&s.path),
            name: s.name,
            description: s.description,
            path: s.path,
            source: s.source,
            chars: s.chars,
        })
        .collect())
}

pub fn user_skills_root() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| AppError::from("无法定位用户主目录"))?;
    Ok(home.join(".claude").join("skills"))
}

pub fn ensure_under_user_skills(path: &Path) -> AppResult<PathBuf> {
    let root = user_skills_root()?;
    let root_canonical = root.canonicalize().unwrap_or(root);
    let canonical = path.canonicalize().map_err(|_| AppError::from("技能路径无效"))?;
    if !canonical.starts_with(&root_canonical) {
        return Err(AppError::from("只能删除用户目录下的技能"));
    }
    Ok(canonical)
}

pub fn delete_user_skill_dir(path: &str) -> AppResult<()> {
    let p = Path::new(path);
    let canonical = ensure_under_user_skills(p)?;
    if canonical.is_dir() {
        fs::remove_dir_all(&canonical)?;
    } else if canonical.is_file() {
        fs::remove_file(&canonical)?;
    } else {
        return Err(AppError::from("技能路径不存在"));
    }
    let mut disabled = load_disabled_paths()?;
    disabled.remove(path);
    save_disabled_paths(&disabled)?;
    Ok(())
}

pub fn reveal_in_file_manager(path: &str) -> AppResult<()> {
    let p = Path::new(path);
    let target = if p.is_file() {
        p.parent().map(Path::to_path_buf).unwrap_or_else(|| p.to_path_buf())
    } else {
        p.to_path_buf()
    };
    if !target.exists() {
        return Err(AppError::from("路径不存在"));
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&target)
            .status()
            .map_err(|e| AppError::from(format!("无法打开目录: {e}")))?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        return Err(AppError::from("当前平台暂不支持在文件管理器中打开"));
    }
    Ok(())
}
