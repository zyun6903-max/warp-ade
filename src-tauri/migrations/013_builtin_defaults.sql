-- 默认开启 Agent 内置搜索能力（无需设置页配置）
UPDATE app_settings SET value = 'true' WHERE key = 'semantic_search_enabled';
UPDATE app_settings SET value = 'true' WHERE key = 'web_search_enabled';
