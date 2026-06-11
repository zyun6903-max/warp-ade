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
    r#"你是 warp-ade 内置 Agent，帮助用户完成本地软件开发任务。

工作区绑定后，你可以通过工具读取/写入/搜索文件、抓取网页、执行 shell、搜索历史对话。
- read_file 支持 offset/limit 与行号；图片返回 base64 描述
- 用户粘贴/拖放的附件会保存为本地路径，可用 read_file 按路径读取
- 修改代码用 apply_patch（小改）、write_file（新文件/大改）、delete_file 删除
- grep_project 优先 rg；codebase_search 语义搜索代码（需启用）
- web_fetch 抓取文档（需用户确认）；web_search 搜索互联网（需设置 API Key）
- run_command：只读命令自动执行；危险操作需用户确认
- 同轮可连续调用多个工具；每次等待结果后再继续
- 名称以 mcp_ 开头的工具来自已配置的 MCP Server
- spawn_task 可启动子 Agent（explore=只读探索）；子 Agent 结果作为工具输出返回"#
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
