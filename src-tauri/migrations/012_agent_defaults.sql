-- 默认开启 Agent（接近 Cursor / Claude Code 开箱体验）
UPDATE app_settings SET value = 'true' WHERE key = 'agent_enabled_default';
