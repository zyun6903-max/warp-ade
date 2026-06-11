use serde_json::{json, Value};

fn openai_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}

fn anthropic_tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "input_schema": input_schema
    })
}

pub fn tool_definitions_openai() -> Vec<Value> {
    vec![
        openai_tool(
            "read_file",
            "读取工作区内文本文件，返回带行号内容",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对工作区的文件路径" },
                    "offset": { "type": "integer", "description": "起始行号（1-based），默认 1" },
                    "limit": { "type": "integer", "description": "最多读取行数" }
                },
                "required": ["path"]
            }),
        ),
        openai_tool(
            "write_file",
            "写入或创建工作区内文件",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对工作区的文件路径" },
                    "content": { "type": "string", "description": "完整文件内容" }
                },
                "required": ["path", "content"]
            }),
        ),
        openai_tool(
            "apply_patch",
            "在工作区文件中替换文本片段",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" },
                    "replace_all": { "type": "boolean", "description": "是否替换所有匹配" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        ),
        openai_tool(
            "delete_file",
            "删除工作区内的文件（不能删目录）",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对工作区的文件路径" }
                },
                "required": ["path"]
            }),
        ),
        openai_tool(
            "glob_files",
            "按 glob 模式查找工作区内文件",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "如 src/**/*.rs" },
                    "path": { "type": "string", "description": "起始目录，默认 ." }
                },
                "required": ["pattern"]
            }),
        ),
        openai_tool(
            "list_directory",
            "列出工作区目录内容",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对路径，默认 ." }
                }
            }),
        ),
        openai_tool(
            "grep_project",
            "在工作区中搜索文本（优先使用 rg）",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "正则或关键词" },
                    "path": { "type": "string", "description": "可选子目录" },
                    "case_insensitive": { "type": "boolean" },
                    "context": { "type": "integer", "description": "上下文行数" },
                    "max_results": { "type": "integer", "description": "最多匹配数，默认 50" }
                },
                "required": ["pattern"]
            }),
        ),
        openai_tool(
            "codebase_search",
            "语义搜索工作区代码（需设置中启用；自动增量建立 embedding 索引）",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "自然语言查询，如「用户认证逻辑在哪」" },
                    "max_results": { "type": "integer", "description": "最多返回条数" }
                },
                "required": ["query"]
            }),
        ),
        openai_tool(
            "web_fetch",
            "抓取 http/https 网页或 API 内容（需用户确认）",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "完整 URL" }
                },
                "required": ["url"]
            }),
        ),
        openai_tool(
            "web_search",
            "搜索互联网获取最新信息（需在设置中启用并配置 API Key）",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词" },
                    "max_results": { "type": "integer", "description": "最多返回条数，默认使用设置值" }
                },
                "required": ["query"]
            }),
        ),
        openai_tool(
            "search_history",
            "搜索本地已导入的聊天记录",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        ),
        openai_tool(
            "use_skill",
            "加载 Skill 完整说明（SKILL.md）。任务匹配某 Skill 描述时必须先调用此工具，再按说明执行",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Skill 名称，与目录中一致" }
                },
                "required": ["name"]
            }),
        ),
        openai_tool(
            "run_command",
            "执行 shell 命令；只读命令可自动执行，安装/写入/网络命令需用户确认",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "shell 命令" }
                },
                "required": ["command"]
            }),
        ),
        openai_tool(
            "spawn_task",
            "启动子 Agent 处理独立子任务（探索代码、并行调研）；子 Agent 不能再次 spawn 或请求用户确认",
            json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "完整、可独立执行的子任务说明" },
                    "subagent_type": {
                        "type": "string",
                        "enum": ["explore", "general"],
                        "description": "explore=只读探索；general=可读写"
                    },
                    "readonly": { "type": "boolean", "description": "true 时禁止写文件与 shell" }
                },
                "required": ["description"]
            }),
        ),
    ]
}

fn tool_name_openai(tool: &Value) -> Option<&str> {
    tool.get("function")
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
}

pub fn plan_mode_excluded_tools() -> &'static [&'static str] {
    &["delete_file", "run_command", "spawn_task"]
}

pub fn plan_mode_openai_tools() -> Vec<Value> {
    tool_definitions_openai_excluding(plan_mode_excluded_tools())
}

pub fn tool_definitions_openai_excluding(exclude: &[&str]) -> Vec<Value> {
    tool_definitions_openai()
        .into_iter()
        .filter(|t| {
            tool_name_openai(t)
                .map(|name| !exclude.contains(&name))
                .unwrap_or(true)
        })
        .collect()
}

pub fn tool_definitions_anthropic_excluding(exclude: &[&str]) -> Vec<Value> {
    tool_definitions_openai_excluding(exclude)
        .into_iter()
        .filter_map(|t| {
            let f = t.get("function")?;
            Some(anthropic_tool(
                f.get("name")?.as_str()?,
                f.get("description")?.as_str().unwrap_or(""),
                f.get("parameters")?.clone(),
            ))
        })
        .collect()
}

pub fn tool_definitions_anthropic() -> Vec<Value> {
    tool_definitions_openai()
        .into_iter()
        .filter_map(|t| {
            let f = t.get("function")?;
            Some(anthropic_tool(
                f.get("name")?.as_str()?,
                f.get("description")?.as_str().unwrap_or(""),
                f.get("parameters")?.clone(),
            ))
        })
        .collect()
}
