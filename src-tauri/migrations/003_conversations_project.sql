UPDATE projects
SET name = '对话', source_origin = 'conversations'
WHERE name = '默认项目' AND (workspace_path IS NULL OR workspace_path = '');

UPDATE projects
SET name = '对话', source_origin = 'conversations'
WHERE name = '默认项目';
