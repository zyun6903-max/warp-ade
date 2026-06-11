CREATE TABLE IF NOT EXISTS agent_tool_audit_log (
  id TEXT PRIMARY KEY,
  session_id TEXT,
  tool_name TEXT NOT NULL,
  mode TEXT NOT NULL,
  input_preview TEXT,
  output_preview TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_tool_audit_created ON agent_tool_audit_log(created_at DESC);
