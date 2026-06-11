use serde_json::Value;

use crate::storage::db::Database;

const PREVIEW_INPUT_MAX: usize = 400;
const PREVIEW_OUTPUT_MAX: usize = 500;

pub fn preview_args(args: &Value) -> String {
    serde_json::to_string(args)
        .unwrap_or_default()
        .chars()
        .take(PREVIEW_INPUT_MAX)
        .collect()
}

pub fn preview_text(text: &str) -> String {
    text.chars().take(PREVIEW_OUTPUT_MAX).collect()
}

pub fn log_tool_audit(
    db: &Database,
    session_id: &str,
    tool_name: &str,
    mode: &str,
    input: &str,
    output: Option<&str>,
) {
    let _ = db.insert_tool_audit_log(
        Some(session_id),
        tool_name,
        mode,
        Some(&preview_text(input)),
        output.map(preview_text).as_deref(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn truncates_long_input_preview() {
        let long = "x".repeat(500);
        let args = json!({ "content": long });
        assert!(preview_args(&args).chars().count() <= PREVIEW_INPUT_MAX);
    }
}
