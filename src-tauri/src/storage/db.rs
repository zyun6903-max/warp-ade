use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::storage::compression;

fn run_migrations(conn: &mut Connection) -> AppResult<()> {
    let migrations = Migrations::new(vec![
        M::up(include_str!("../../migrations/001_init.sql")),
        M::up(include_str!("../../migrations/002_projects.sql")),
        M::up(include_str!("../../migrations/003_conversations_project.sql")),
        M::up(include_str!("../../migrations/004_settings.sql")),
        M::up(include_str!("../../migrations/005_agent_shell.sql")),
        M::up(include_str!("../../migrations/006_mcp.sql")),
        M::up(include_str!("../../migrations/007_web_search.sql")),
        M::up(include_str!("../../migrations/008_subagent.sql")),
        M::up(include_str!("../../migrations/009_semantic_index.sql")),
        M::up(include_str!("../../migrations/010_tool_audit.sql")),
        M::up(include_str!("../../migrations/011_workspace_policy.sql")),
    ]);
    migrations.to_latest(conn)?;
    Ok(())
}

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub source: String,
    pub source_path: Option<String>,
    pub project_slug: Option<String>,
    pub workspace_path: Option<String>,
    pub project_id: Option<String>,
    pub continued_from: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub workspace_path: Option<String>,
    pub source_slug: Option<String>,
    pub source_origin: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePart {
    pub part_type: String,
    pub text: Option<String>,
    pub name: Option<String>,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalMessage {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: String,
    pub parts: Vec<MessagePart>,
    pub timestamp: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: String,
    pub parts: Vec<MessagePart>,
    pub preview: String,
    pub created_at: i64,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchHit {
    pub session: Session,
    pub project_name: Option<String>,
    pub project_workspace_path: Option<String>,
    pub matched_preview: String,
    pub matched_seq: i64,
    pub matched_at: i64,
}

fn escape_like_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 8);
    for ch in query.chars() {
        match ch {
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            c => out.push(c),
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_format: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub priority: i64,
    pub enabled: bool,
    pub has_key: bool,
}

pub fn open_db(app_data: &Path) -> AppResult<Database> {
    std::fs::create_dir_all(app_data)?;
    let path = app_data.join("warp-ade.db");
    let mut conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    run_migrations(&mut conn)?;
    let db = Database {
        conn: Mutex::new(conn),
    };
    db.backfill_projects()?;
    db.ensure_default_project()?;
    Ok(db)
}

impl Database {
    pub fn create_session(
        &self,
        title: &str,
        source: &str,
        source_path: Option<&str>,
        project_slug: Option<&str>,
        workspace_path: Option<&str>,
        project_id: Option<&str>,
        continued_from: Option<&str>,
    ) -> AppResult<Session> {
        let project_id = match project_id {
            Some(pid) => Some(pid.to_string()),
            None => Some(self.ensure_default_project()?),
        };
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let now = chrono::Utc::now().timestamp();
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO sessions (id, title, source, source_path, project_slug, workspace_path, project_id, continued_from, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                title,
                source,
                source_path,
                project_slug,
                workspace_path,
                project_id,
                continued_from,
                now,
                now
            ],
        )?;
        if let Some(pid) = project_id.as_deref() {
            conn.execute(
                "UPDATE projects SET updated_at = ?1 WHERE id = ?2",
                params![now, pid],
            )?;
        }
        Ok(Session {
            id,
            title: title.to_string(),
            source: source.to_string(),
            source_path: source_path.map(str::to_string),
            project_slug: project_slug.map(str::to_string),
            workspace_path: workspace_path.map(str::to_string),
            project_id,
            continued_from: continued_from.map(str::to_string),
            created_at: now,
            updated_at: now,
            message_count: 0,
        })
    }

    pub fn ensure_default_project(&self) -> AppResult<String> {
        self.get_or_create_project("对话", None, None, "conversations")
    }

    pub fn delete_session(&self, session_id: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )?;
        conn.execute(
            "DELETE FROM context_nodes WHERE session_id = ?1",
            params![session_id],
        )?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
        Ok(())
    }

    pub fn delete_project(&self, project_id: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let meta: (String, Option<String>) = conn.query_row(
            "SELECT source_origin, workspace_path FROM projects WHERE id = ?1",
            params![project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if meta.0 == "conversations" {
            return Err(AppError::from("「对话」容器不可删除"));
        }
        let mut stmt = conn.prepare("SELECT id FROM sessions WHERE project_id = ?1")?;
        let session_ids: Vec<String> = stmt
            .query_map(params![project_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for sid in &session_ids {
            conn.execute(
                "DELETE FROM messages WHERE session_id = ?1",
                params![sid],
            )?;
            conn.execute(
                "DELETE FROM context_nodes WHERE session_id = ?1",
                params![sid],
            )?;
        }
        conn.execute(
            "DELETE FROM sessions WHERE project_id = ?1",
            params![project_id],
        )?;
        conn.execute("DELETE FROM projects WHERE id = ?1", params![project_id])?;
        Ok(())
    }

    pub fn save_session_to_workspace(
        &self,
        session_id: &str,
        workspace_path: &str,
    ) -> AppResult<Session> {
        let path = workspace_path.trim();
        if path.is_empty() {
            return Err(AppError::from("请填写工作区路径"));
        }
        let session = self
            .get_session(session_id)?
            .ok_or_else(|| AppError::from("会话不存在"))?;
        let project_name = crate::import::projects::project_name_from_path(
            Some(path),
            session.project_slug.as_deref(),
        );
        let project_id = self.get_or_create_project(
            &project_name,
            Some(path),
            session.project_slug.as_deref(),
            &session.source,
        )?;
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE sessions SET workspace_path = ?1, project_id = ?2, updated_at = ?3 WHERE id = ?4",
            params![path, project_id, now, session_id],
        )?;
        drop(conn);
        self.get_session(session_id)?
            .ok_or_else(|| AppError::from("保存后无法读取会话"))
    }

    pub fn save_project_to_workspace(
        &self,
        project_id: &str,
        workspace_path: &str,
    ) -> AppResult<Project> {
        let path = workspace_path.trim();
        if path.is_empty() {
            return Err(AppError::from("请填写工作区路径"));
        }
        let name = crate::import::projects::project_name_from_path(Some(path), None);
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE projects SET name = ?1, workspace_path = ?2, updated_at = ?3 WHERE id = ?4",
            params![name, path, now, project_id],
        )?;
        drop(conn);
        self.list_projects()?
            .into_iter()
            .find(|p| p.id == project_id)
            .ok_or_else(|| AppError::from("项目不存在"))
    }

    pub fn get_or_create_project(
        &self,
        name: &str,
        workspace_path: Option<&str>,
        source_slug: Option<&str>,
        source_origin: &str,
    ) -> AppResult<String> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;

        if source_origin == "conversations" {
            if let Ok(existing) = conn.query_row(
                "SELECT id FROM projects WHERE source_origin = 'conversations' LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            ) {
                return Ok(existing);
            }
        }

        if let Some(path) = workspace_path.filter(|p| !p.is_empty()) {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM projects WHERE workspace_path = ?1 LIMIT 1",
                    params![path],
                    |row| row.get(0),
                )
                .ok();
            if let Some(id) = existing {
                return Ok(id);
            }
        }

        if let Some(slug) = source_slug.filter(|s| !s.is_empty() && *s != "unknown") {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM projects WHERE source_slug = ?1 AND source_origin = ?2 LIMIT 1",
                    params![slug, source_origin],
                    |row| row.get(0),
                )
                .ok();
            if let Some(id) = existing {
                if let Some(decoded) = crate::import::projects::decode_cursor_project_slug(slug) {
                    let name = crate::import::projects::project_name_from_path(
                        Some(&decoded),
                        Some(slug),
                    );
                    let now = chrono::Utc::now().timestamp();
                    conn.execute(
                        "UPDATE projects SET name = ?1, workspace_path = ?2, updated_at = ?3 WHERE id = ?4",
                        params![name, decoded, now, id],
                    )?;
                    conn.execute(
                        "UPDATE sessions SET workspace_path = ?1 WHERE project_id = ?2",
                        params![decoded, id],
                    )?;
                }
                return Ok(id);
            }
        }

        let now = chrono::Utc::now().timestamp();
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO projects (id, name, workspace_path, source_slug, source_origin, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, name, workspace_path, source_slug, source_origin, now, now],
        )?;
        Ok(id)
    }

    pub fn create_project(
        &self,
        name: &str,
        workspace_path: Option<&str>,
    ) -> AppResult<Project> {
        let now = chrono::Utc::now().timestamp();
        let id = Uuid::new_v4().to_string();
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO projects (id, name, workspace_path, source_slug, source_origin, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, 'native', ?4, ?5)",
            params![id, name, workspace_path, now, now],
        )?;
        Ok(Project {
            id,
            name: name.to_string(),
            workspace_path: workspace_path.map(str::to_string),
            source_slug: None,
            source_origin: "native".to_string(),
            created_at: now,
            updated_at: now,
            session_count: 0,
        })
    }

    pub fn list_projects(&self) -> AppResult<Vec<Project>> {
        self.consolidate_conversation_projects()?;
        self.repair_project_metadata()?;
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.workspace_path, p.source_slug, p.source_origin,
                    p.created_at, p.updated_at,
                    (SELECT COUNT(*) FROM sessions s WHERE s.project_id = p.id) AS session_count
             FROM projects p
             ORDER BY p.updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                workspace_path: row.get(2)?,
                source_slug: row.get(3)?,
                source_origin: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                session_count: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    /// 合并重复的「对话」容器（历史版本可能创建了多个 conversations 项目）
    fn consolidate_conversation_projects(&self) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.created_at,
                    (SELECT COUNT(*) FROM sessions s WHERE s.project_id = p.id) AS session_count
             FROM projects p
             WHERE p.source_origin = 'conversations'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut projects: Vec<(String, i64, i64)> = rows.collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        if projects.len() <= 1 {
            return Ok(());
        }

        projects.sort_by(|a, b| {
            b.2
                .cmp(&a.2)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        let canonical_id = projects[0].0.clone();
        for (duplicate_id, _, _) in projects.iter().skip(1) {
            conn.execute(
                "UPDATE sessions SET project_id = ?1 WHERE project_id = ?2",
                params![canonical_id, duplicate_id],
            )?;
            conn.execute("DELETE FROM projects WHERE id = ?1", params![duplicate_id])?;
        }
        Ok(())
    }

    fn repair_project_metadata(&self) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, workspace_path, source_slug, name FROM projects WHERE source_slug IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let projects: Vec<_> = rows.collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for (id, workspace_path, source_slug, name) in projects {
            let Some(decoded) = crate::import::projects::decode_cursor_project_slug(&source_slug)
            else {
                continue;
            };
            let resolved_name = crate::import::projects::project_name_from_path(
                Some(&decoded),
                Some(&source_slug),
            );
            let needs_update = workspace_path.as_deref() != Some(decoded.as_str()) || name != resolved_name;
            if !needs_update {
                continue;
            }
            let now = chrono::Utc::now().timestamp();
            conn.execute(
                "UPDATE projects SET name = ?1, workspace_path = ?2, updated_at = ?3 WHERE id = ?4",
                params![resolved_name, decoded, now, id],
            )?;
            conn.execute(
                "UPDATE sessions SET workspace_path = ?1 WHERE project_id = ?2",
                params![decoded, id],
            )?;
        }
        Ok(())
    }

    fn backfill_projects(&self) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))?;
        if count > 0 {
            return Ok(());
        }

        let mut stmt = conn.prepare(
            "SELECT id, source, project_slug, workspace_path FROM sessions WHERE project_id IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let sessions: Vec<_> = rows.collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        for (session_id, source, project_slug, workspace_path) in sessions {
            let name = crate::import::projects::project_name_from_path(
                workspace_path.as_deref(),
                project_slug.as_deref(),
            );
            let project_id = self.get_or_create_project(
                &name,
                workspace_path.as_deref(),
                project_slug.as_deref(),
                &source,
            )?;
            let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
            conn.execute(
                "UPDATE sessions SET project_id = ?1 WHERE id = ?2",
                params![project_id, session_id],
            )?;
        }
        Ok(())
    }

    pub fn list_sessions(&self, project_id: Option<&str>) -> AppResult<Vec<Session>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut sql = String::from(
            "SELECT s.id, s.title, s.source, s.source_path, s.project_slug, s.workspace_path, s.project_id,
                    s.continued_from, s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id) AS message_count
             FROM sessions s",
        );
        if project_id.is_some() {
            sql.push_str(" WHERE s.project_id = ?1");
        }
        sql.push_str(" ORDER BY s.updated_at DESC");

        let mut stmt = conn.prepare(&sql)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                source: row.get(2)?,
                source_path: row.get(3)?,
                project_slug: row.get(4)?,
                workspace_path: row.get(5)?,
                project_id: row.get(6)?,
                continued_from: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                message_count: row.get(10)?,
            })
        };

        if let Some(pid) = project_id {
            let rows = stmt.query_map(params![pid], map_row)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
        } else {
            let rows = stmt.query_map([], map_row)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
        }
    }

    pub fn get_session(&self, session_id: &str) -> AppResult<Option<Session>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.source, s.source_path, s.project_slug, s.workspace_path, s.project_id,
                    s.continued_from, s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id) AS message_count
             FROM sessions s WHERE s.id = ?1",
        )?;
        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                source: row.get(2)?,
                source_path: row.get(3)?,
                project_slug: row.get(4)?,
                workspace_path: row.get(5)?,
                project_id: row.get(6)?,
                continued_from: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                message_count: row.get(10)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert_message(&self, message: &CanonicalMessage) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let preview = message
            .parts
            .iter()
            .filter_map(|p| p.text.as_deref())
            .collect::<Vec<_>>()
            .join(" ");
        let preview = preview.chars().take(500).collect::<String>();
        let body = serde_json::to_vec(message)?;
        let compressed = compression::compress(&body)?;
        let dedup_source = format!(
            "{}:{}:{}",
            message.session_id, message.seq, preview
        );
        let dedup_hash = hex::encode(Sha256::digest(dedup_source.as_bytes()));
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT OR IGNORE INTO messages
             (id, session_id, seq, role, preview, body_compressed, dedup_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                message.id,
                message.session_id,
                message.seq,
                message.role,
                preview,
                compressed,
                dedup_hash,
                now
            ],
        )?;

        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, message.session_id],
        )?;
        if let Ok(Some(pid)) = conn.query_row(
            "SELECT project_id FROM sessions WHERE id = ?1",
            params![message.session_id],
            |row| row.get::<_, Option<String>>(0),
        ) {
            conn.execute(
                "UPDATE projects SET updated_at = ?1 WHERE id = ?2",
                params![now, pid],
            )?;
        }
        Ok(())
    }

    pub fn list_messages(&self, session_id: &str) -> AppResult<Vec<MessageView>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, seq, role, preview, body_compressed, created_at
             FROM messages WHERE session_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            let compressed: Vec<u8> = row.get(5)?;
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, compressed, row.get(6)?))
        })?;

        let mut messages = Vec::new();
        for row in rows {
            let (id, session_id, seq, role, preview, compressed, created_at) = row?;
            let body = compression::decompress(&compressed)?;
            let canonical: CanonicalMessage = serde_json::from_slice(&body)?;
            messages.push(MessageView {
                id,
                session_id,
                seq,
                role,
                parts: canonical.parts,
                preview,
                created_at,
                metadata: Some(canonical.metadata),
            });
        }
        Ok(messages)
    }

    pub fn search_sessions(
        &self,
        query: &str,
        limit: Option<i64>,
        project_id: Option<&str>,
        source: Option<&str>,
    ) -> AppResult<Vec<SessionSearchHit>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let limit = limit.unwrap_or(50).clamp(1, 200);
        let pattern = format!("%{}%", escape_like_pattern(trimmed));
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;

        let sql = if project_id.is_some() && source.is_some() {
            "SELECT s.id, s.title, s.source, s.source_path, s.project_slug, s.workspace_path, s.project_id,
                    s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM messages m2 WHERE m2.session_id = s.id) AS message_count,
                    m.preview, m.seq, m.created_at,
                    p.name, p.workspace_path
             FROM messages m
             INNER JOIN sessions s ON s.id = m.session_id
             LEFT JOIN projects p ON p.id = s.project_id
             WHERE m.role = 'user'
               AND m.preview LIKE ?1 ESCAPE '\\'
               AND s.project_id = ?2
               AND s.source = ?3
             ORDER BY m.created_at DESC
             LIMIT ?4"
        } else if project_id.is_some() {
            "SELECT s.id, s.title, s.source, s.source_path, s.project_slug, s.workspace_path, s.project_id,
                    s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM messages m2 WHERE m2.session_id = s.id) AS message_count,
                    m.preview, m.seq, m.created_at,
                    p.name, p.workspace_path
             FROM messages m
             INNER JOIN sessions s ON s.id = m.session_id
             LEFT JOIN projects p ON p.id = s.project_id
             WHERE m.role = 'user'
               AND m.preview LIKE ?1 ESCAPE '\\'
               AND s.project_id = ?2
             ORDER BY m.created_at DESC
             LIMIT ?3"
        } else if source.is_some() {
            "SELECT s.id, s.title, s.source, s.source_path, s.project_slug, s.workspace_path, s.project_id,
                    s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM messages m2 WHERE m2.session_id = s.id) AS message_count,
                    m.preview, m.seq, m.created_at,
                    p.name, p.workspace_path
             FROM messages m
             INNER JOIN sessions s ON s.id = m.session_id
             LEFT JOIN projects p ON p.id = s.project_id
             WHERE m.role = 'user'
               AND m.preview LIKE ?1 ESCAPE '\\'
               AND s.source = ?2
             ORDER BY m.created_at DESC
             LIMIT ?3"
        } else {
            "SELECT s.id, s.title, s.source, s.source_path, s.project_slug, s.workspace_path, s.project_id,
                    s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM messages m2 WHERE m2.session_id = s.id) AS message_count,
                    m.preview, m.seq, m.created_at,
                    p.name, p.workspace_path
             FROM messages m
             INNER JOIN sessions s ON s.id = m.session_id
             LEFT JOIN projects p ON p.id = s.project_id
             WHERE m.role = 'user'
               AND m.preview LIKE ?1 ESCAPE '\\'
             ORDER BY m.created_at DESC
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            Ok((
                Session {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    source: row.get(2)?,
                    source_path: row.get(3)?,
                    project_slug: row.get(4)?,
                    workspace_path: row.get(5)?,
                    project_id: row.get(6)?,
                    continued_from: None,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                    message_count: row.get(9)?,
                },
                row.get::<_, String>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
            ))
        };

        let rows: Vec<_> = match (project_id, source) {
            (Some(pid), Some(src)) => stmt
                .query_map(params![pattern, pid, src, limit], map_row)?
                .collect::<Result<Vec<_>, _>>()?,
            (Some(pid), None) => stmt
                .query_map(params![pattern, pid, limit], map_row)?
                .collect::<Result<Vec<_>, _>>()?,
            (None, Some(src)) => stmt
                .query_map(params![pattern, src, limit], map_row)?
                .collect::<Result<Vec<_>, _>>()?,
            (None, None) => stmt
                .query_map(params![pattern, limit], map_row)?
                .collect::<Result<Vec<_>, _>>()?,
        };

        let mut seen = std::collections::HashSet::new();
        let mut hits = Vec::new();
        for (session, preview, seq, matched_at, project_name, project_workspace_path) in rows {
            if !seen.insert(session.id.clone()) {
                continue;
            }
            hits.push(SessionSearchHit {
                session,
                project_name,
                project_workspace_path,
                matched_preview: preview,
                matched_seq: seq,
                matched_at,
            });
        }
        Ok(hits)
    }

    pub fn upsert_provider(&self, provider: &Provider) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let models_json = serde_json::to_string(&provider.models)?;
        conn.execute(
            "INSERT INTO providers (id, name, base_url, api_format, models, default_model, priority, enabled, keychain_account)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               base_url = excluded.base_url,
               api_format = excluded.api_format,
               models = excluded.models,
               default_model = excluded.default_model,
               priority = excluded.priority,
               enabled = excluded.enabled,
               keychain_account = excluded.keychain_account",
            params![
                provider.id,
                provider.name,
                provider.base_url,
                provider.api_format,
                models_json,
                provider.default_model,
                provider.priority,
                provider.enabled,
                provider.id,
            ],
        )?;
        Ok(())
    }

    pub fn list_providers(&self) -> AppResult<Vec<Provider>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, api_format, models, default_model, priority, enabled, keychain_account
             FROM providers ORDER BY priority ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let models_json: String = row.get(4)?;
            let keychain_account: String = row.get(8)?;
            let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                models,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)? != 0,
                keychain_account,
            ))
        })?;

        let mut providers = Vec::new();
        for row in rows {
            let (id, name, base_url, api_format, models, default_model, priority, enabled, keychain_account) =
                row?;
            let secret_key = if keychain_account.is_empty() {
                id.as_str()
            } else {
                keychain_account.as_str()
            };
            let has_key = crate::secrets::has_api_key(secret_key).unwrap_or(false);
            providers.push(Provider {
                id,
                name,
                base_url,
                api_format,
                models,
                default_model,
                priority,
                enabled,
                has_key,
            });
        }
        Ok(providers)
    }

    pub fn next_provider_priority(&self) -> AppResult<i64> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let max: i64 = conn
            .query_row("SELECT COALESCE(MAX(priority), 0) FROM providers", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        Ok(max + 1)
    }

    pub fn reorder_providers(&self, ids: &[String]) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let tx = conn.unchecked_transaction()?;
        for (index, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE providers SET priority = ?1 WHERE id = ?2",
                params![(index + 1) as i64, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_provider(&self, provider_id: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute("DELETE FROM providers WHERE id = ?1", params![provider_id])?;
        let _ = crate::secrets::delete_api_key(provider_id);
        Ok(())
    }

    pub fn get_enabled_providers(&self) -> AppResult<Vec<Provider>> {
        Ok(self
            .list_providers()?
            .into_iter()
            .filter(|p| p.enabled && p.has_key)
            .collect())
    }

    pub fn session_exists_by_source(&self, source_path: &str) -> AppResult<bool> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE source_path = ?1",
            params![source_path],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn imported_source_paths(&self) -> AppResult<std::collections::HashSet<String>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt =
            conn.prepare("SELECT source_path FROM sessions WHERE source_path IS NOT NULL")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn get_session_by_source_path(&self, source_path: &str) -> AppResult<Option<Session>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.source, s.source_path, s.project_slug, s.workspace_path, s.project_id,
                    s.continued_from, s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id) AS message_count
             FROM sessions s WHERE s.source_path = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![source_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                source: row.get(2)?,
                source_path: row.get(3)?,
                project_slug: row.get(4)?,
                workspace_path: row.get(5)?,
                project_id: row.get(6)?,
                continued_from: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                message_count: row.get(10)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn update_session_title(&self, session_id: &str, title: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, chrono::Utc::now().timestamp(), session_id],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn resolve_session_workspace(&self, session_id: &str) -> AppResult<Option<String>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let path: Option<String> = conn.query_row(
            "SELECT COALESCE(NULLIF(TRIM(s.workspace_path), ''), NULLIF(TRIM(p.workspace_path), ''))
             FROM sessions s
             LEFT JOIN projects p ON p.id = s.project_id
             WHERE s.id = ?1",
            params![session_id],
            |row| row.get(0),
        ).optional()?;
        Ok(path.filter(|p| !p.trim().is_empty()))
    }

    pub fn get_latest_context_node(&self, session_id: &str) -> AppResult<Option<ContextNode>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, depth, summary, token_count, covers_seq_start, covers_seq_end, created_at
             FROM context_nodes WHERE session_id = ?1
             ORDER BY covers_seq_end DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ContextNode {
                id: row.get(0)?,
                session_id: row.get(1)?,
                depth: row.get(2)?,
                summary: row.get(3)?,
                token_count: row.get(4)?,
                covers_seq_start: row.get(5)?,
                covers_seq_end: row.get(6)?,
                created_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert_context_node(
        &self,
        session_id: &str,
        summary: &str,
        token_count: i64,
        covers_seq_start: i64,
        covers_seq_end: i64,
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO context_nodes
             (id, session_id, depth, summary, token_count, covers_seq_start, covers_seq_end, created_at)
             VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                session_id,
                summary,
                token_count,
                covers_seq_start,
                covers_seq_end,
                chrono::Utc::now().timestamp(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_shell_log(
        &self,
        session_id: Option<&str>,
        command: &str,
        mode: &str,
        exit_code: Option<i32>,
        output_preview: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO agent_shell_log (id, session_id, command, mode, exit_code, output_preview, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                session_id,
                command,
                mode,
                exit_code,
                output_preview.map(|p| p.chars().take(500).collect::<String>()),
                chrono::Utc::now().timestamp(),
            ],
        )?;
        Ok(())
    }

    pub fn list_shell_logs(&self, limit: usize) -> AppResult<Vec<ShellLogEntry>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, command, mode, exit_code, output_preview, created_at
             FROM agent_shell_log
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ShellLogEntry {
                id: row.get(0)?,
                session_id: row.get(1)?,
                command: row.get(2)?,
                mode: row.get(3)?,
                exit_code: row.get(4)?,
                output_preview: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::from(e.to_string()))
    }

    pub fn insert_tool_audit_log(
        &self,
        session_id: Option<&str>,
        tool_name: &str,
        mode: &str,
        input_preview: Option<&str>,
        output_preview: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO agent_tool_audit_log (id, session_id, tool_name, mode, input_preview, output_preview, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                session_id,
                tool_name,
                mode,
                input_preview.map(|p| p.chars().take(500).collect::<String>()),
                output_preview.map(|p| p.chars().take(500).collect::<String>()),
                chrono::Utc::now().timestamp(),
            ],
        )?;
        Ok(())
    }

    pub fn list_tool_audit_logs(&self, limit: usize) -> AppResult<Vec<ToolAuditEntry>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, tool_name, mode, input_preview, output_preview, created_at
             FROM agent_tool_audit_log
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ToolAuditEntry {
                id: row.get(0)?,
                session_id: row.get(1)?,
                tool_name: row.get(2)?,
                mode: row.get(3)?,
                input_preview: row.get(4)?,
                output_preview: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::from(e.to_string()))
    }

    pub fn list_mcp_servers(&self) -> AppResult<Vec<McpServerRow>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT id, name, command, args, env, enabled, created_at, updated_at
             FROM mcp_servers ORDER BY name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(McpServerRow {
                id: row.get(0)?,
                name: row.get(1)?,
                command: row.get(2)?,
                args_json: row.get(3)?,
                env_json: row.get(4)?,
                enabled: row.get::<_, i64>(5)? != 0,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::from(e.to_string()))
    }

    pub fn upsert_mcp_server(&self, server: &McpServerRow) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO mcp_servers (id, name, command, args, env, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               command = excluded.command,
               args = excluded.args,
               env = excluded.env,
               enabled = excluded.enabled,
               updated_at = excluded.updated_at",
            params![
                server.id,
                server.name,
                server.command,
                server.args_json,
                server.env_json,
                if server.enabled { 1 } else { 0 },
                server.created_at,
                server.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_mcp_server(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_code_index_files(&self, workspace_path: &str) -> AppResult<Vec<CodeIndexFileRow>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT rel_path, content_hash, mtime_secs FROM code_index_files WHERE workspace_path = ?1",
        )?;
        let rows = stmt.query_map(params![workspace_path], |row| {
            Ok(CodeIndexFileRow {
                rel_path: row.get(0)?,
                content_hash: row.get(1)?,
                mtime_secs: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_code_index_chunks(&self, workspace_path: &str) -> AppResult<Vec<CodeIndexChunkRow>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT rel_path, start_line, end_line, content, embedding FROM code_index_chunks WHERE workspace_path = ?1",
        )?;
        let rows = stmt.query_map(params![workspace_path], |row| {
            Ok(CodeIndexChunkRow {
                rel_path: row.get(0)?,
                start_line: row.get(1)?,
                end_line: row.get(2)?,
                content: row.get(3)?,
                embedding: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_code_index_file(&self, workspace_path: &str, rel_path: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "DELETE FROM code_index_chunks WHERE workspace_path = ?1 AND rel_path = ?2",
            params![workspace_path, rel_path],
        )?;
        conn.execute(
            "DELETE FROM code_index_files WHERE workspace_path = ?1 AND rel_path = ?2",
            params![workspace_path, rel_path],
        )?;
        Ok(())
    }

    pub fn clear_code_index(&self, workspace_path: &str) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "DELETE FROM code_index_chunks WHERE workspace_path = ?1",
            params![workspace_path],
        )?;
        conn.execute(
            "DELETE FROM code_index_files WHERE workspace_path = ?1",
            params![workspace_path],
        )?;
        conn.execute(
            "DELETE FROM code_index_workspace WHERE workspace_path = ?1",
            params![workspace_path],
        )?;
        Ok(())
    }

    pub fn insert_code_index_chunk(
        &self,
        workspace_path: &str,
        rel_path: &str,
        start_line: i64,
        end_line: i64,
        content: &str,
        embedding: &[u8],
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO code_index_chunks (id, workspace_path, rel_path, start_line, end_line, content, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                workspace_path,
                rel_path,
                start_line,
                end_line,
                content,
                embedding,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_code_index_file(
        &self,
        workspace_path: &str,
        rel_path: &str,
        content_hash: &str,
        mtime_secs: i64,
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        conn.execute(
            "INSERT INTO code_index_files (workspace_path, rel_path, content_hash, mtime_secs)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workspace_path, rel_path) DO UPDATE SET
               content_hash = excluded.content_hash,
               mtime_secs = excluded.mtime_secs",
            params![workspace_path, rel_path, content_hash, mtime_secs],
        )?;
        Ok(())
    }

    pub fn upsert_code_index_workspace(
        &self,
        workspace_path: &str,
        embedding_model: &str,
        chunk_count: usize,
        file_count: usize,
    ) -> AppResult<()> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO code_index_workspace (workspace_path, embedding_model, chunk_count, file_count, last_indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(workspace_path) DO UPDATE SET
               embedding_model = excluded.embedding_model,
               chunk_count = excluded.chunk_count,
               file_count = excluded.file_count,
               last_indexed_at = excluded.last_indexed_at",
            params![
                workspace_path,
                embedding_model,
                chunk_count as i64,
                file_count as i64,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn get_code_index_workspace(
        &self,
        workspace_path: &str,
    ) -> AppResult<Option<CodeIndexWorkspaceRow>> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let mut stmt = conn.prepare(
            "SELECT chunk_count, file_count, last_indexed_at FROM code_index_workspace WHERE workspace_path = ?1",
        )?;
        let mut rows = stmt.query(params![workspace_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CodeIndexWorkspaceRow {
                chunk_count: row.get(0)?,
                file_count: row.get(1)?,
                last_indexed_at: row.get(2)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn count_code_index_chunks(&self, workspace_path: &str) -> AppResult<usize> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM code_index_chunks WHERE workspace_path = ?1",
            params![workspace_path],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_code_index_files(&self, workspace_path: &str) -> AppResult<usize> {
        let conn = self.conn.lock().map_err(|_| "database lock poisoned")?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM code_index_files WHERE workspace_path = ?1",
            params![workspace_path],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextNode {
    pub id: String,
    pub session_id: String,
    pub depth: i64,
    pub summary: String,
    pub token_count: Option<i64>,
    pub covers_seq_start: i64,
    pub covers_seq_end: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellLogEntry {
    pub id: String,
    pub session_id: Option<String>,
    pub command: String,
    pub mode: String,
    pub exit_code: Option<i32>,
    pub output_preview: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct McpServerRow {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args_json: String,
    pub env_json: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAuditEntry {
    pub id: String,
    pub session_id: Option<String>,
    pub tool_name: String,
    pub mode: String,
    pub input_preview: Option<String>,
    pub output_preview: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct CodeIndexFileRow {
    pub rel_path: String,
    pub content_hash: String,
    pub mtime_secs: i64,
}

#[derive(Debug, Clone)]
pub struct CodeIndexChunkRow {
    pub rel_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
    pub embedding: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct CodeIndexWorkspaceRow {
    pub chunk_count: i64,
    pub file_count: i64,
    pub last_indexed_at: Option<i64>,
}

pub fn app_data_dir() -> AppResult<PathBuf> {
    if let Ok(dir) = std::env::var("WARP_ADE_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    dirs::data_dir()
        .map(|p| p.join("com.warpade.app"))
        .ok_or_else(|| AppError::from("无法定位应用数据目录"))
}
