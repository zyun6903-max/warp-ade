//! Plan 模式：可选加载 writing-plans 作为计划格式参考

use std::path::{Path, PathBuf};

use crate::agent::project_context::{
    parse_skill_entry, strip_frontmatter, truncate_chars, SkillEntry, MAX_SINGLE_FILE,
    MAX_SKILLS_CHARS,
};

const PLAN_SKILL_NAMES: &[&str] = &["writing-plans"];

/// 从本机 superpowers / compound-engineering / ~/.claude/skills 加载规划相关技能
pub fn load_plan_mode_superpowers_skills() -> Vec<SkillEntry> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in superpowers_skill_roots() {
        for name in PLAN_SKILL_NAMES {
            if seen.contains(*name) {
                continue;
            }
            let skill_md = root.join(name).join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            if let Ok(entry) = parse_skill_entry(&skill_md, "superpowers") {
                seen.insert(*name);
                found.push(entry);
            }
        }
    }

    found.sort_by(|a, b| {
        let ai = PLAN_SKILL_NAMES
            .iter()
            .position(|n| *n == a.name)
            .unwrap_or(99);
        let bi = PLAN_SKILL_NAMES
            .iter()
            .position(|n| *n == b.name)
            .unwrap_or(99);
        ai.cmp(&bi)
    });
    found
}

pub fn format_plan_skills_block(skills: &[SkillEntry]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let mut block = String::from("## 计划写作参考（可选）\n\n");
    block.push_str(
        "以下技能仅作参考；**按需**查阅其中的计划格式与粒度，不要机械执行多轮澄清流程。\n\n",
    );

    let mut used = 0usize;
    for skill in skills {
        if used >= MAX_SKILLS_CHARS {
            block.push_str("\n（其余技能因长度限制未注入，请用 read_file 读取 SKILL.md）\n");
            break;
        }
        let body = match std::fs::read_to_string(&skill.path) {
            Ok(s) => {
                let body = strip_frontmatter(&s);
                truncate_chars(&body, MAX_SINGLE_FILE.min(MAX_SKILLS_CHARS - used))
            }
            Err(_) => continue,
        };
        block.push_str(&format!("### Skill: {}\n\n", skill.name));
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

fn superpowers_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let Some(home) = dirs::home_dir() else {
        return roots;
    };

    push_skills_subdirs(
        &home.join(".claude/plugins/cache/claude-plugins-official/superpowers"),
        &mut roots,
    );
    push_skills_subdirs(
        &home.join(
            ".claude/plugins/cache/EveryInc-compound-engineering-plugin/compound-engineering",
        ),
        &mut roots,
    );
    let user_skills = home.join(".claude/skills");
    if user_skills.is_dir() {
        roots.push(user_skills);
    }
    roots
}

fn push_skills_subdirs(base: &Path, out: &mut Vec<PathBuf>) {
    if !base.is_dir() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let skills = entry.path().join("skills");
            if skills.is_dir() {
                out.push(skills);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_plan_skills_does_not_panic() {
        let _ = load_plan_mode_superpowers_skills();
    }
}
