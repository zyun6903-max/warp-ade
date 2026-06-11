# Agent 工具对比与缺口（vs Cursor / Claude Code）

> Git 专用工具（git_commit 等）**不在范围内**——通过 `run_command` + 环境面板即可。

## 当前 warp-ade 内置工具

| 工具 | 说明 |
|------|------|
| read_file | 读工作区文件 |
| write_file | 写/创建文件 |
| apply_patch | 局部替换（≈ StrReplace / Edit） |
| delete_file | 删除工作区文件 |
| glob_files | glob 查找 |
| list_directory | 列目录 |
| grep_project | 项目内搜索（优先 rg） |
| codebase_search | 语义代码搜索（Embedding 索引） |
| web_fetch | 抓取 URL（需确认） |
| web_search | 互联网搜索（Brave / Tavily） |
| search_history | 搜索本地导入对话（独有） |
| run_command | Shell（分级确认） |
| spawn_task | 启动子 Agent（explore / general） |
| mcp_* | MCP 扩展工具 |

## 对比矩阵

| 能力 | Cursor | Claude Code | warp-ade |
|------|--------|-------------|----------|
| Read（行号/offset/limit） | ✅ | ✅ | ✅ |
| Write | ✅ | ✅ | ✅ |
| StrReplace / Edit | ✅ | ✅ | ✅ apply_patch |
| Delete | ✅ | ✅ | ✅ |
| Glob | ✅ | ✅ | ✅ |
| Grep（rg） | ✅ | ✅ | ✅ 优先 rg |
| Bash | ✅ | ✅ | ✅ run_command |
| WebFetch | ✅ | ✅ | ✅ 需确认 |
| WebSearch | ✅ | 可选 | ✅ Brave / Tavily |
| MCP | ✅ | ✅ | ✅ |
| 多工具并行/同轮 | ✅ | ✅ | ✅ 同轮顺序执行 |
| Task / Subagent | ✅ | ✅ | ✅ spawn_task |
| 语义代码搜索 | ✅ | ❌ | ✅ codebase_search |
| Linter/Diagnostics | ✅ | ❌ | ❌ 可接 MCP |
| Notebook 编辑 | ✅ | ✅ | ❌ 低优先级 |
| 图片/多模态读 | ✅ | ✅ | ❌ |
| Skills / Hooks | ✅ | ✅ | ❌ 产品层 |

## 本次迭代（P2 续）

1. read_file：`offset` / `limit` + 行号
2. delete_file
3. grep_project：rg 优先、`-i`、上下文行
4. web_fetch：HTTP GET + 用户确认
5. 同轮多 tool call 顺序执行

## 后续候选

（Agent 工具矩阵主要缺口已补齐）
