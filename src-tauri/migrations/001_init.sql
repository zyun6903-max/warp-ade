CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  source TEXT NOT NULL,
  source_path TEXT,
  source_session_id TEXT,
  project_slug TEXT,
  workspace_path TEXT,
  continued_from TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  seq INTEGER NOT NULL,
  role TEXT NOT NULL,
  preview TEXT,
  body_compressed BLOB NOT NULL,
  dedup_hash TEXT UNIQUE,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_session_seq ON messages(session_id, seq);
CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);

CREATE TABLE IF NOT EXISTS providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  api_format TEXT NOT NULL,
  models TEXT NOT NULL,
  default_model TEXT NOT NULL,
  priority INTEGER NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  keychain_account TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS context_nodes (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  depth INTEGER NOT NULL DEFAULT 0,
  summary TEXT NOT NULL,
  token_count INTEGER,
  covers_seq_start INTEGER,
  covers_seq_end INTEGER,
  created_at INTEGER NOT NULL
);
