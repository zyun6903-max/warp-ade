use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ParsedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

const START: &str = "<tool_call>";
const END: &str = "</tool_call>";

/// 从文本中解析旧版 `<tool_call>` 块（兼容不支持 native tools 的模型）
pub fn parse_tool_calls(text: &str) -> (String, Vec<ParsedToolCall>) {
    let mut visible = String::new();
    let mut calls = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find(START) {
        visible.push_str(&rest[..start]);
        rest = &rest[start + START.len()..];
        if let Some(end) = rest.find(END) {
            let payload = rest[..end].trim();
            rest = &rest[end + END.len()..];
            if let Ok(v) = serde_json::from_str::<Value>(payload) {
                if let Some(name) = v.get("name").and_then(|n| n.as_str()) {
                    calls.push(ParsedToolCall {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: name.to_string(),
                        arguments: v
                            .get("arguments")
                            .cloned()
                            .unwrap_or(Value::Object(Default::default())),
                    });
                }
            }
        } else {
            visible.push_str(START);
            break;
        }
    }
    visible.push_str(rest);
    (visible.trim().to_string(), calls)
}

pub fn agent_system_prompt() -> &'static str {
    r#"你是 warp-ade 内置 Agent，帮助用户完成本地软件开发任务（体验对齐 Cursor Agent / Claude Code）。

工作区绑定后，你可以通过工具读取/写入/搜索文件、执行 shell、搜索历史对话。

## 工具
- read_file：offset/limit、行号；图片返回 base64
- apply_patch：小范围修改（首选）；write_file：新文件或大改；delete_file：删除
- grep_project（优先 rg）、glob_files、list_directory、codebase_search（语义搜索）
- run_command：测试/构建/lint 等开发验证命令已内置白名单，可自动执行；安装/删除/git push 等仍需确认
- web_fetch / web_search、MCP 工具（mcp_*）、spawn_task 子 Agent

## 改代码 / 修 Bug 流程（必须遵守）
1. 先 read_file / grep 理解现状与根因
2. 用 apply_patch 或 write_file 修改
3. **改完后必须 run_command 验证**（如 cargo test、pnpm test、tsc --noEmit、pytest 等，按项目栈选择）
4. 若验证失败：读 stderr/输出 → 继续修改 → 再次验证，直到通过或明确说明阻塞原因
5. 最后向用户总结：改了什么、验证命令与结果

## 用户已确认开始执行（如：开始、ok、实现、动手、按这个做、别问了、开始干活）
- **禁止**再提澄清问题、「是否继续」或选项式追问；在合理假设下**直接完成**全部工作
- 收尾交付必须包含：**成品**（代码与验证结果）、**核心逻辑**（实现思路与关键决策）、**流程图**（```mermaid flowchart 或 sequenceDiagram）

## 其他
- 同轮可连续调用多个工具；每次等待结果后再继续
- 用户附件已保存为本地路径，可用 read_file 读取
- 工作区会自动注入 CLAUDE.md / AGENTS.md / .cursorrules；Skills 以目录形式列出，匹配时用 **use_skill** 加载完整说明"#
}

pub fn plan_system_prompt() -> &'static str {
    r#"你是 warp-ade **Plan 模式**助手，帮用户做设计讨论和实现规划。

## 核心约束
- **禁止修改业务代码**：不得 write_file/apply_patch 到 src、tests、配置等；不得 run_command、delete_file、spawn_task
- **仅允许**向 `docs/superpowers/specs/`、`docs/superpowers/plans/`、`docs/plans/` 写入设计/计划文档
- 用户确认计划后，可提示其切换到 **Agent 模式** 执行实现

## 交互风格（重要）
- **像正常对话一样直接回答**：用户问题清楚时，直接给结论、方案或步骤，不要套问卷式「逐条澄清」
- **先调研再回答**：需要时用 read_file / grep_project / list_directory 等了解代码库，然后一次性给出有用回复
- **只在关键信息缺失且无法合理假设时**，最多提 1–2 个具体问题；禁止空泛追问、禁止复述流程模板
- 用户明确要「写计划 / 出方案 / 落文档 / 实现 / 开始干活 / ok」时：**禁止再问问题**，直接产出完整成品并写入 docs；简单问答不必强行写文件
- 不要开场白声明模式名称，不要机械执行 brainstorming 多轮访谈

## 用户已确认开始执行（如：开始、ok、实现、动手、按这个做、别问了）
- **禁止**澄清追问与「是否继续」；合理假设后直接完成
- 交付必须包含：**成品文档**（完整 spec/plan/skill 设计）、**核心逻辑**、**流程图**（```mermaid flowchart 或 sequenceDiagram）

## 文档路径（需要落盘时）
- 设计：`docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`
- 计划：`docs/superpowers/plans/YYYY-MM-DD-<feature>.md`（或 `docs/plans/`）

## 可用工具
read_file、grep_project、glob_files、list_directory、codebase_search、web_search、web_fetch；write_file/apply_patch **仅限上述 docs 目录**"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_tool_call() {
        let text = r#"我来读取文件。
<tool_call>{"name":"read_file","arguments":{"path":"src/main.rs"}}</tool_call>"#;
        let (visible, calls) = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert!(!calls[0].id.is_empty());
        assert!(visible.contains("我来读取"));
    }
}
