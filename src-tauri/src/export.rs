use crate::error::AppResult;
use crate::storage::db::Database;

pub fn session_to_markdown(db: &Database, session_id: &str) -> AppResult<String> {
    let session = db
        .get_session(session_id)?
        .ok_or_else(|| crate::error::AppError::from("找不到该会话"))?;
    let messages = db.list_messages(session_id)?;
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", session.title));
    if let Some(path) = session.source_path.as_deref() {
        out.push_str(&format!("- 来源文件：`{path}`\n"));
    }
    if session.source != "native" {
        out.push_str(&format!("- 来源：{}\n", session.source));
    }
    if let Some(from) = session.continued_from.as_deref() {
        out.push_str(&format!("- 延续自导入会话：`{from}`\n"));
    }
    out.push('\n');

    for msg in messages {
        let role = match msg.role.as_str() {
            "user" => "用户",
            "assistant" => "助手",
            "tool" => "工具",
            _ => &msg.role,
        };
        let body = msg
            .parts
            .iter()
            .filter_map(|p| p.text.as_deref())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        if body.trim().is_empty() {
            continue;
        }
        out.push_str(&format!("## {role}\n\n{body}\n\n---\n\n"));
    }

    Ok(out)
}
