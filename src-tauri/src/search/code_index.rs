use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use reqwest::Client;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::agent::context::AppContextSettings;
use crate::error::{AppError, AppResult};
use crate::secrets;
use crate::storage::db::{Database, Provider};

use super::embeddings::{
    cosine_similarity, embed_texts, embedding_from_bytes, embedding_to_bytes,
};

pub const CHUNK_LINES: usize = 60;
pub const CHUNK_OVERLAP: usize = 10;
pub const MAX_FILE_BYTES: usize = 256 * 1024;
pub const MAX_EMBED_BATCH: usize = 16;
pub const MAX_INDEX_FILES_PER_CALL: usize = 40;

#[derive(Debug, Clone)]
pub struct SemanticSearchConfig {
    pub enabled: bool,
    pub model: String,
    pub provider_id: Option<String>,
    pub max_results: usize,
}

impl SemanticSearchConfig {
    pub fn from_settings(settings: &AppContextSettings) -> Self {
        Self {
            enabled: settings.semantic_search_enabled,
            model: settings.semantic_search_model.clone(),
            provider_id: if settings.semantic_search_provider_id.trim().is_empty() {
                None
            } else {
                Some(settings.semantic_search_provider_id.clone())
            },
            max_results: settings.semantic_search_max_results.max(1).min(20),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeIndexStatus {
    pub enabled: bool,
    pub workspace_path: Option<String>,
    pub chunk_count: usize,
    pub file_count: usize,
    pub last_indexed_at: Option<i64>,
    pub model: String,
}

#[derive(Debug, Clone)]
struct FileChunk {
    start_line: usize,
    end_line: usize,
    content: String,
}

#[derive(Debug, Clone)]
struct ScoredChunk {
    rel_path: String,
    start_line: usize,
    end_line: usize,
    content: String,
    score: f32,
}

pub fn search_blocking(
    http: &Client,
    db: &Database,
    workspace: &Path,
    config: &SemanticSearchConfig,
    query: &str,
) -> AppResult<String> {
    if !config.enabled {
        return Err(AppError::from("语义代码搜索未启用，请在设置中开启"));
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AppError::from(format!("无法启动搜索运行时: {e}")))?;
    rt.block_on(search_async(
        http,
        db,
        workspace,
        config,
        query,
        config.max_results,
    ))
}

pub async fn search_async(
    http: &Client,
    db: &Database,
    workspace: &Path,
    config: &SemanticSearchConfig,
    query: &str,
    max_results: usize,
) -> AppResult<String> {
    if !config.enabled {
        return Err(AppError::from("语义代码搜索未启用，请在设置中开启"));
    }
    let (provider, api_key) = resolve_embedding_provider(db, config.provider_id.as_deref())?;
    ensure_index(
        http,
        db,
        workspace,
        config,
        &provider,
        &api_key,
        MAX_INDEX_FILES_PER_CALL,
    )
    .await?;

    let query_vec = embed_texts(
        http,
        &provider,
        &api_key,
        &config.model,
        &[query.to_string()],
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| AppError::from("无法生成查询向量"))?;

    let ws = workspace.to_string_lossy().to_string();
    let chunks = db.list_code_index_chunks(&ws)?;
    let mut scored: Vec<ScoredChunk> = chunks
        .into_iter()
        .filter_map(|row| {
            let embedding = embedding_from_bytes(&row.embedding).ok()?;
            let score = cosine_similarity(&query_vec, &embedding);
            Some(ScoredChunk {
                rel_path: row.rel_path,
                start_line: row.start_line as usize,
                end_line: row.end_line as usize,
                content: row.content,
                score,
            })
        })
        .collect();

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(max_results.max(1).min(20));

    Ok(format_hits(&scored))
}

pub async fn rebuild_index_async(
    http: &Client,
    db: &Database,
    workspace: &Path,
    config: &SemanticSearchConfig,
) -> AppResult<CodeIndexStatus> {
    if !config.enabled {
        return Err(AppError::from("语义代码搜索未启用"));
    }
    let (provider, api_key) = resolve_embedding_provider(db, config.provider_id.as_deref())?;
    let ws = workspace.to_string_lossy().to_string();
    db.clear_code_index(&ws)?;
    loop {
        let updated = ensure_index(
            http,
            db,
            workspace,
            config,
            &provider,
            &api_key,
            MAX_INDEX_FILES_PER_CALL,
        )
        .await?;
        if updated == 0 {
            break;
        }
    }
    index_status(db, config, Some(ws))
}

fn format_hits(hits: &[ScoredChunk]) -> String {
    if hits.is_empty() {
        return "未找到语义相关代码".into();
    }
    hits.iter()
        .enumerate()
        .map(|(i, h)| {
            let preview = h
                .content
                .lines()
                .take(6)
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{}. {}:{}-{} (score {:.3})\n{}",
                i + 1,
                h.rel_path,
                h.start_line,
                h.end_line,
                h.score,
                preview
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

async fn ensure_index(
    http: &Client,
    db: &Database,
    workspace: &Path,
    config: &SemanticSearchConfig,
    provider: &Provider,
    api_key: &str,
    max_files: usize,
) -> AppResult<usize> {
    let ws = workspace.to_string_lossy().to_string();
    let disk_files = collect_indexable_files(workspace)?;
    let indexed = db.list_code_index_files(&ws)?;
    let indexed_map: HashMap<String, (String, i64)> = indexed
        .into_iter()
        .map(|f| (f.rel_path, (f.content_hash, f.mtime_secs)))
        .collect();

    let mut to_update: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();

    for rel in &disk_files {
        seen.insert(rel.clone());
        let full = workspace.join(rel);
        let meta = std::fs::metadata(&full)?;
        let mtime = file_mtime_secs(&meta);
        let hash = hash_file(&full)?;
        match indexed_map.get(rel) {
            Some((old_hash, old_mtime)) if old_hash == &hash && *old_mtime == mtime => {}
            _ => to_update.push(full),
        }
        if to_update.len() >= max_files {
            break;
        }
    }

    for rel in indexed_map.keys() {
        if !seen.contains(rel) {
            db.delete_code_index_file(&ws, rel)?;
        }
    }

    let mut updated_files = 0usize;
    for full in to_update {
        let rel = full
            .strip_prefix(workspace)
            .unwrap_or(&full)
            .display()
            .to_string();
        index_one_file(http, db, &ws, &full, &rel, config, provider, api_key)
            .await?;
        updated_files += 1;
    }

    let chunk_count = db.count_code_index_chunks(&ws)?;
    let file_count = db.count_code_index_files(&ws)?;
    db.upsert_code_index_workspace(&ws, &config.model, chunk_count, file_count)?;
    Ok(updated_files)
}

async fn index_one_file(
    http: &Client,
    db: &Database,
    ws: &str,
    full: &Path,
    rel: &str,
    config: &SemanticSearchConfig,
    provider: &Provider,
    api_key: &str,
) -> AppResult<()> {
    db.delete_code_index_file(ws, rel)?;
    let content = std::fs::read_to_string(full)?;
    let chunks = chunk_file_content(&content);
    if chunks.is_empty() {
        return Ok(());
    }

    let meta = std::fs::metadata(full)?;
    let mtime = file_mtime_secs(&meta);
    let hash = hash_bytes(content.as_bytes());

    for batch in chunks.chunks(MAX_EMBED_BATCH) {
        let texts: Vec<String> = batch
            .iter()
            .map(|c| format_chunk_for_embedding(rel, c))
            .collect();
        let vectors = embed_texts(http, provider, api_key, &config.model, &texts).await?;
        for (chunk, vector) in batch.iter().zip(vectors.iter()) {
            db.insert_code_index_chunk(
                ws,
                rel,
                chunk.start_line as i64,
                chunk.end_line as i64,
                &chunk.content,
                &embedding_to_bytes(vector),
            )?;
        }
    }

    db.upsert_code_index_file(ws, rel, &hash, mtime)?;
    Ok(())
}

fn format_chunk_for_embedding(rel: &str, chunk: &FileChunk) -> String {
    format!(
        "File: {rel}\nLines {}-{}\n{}",
        chunk.start_line, chunk.end_line, chunk.content
    )
}

pub fn chunk_file_content(content: &str) -> Vec<FileChunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let end = (start + CHUNK_LINES).min(lines.len());
        let body = lines[start..end].join("\n");
        if body.trim().len() >= 20 {
            chunks.push(FileChunk {
                start_line: start + 1,
                end_line: end,
                content: body,
            });
        }
        if end >= lines.len() {
            break;
        }
        let next = end.saturating_sub(CHUNK_OVERLAP);
        if next <= start {
            break;
        }
        start = next;
    }
    chunks
}

fn collect_indexable_files(workspace: &Path) -> AppResult<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(workspace)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if should_skip_path(workspace, path) {
            continue;
        }
        if path.metadata().map(|m| m.len() as usize).unwrap_or(0) > MAX_FILE_BYTES {
            continue;
        }
        let rel = path
            .strip_prefix(workspace)
            .unwrap_or(path)
            .display()
            .to_string();
        files.push(rel);
        if files.len() > 8000 {
            break;
        }
    }
    files.sort();
    Ok(files)
}

fn should_skip_path(workspace: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(workspace).unwrap_or(path);
    for comp in rel.components() {
        let name = comp.as_os_str().to_string_lossy();
        if matches!(
            name.as_ref(),
            ".git"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
                | ".next"
                | ".turbo"
                | "vendor"
                | "__pycache__"
                | ".pnpm-store"
        ) {
            return true;
        }
    }
    if path.extension().is_some_and(|ext| {
        matches!(
            ext.to_str(),
            Some(
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "woff" | "woff2" | "pdf"
                    | "zip" | "gz" | "tar" | "mp4" | "mp3" | "wasm" | "lock"
            )
        )
    }) {
        return true;
    }
    false
}

fn hash_file(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn file_mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn resolve_embedding_provider(
    db: &Database,
    provider_id: Option<&str>,
) -> AppResult<(Provider, String)> {
    let providers = db.get_enabled_providers()?;
    if let Some(id) = provider_id.filter(|s| !s.is_empty()) {
        let provider = providers
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::from("指定的 Embedding 提供商不可用"))?;
        if provider.api_format == "anthropic_messages" {
            return Err(AppError::from("请选择 OpenAI 兼容提供商用于 Embedding"));
        }
        let key = secrets::get_api_key(&provider.id)?;
        return Ok((provider, key));
    }

    for provider in providers {
        if provider.api_format == "anthropic_messages" {
            continue;
        }
        if secrets::has_api_key(&provider.id)? {
            let key = secrets::get_api_key(&provider.id)?;
            return Ok((provider, key));
        }
    }
    Err(AppError::from(
        "未找到可用于 Embedding 的 OpenAI 兼容提供商，请先配置 API Key",
    ))
}

pub fn index_status(
    db: &Database,
    config: &SemanticSearchConfig,
    workspace_path: Option<String>,
) -> AppResult<CodeIndexStatus> {
    let (chunk_count, file_count, last_indexed_at) = if let Some(ref ws) = workspace_path {
        let row = db.get_code_index_workspace(ws)?;
        (
            row.as_ref().map(|r| r.chunk_count as usize).unwrap_or(0),
            row.as_ref().map(|r| r.file_count as usize).unwrap_or(0),
            row.and_then(|r| r.last_indexed_at),
        )
    } else {
        (0, 0, None)
    };
    Ok(CodeIndexStatus {
        enabled: config.enabled,
        workspace_path,
        chunk_count,
        file_count,
        last_indexed_at,
        model: config.model.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_large_file_with_overlap() {
        let content = (1..=150)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_file_content(&content);
        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].start_line, 1);
    }
}
