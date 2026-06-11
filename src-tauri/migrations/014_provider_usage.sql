CREATE TABLE IF NOT EXISTS provider_usage (
  provider_id TEXT NOT NULL,
  model TEXT NOT NULL,
  request_count INTEGER NOT NULL DEFAULT 0,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  test_count INTEGER NOT NULL DEFAULT 0,
  last_used_at INTEGER,
  PRIMARY KEY (provider_id, model)
);

CREATE INDEX IF NOT EXISTS idx_provider_usage_provider ON provider_usage(provider_id);
