INSERT OR IGNORE INTO app_settings (key, value) VALUES
  ('shell_auto_readonly', 'true'),
  ('shell_always_confirm', 'false'),
  ('shell_extra_allowlist', '');

CREATE TABLE IF NOT EXISTS agent_shell_log (
  id TEXT PRIMARY KEY,
  session_id TEXT,
  command TEXT NOT NULL,
  mode TEXT NOT NULL,
  exit_code INTEGER,
  output_preview TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_shell_log_created ON agent_shell_log(created_at DESC);
