use serde_json::{json, Value};

const PREVIEW_TEXT_MAX: usize = 320;

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max).collect::<String>())
}

/// 结构化 preview，供前端渲染 Cursor 风格写入/ diff 卡片
pub fn format_tool_call_preview(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "write_file" => {
            let path = arg_str(args, "path").unwrap_or_default();
            let bytes = args
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::len)
                .unwrap_or(0);
            json!({
                "kind": "write_file",
                "path": path,
                "bytes": bytes,
            })
            .to_string()
        }
        "apply_patch" => format_apply_patch_preview(args),
        _ => serde_json::to_string(args)
            .unwrap_or_default()
            .chars()
            .take(PREVIEW_TEXT_MAX)
            .collect(),
    }
}

pub fn format_apply_patch_preview(args: &Value) -> String {
    let path = arg_str(args, "path").unwrap_or_default();
    let old = args
        .get("old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new = args
        .get("new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    json!({
        "kind": "apply_patch",
        "path": path,
        "old": truncate_chars(old, PREVIEW_TEXT_MAX),
        "new": truncate_chars(new, PREVIEW_TEXT_MAX),
    })
    .to_string()
}

pub fn format_tool_done_preview(tool_name: &str, args: &Value, output: &str) -> String {
    match tool_name {
        "write_file" => output.to_string(),
        "apply_patch" => format_apply_patch_preview(args),
        _ => truncate_chars(output, PREVIEW_TEXT_MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn write_file_preview_json() {
        let preview = format_tool_call_preview(
            "write_file",
            &json!({ "path": "/tmp/a.ts", "content": "hello" }),
        );
        assert!(preview.contains("write_file"));
        assert!(preview.contains("/tmp/a.ts"));
    }
}
