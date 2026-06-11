use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};

pub(crate) const MAX_RULES_CHARS: usize = 32_000;
pub(crate) const MAX_SKILLS_CHARS: usize = 24_000;
pub(crate) const MAX_SINGLE_FILE: usize = 16_000;

const RULE_FILENAMES: &[&str] = &["CLAUDE.md", "AGENTS.md", ".cursorrules"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuleEntry {
    pub label: String,
    pub path: String,
    pub chars: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub path: String,
    pub source: String,
    pub chars: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContextBundle {
    pub workspace_path: String,
    pub rules: Vec<ProjectRuleEntry>,
    pub skills: Vec<SkillEntry>,
}

pub fn load_project_context(workspace: &Path) -> AppResult<ProjectContextBundle> {
    let workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    if !workspace.is_dir() {
        return Err(AppError::from(format!("工作区不存在: {}", workspace.display())));
    }

    let rules = collect_rule_files(&workspace)?;
    let mut skills = collect_skills(&workspace)?;
    skills.extend(collect_user_skills()?);

    Ok(ProjectContextBundle {
        workspace_path: workspace.to_string_lossy().to_string(),
        rules,
        skills,
    })
}

pub fn build_plan_system_prompt(
    summary: Option<&str>,
    project_ctx: Option<&ProjectContextBundle>,
    plan_skills: &[SkillEntry],
) -> String {
    let mut out = crate::agent::parser::plan_system_prompt().to_string();
    if let Some(block) = crate::agent::plan_mode::format_plan_skills_block(plan_skills) {
        out.push_str("\n\n");
        out.push_str(&block);
    }
    if let Some(sum) = summary.filter(|s| !s.trim().is_empty()) {
        out.push_str("\n\n");
        out.push_str(sum);
    }
    if let Some(ctx) = project_ctx {
        if let Some(block) = format_rules_block(ctx) {
            out.push_str("\n\n");
            out.push_str(&block);
        }
    }
    out
}

pub fn build_agent_system_prompt(
    base: &str,
    summary: Option<&str>,
    bundle: Option<&ProjectContextBundle>,
) -> String {
    let mut out = base.to_string();
    if let Some(sum) = summary.filter(|s| !s.trim().is_empty()) {
        out.push_str("\n\n");
        out.push_str(sum);
    }
    if let Some(ctx) = bundle {
        if let Some(block) = format_rules_block(ctx) {
            out.push_str("\n\n");
            out.push_str(&block);
        }
        if let Some(block) = format_skills_block(ctx) {
            out.push_str("\n\n");
            out.push_str(&block);
        }
    }
    out
}

fn format_rules_block(ctx: &ProjectContextBundle) -> Option<String> {
    if ctx.rules.is_empty() {
        return None;
    }
    let mut block = String::from("## 项目上下文（自动加载）\n\n");
    block.push_str("以下文件来自工作区及上级目录，请优先遵循其中的约定。\n\n");
    let mut used = 0usize;
    for rule in &ctx.rules {
        if used >= MAX_RULES_CHARS {
            block.push_str("\n（其余项目规则因长度限制未注入）\n");
            break;
        }
        let content = match std::fs::read_to_string(&rule.path) {
            Ok(s) => truncate_chars(&s, MAX_SINGLE_FILE.min(MAX_RULES_CHARS - used)),
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }
        block.push_str(&format!("### {} (`{}`)\n\n", rule.label, rule.path));
        block.push_str(&content);
        block.push_str("\n\n");
        used += content.chars().count();
    }
    Some(block.trim_end().to_string())
}

fn format_skills_block(ctx: &ProjectContextBundle) -> Option<String> {
    if ctx.skills.is_empty() {
        return None;
    }
    let mut block = String::from("## 可用 Skills（自动加载）\n\n");
    block.push_str(
        "当任务匹配某 Skill 的描述时，按该 Skill 的说明执行；可结合 read_file 读取 Skill 路径下的其他文件。\n\n",
    );
    let mut used = 0usize;
    for skill in &ctx.skills {
        if used >= MAX_SKILLS_CHARS {
            block.push_str("\n（其余 Skills 因长度限制未注入）\n");
            break;
        }
        let body = match std::fs::read_to_string(&skill.path) {
            Ok(s) => {
                let body = strip_frontmatter(&s);
                truncate_chars(&body, MAX_SINGLE_FILE.min(MAX_SKILLS_CHARS - used))
            }
            Err(_) => continue,
        };
        if body.trim().is_empty() && skill.description.trim().is_empty() {
            continue;
        }
        block.push_str(&format!(
            "### Skill: {}（{}）\n",
            skill.name, skill.source
        ));
        if !skill.description.trim().is_empty() {
            block.push_str(&format!("**何时使用：** {}\n\n", skill.description.trim()));
        }
        block.push_str(&format!("**路径：** `{}`\n\n", skill.path));
        if !body.trim().is_empty() {
            block.push_str(&body);
            block.push_str("\n\n");
            used += body.chars().count();
        }
    }
    Some(block.trim_end().to_string())
}

fn collect_rule_files(workspace: &Path) -> AppResult<Vec<ProjectRuleEntry>> {
    let mut entries = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    let stop = find_git_root(workspace).unwrap_or_else(|| workspace.to_path_buf());

    let mut dir = workspace.to_path_buf();
    loop {
        for name in RULE_FILENAMES {
            let path = dir.join(name);
            if path.is_file() {
                let key = path.to_string_lossy().to_string();
                if seen_paths.insert(key.clone()) {
                    entries.push(ProjectRuleEntry {
                        label: format!("{name} ({})", dir.file_name().unwrap_or_default().to_string_lossy()),
                        path: key,
                        chars: std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0),
                    });
                }
            }
        }

        if dir == stop {
            break;
        }
        if !dir.pop() {
            break;
        }
        if entries.len() > 32 {
            break;
        }
    }

    if let Ok(cursor_rules) = collect_cursor_rules(workspace) {
        for rule in cursor_rules {
            if seen_paths.insert(rule.path.clone()) {
                entries.push(rule);
            }
        }
    }

    Ok(entries)
}

fn collect_cursor_rules(workspace: &Path) -> AppResult<Vec<ProjectRuleEntry>> {
    let rules_dir = workspace.join(".cursor").join("rules");
    if !rules_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    collect_md_files(&rules_dir, &mut entries, 20)?;
    Ok(entries)
}

fn collect_md_files(dir: &Path, out: &mut Vec<ProjectRuleEntry>, limit: usize) -> AppResult<()> {
    if out.len() >= limit {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(&path, out, limit)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(ProjectRuleEntry {
                label: format!(
                    "Cursor rule ({})",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
                path: path.to_string_lossy().to_string(),
                chars: entry.metadata().map(|m| m.len() as usize).unwrap_or(0),
            });
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(())
}

fn collect_skills_from_dir(base: &Path, source: &str) -> AppResult<Vec<SkillEntry>> {
    if !base.is_dir() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.is_file() {
                skills.push(parse_skill_entry(&skill_md, source)?);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            skills.push(parse_skill_entry(&path, source)?);
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

fn collect_skills(workspace: &Path) -> AppResult<Vec<SkillEntry>> {
    collect_skills_from_dir(&workspace.join(".claude").join("skills"), "project")
}

fn collect_user_skills() -> AppResult<Vec<SkillEntry>> {
    let home = dirs::home_dir().ok_or_else(|| AppError::from("无法定位用户主目录"))?;
    collect_skills_from_dir(&home.join(".claude").join("skills"), "user")
}

pub(crate) fn parse_skill_entry(path: &Path, source: &str) -> AppResult<SkillEntry> {
    let raw = std::fs::read_to_string(path)?;
    let (name, description) = parse_skill_frontmatter(&raw, path);
    Ok(SkillEntry {
        name,
        description,
        path: path.to_string_lossy().to_string(),
        source: source.to_string(),
        chars: raw.chars().count(),
    })
}

fn parse_skill_frontmatter(raw: &str, path: &Path) -> (String, String) {
    let trimmed = raw.trim_start();
    let fallback_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_string());

    if !trimmed.starts_with("---") {
        return (fallback_name, String::new());
    }

    let rest = &trimmed[3..];
    let Some(end) = rest.find("\n---") else {
        return (fallback_name, String::new());
    };
    let front = &rest[..end];
    let mut name = fallback_name.clone();
    let mut description = String::new();
    for line in front.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = v.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }
    (name, description)
}

pub(crate) fn strip_frontmatter(raw: &str) -> String {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return raw.to_string();
    }
    let rest = &trimmed[3..];
    if let Some(end) = rest.find("\n---") {
        let after = &rest[end + 4..];
        return after.trim_start().to_string();
    }
    raw.to_string()
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..32 {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

pub(crate) fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("\n…（内容已截断）");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_skill_frontmatter() {
        let raw = r#"---
name: zipkin-trace
description: 查询 Zipkin 链路
---
# Body
"#;
        let (name, desc) = parse_skill_frontmatter(raw, Path::new("/tmp/skills/zipkin/SKILL.md"));
        assert_eq!(name, "zipkin-trace");
        assert!(desc.contains("Zipkin"));
        assert!(strip_frontmatter(raw).contains("# Body"));
    }

    #[test]
    fn loads_rules_and_skills_from_temp_workspace() {
        let tmp = std::env::temp_dir().join(format!("warp-ade-ctx-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(tmp.join(".claude/skills/demo-skill")).unwrap();
        fs::write(tmp.join("CLAUDE.md"), "# Project rules").unwrap();
        fs::write(
            tmp.join(".claude/skills/demo-skill/SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\n---\nDo demo things.",
        )
        .unwrap();

        let bundle = load_project_context(&tmp).unwrap();
        assert!(bundle.rules.iter().any(|r| r.path.ends_with("CLAUDE.md")));
        let project_skills: Vec<_> = bundle
            .skills
            .iter()
            .filter(|s| s.source == "project")
            .collect();
        assert_eq!(project_skills.len(), 1);
        assert_eq!(project_skills[0].name, "demo");

        let prompt = build_agent_system_prompt("base", None, Some(&bundle));
        assert!(prompt.contains("项目上下文"));
        assert!(prompt.contains("可用 Skills"));
        assert!(prompt.contains("Do demo things"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
