CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

INSERT OR IGNORE INTO app_settings (key, value) VALUES
  ('context_recent_turns', '12'),
  ('context_token_threshold', '60000'),
  ('context_summary_enabled', 'true'),
  ('agent_max_iterations', '25'),
  ('agent_enabled_default', 'false');
