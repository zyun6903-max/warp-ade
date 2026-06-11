INSERT OR IGNORE INTO app_settings (key, value) VALUES
  ('semantic_search_enabled', 'false'),
  ('semantic_search_model', 'text-embedding-3-small'),
  ('semantic_search_provider_id', ''),
  ('semantic_search_max_results', '8');

CREATE TABLE IF NOT EXISTS code_index_workspace (
  workspace_path TEXT PRIMARY KEY,
  embedding_model TEXT NOT NULL,
  chunk_count INTEGER NOT NULL DEFAULT 0,
  file_count INTEGER NOT NULL DEFAULT 0,
  last_indexed_at INTEGER
);

CREATE TABLE IF NOT EXISTS code_index_files (
  workspace_path TEXT NOT NULL,
  rel_path TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  mtime_secs INTEGER NOT NULL,
  PRIMARY KEY (workspace_path, rel_path)
);

CREATE TABLE IF NOT EXISTS code_index_chunks (
  id TEXT PRIMARY KEY,
  workspace_path TEXT NOT NULL,
  rel_path TEXT NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  content TEXT NOT NULL,
  embedding BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_code_index_chunks_ws ON code_index_chunks(workspace_path);
