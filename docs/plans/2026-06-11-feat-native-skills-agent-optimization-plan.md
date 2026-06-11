---
title: "feat: 原生 Skills 安装/使用与 Agent 优化"
type: feat
status: active
date: 2026-06-11
---

# feat: 原生 Skills 安装/使用与 Agent 优化

## Overview

warp-ade 已在 Rust 层实现 Skills **被动注入**（`.claude/skills` + `~/.claude/skills` → system prompt），但产品层仍缺：

- **安装**：无向导、无模板、无导入
- **使用**：全文塞进 prompt（24k 上限），无按需加载、无「正在使用某 Skill」的可视反馈
- **管理**：Chat 仅显示「N 个 Skills」计数，Settings 无 Skills / MCP 面板

本计划在不引入 OpenClaw 的前提下，补齐四核 **④ 插件/技能** 的产品体验，并顺带优化 Agent 的 token 效率与 Live UI。

> 边界：遵循 `docs/plans/2026-06-11-replacement-parity.md` —— 不做技能市场、Hooks 框架、多技能编排可视化。

---

## Problem Statement / Motivation

| 现状 | 问题 | 用户期望 |
|------|------|----------|
| 扫描 + 全文注入 | Token 浪费；大技能挤占规则/上下文 | 按需加载，匹配时再读 |
| 手动建目录 | 门槛高 | App 内「新建 / 导入」 |
| MCP 后端完备、UI 缺失 | 扩展能力不可配置 | Settings 可管理 MCP |
| Agent 提示词 | 未强制 Cursor 式 `use_skill` 流程 | 任务匹配 → 显式加载技能 |
| Live 生成 UI | 只见 tool，不见 skill | 显示「已加载 writing-plans」 |

`agent-tools-gap.md` 仍将 Skills 标为 ❌ 产品层；`replacement-parity.md` 标为后端已做 —— 本计划对齐为 **⚠️ 后端 ✅ / 产品 ❌ → 产品补齐**。

---

## Proposed Solution

### 架构概览

```
┌──────────────────────────────────────────────────────────────┐
│  Settings → 扩展                                             │
│    ├── Skills：列表 / 新建 / 导入 / 启用 / 打开目录          │
│    └── MCP：CRUD / 测试 / 从 Cursor 导入（复用现有 IPC）     │
├──────────────────────────────────────────────────────────────┤
│  Chat                                                        │
│    ├── 上下文芯片：规则 · Skills（可展开名称）                 │
│    └── 会话 Pin：固定本次对话要用的 Skills                   │
├──────────────────────────────────────────────────────────────┤
│  Agent Loop (loop_runner.rs)                                 │
│    ├── 启动：扫描 catalog（metadata only）                   │
│    ├── 工具：use_skill(name) → 读 SKILL.md body              │
│    └── 事件：skills-catalog / use_skill → LiveGenerationView │
└──────────────────────────────────────────────────────────────┘
```

### 核心设计决策

1. **Metadata-first 注入**：system prompt 只带 `name + description + path + source`；正文通过 `use_skill` 按需加载（Pin 的技能可 eager load）。
2. **路径统一**：继续兼容 `~/.claude/skills` 与 `{workspace}/.claude/skills`；**Phase 2** 增加 `~/.cursor/skills-cursor` 只读发现。
3. **安装落盘**：MVP 仅用户目录 `~/.claude/skills/{slug}/`；项目级安装 Phase 2。
4. **MCP UI 同批交付**：i18n + CSS 已有，后端 commands 已有 —— 与 Skills 同属「扩展」Tab。
5. **不集成 OpenClaw**（用户已明确取消）。

---

## Technical Considerations

### 后端（Rust）

| 模块 | 变更 |
|------|------|
| `agent/project_context.rs` | `format_skills_catalog()` 仅 metadata；`load_skill_body(name)` |
| `agent/tool_schema.rs` | 新增 `use_skill` 工具定义 |
| `agent/tools.rs` | 执行 `use_skill`；校验 enable 状态 |
| `agent/parser.rs` | 更新 `agent_system_prompt()`：匹配描述时必须 `use_skill` |
| `agent/events.rs` | 可选 `skill-loaded` 事件 |
| `commands.rs` | `list_all_skills`, `create_skill`, `import_skill_folder`, `delete_user_skill`, `set_skill_enabled` |
| `storage/db.rs` | 可选 `session_skill_pins` 或 session metadata JSON |
| `Settings` 已有 MCP commands | 前端接线即可 |

### 前端（React）

| 文件 | 变更 |
|------|------|
| `Settings.tsx` | 新 Tab「扩展」：Skills + MCP 子面板 |
| `Chat.tsx` | 扩展 context chip；Pin UI；`use_skill` live 卡片 |
| `LiveGenerationView.tsx` / `liveStepsDisplay.ts` | 识别 `use_skill` |
| `types.ts` | `SkillEntry` 增加 `enabled`, `source` |
| `i18n/zh.ts` | Skills 管理文案（部分 MCP 已有） |

### 安全

- 安装/导入仅允许写入 `~/.claude/skills/` 下；拒绝 path traversal
- Git clone（Phase 2）需用户确认 + shell 策略
- `use_skill` 只能读取已 catalog 内的路径

---

## Implementation Phases

### Phase 1 — 能用起来（MVP）

| # | 任务 | 交付 |
|---|------|------|
| 1 | Metadata catalog 替代全文注入 | Token 下降；prompt 含技能索引 |
| 2 | `use_skill` 工具 + 审计日志 | Agent 按需读 SKILL.md |
| 3 | Settings → MCP 面板 | 复用 `list/save/delete/test/import_cursor_mcp_servers` |
| 4 | Settings → Skills 列表 | 扫描、启用/禁用、Reveal in Finder、删除用户技能 |
| 5 | Live UI：`use_skill` 卡片 | 用户看见「加载技能 xxx」 |
| 6 | 更新 `agent-tools-gap.md` | Skills 标为 ⚠️→✅ 产品层（MVP） |

### Phase 2 — 安装与管理

| # | 任务 |
|---|------|
| 7 | 新建 Skill 向导（模板 SKILL.md） |
| 8 | 导入文件夹（校验 frontmatter） |
| 9 | Chat 会话 Pin（0–N 技能，持久化到 session） |
| 10 | 发现 `~/.cursor/skills-cursor`（只读） |
| 11 | Plan 模式：可选只读 `use_skill` |

### Phase 3 — Agent 深度优化

| # | 任务 |
|---|------|
| 12 | `spawn_task` 子类型绑定技能（如 explore → codebase 规范） |
| 13 | 技能使用统计（Settings 用量，复用 provider_usage 模式） |
| 14 | Composer `@skill` 快捷选择 |
| 15 | 项目级「安装到工作区 `.claude/skills`」 |

---

## 发散思维：值得做的增强功能

以下按 **价值 × 与四核契合度** 排序，供后续迭代选题（非本计划 MVP 范围）。

### A. Skills & 扩展生态

| 想法 | 说明 | 优先级 |
|------|------|--------|
| **技能模板库** | 内置 5–10 个模板（代码审查、写计划、TDD、中文文档）一键 scaffold | 高 |
| **技能组合 Preset** | 「前端开发」「Rust 后端」预设 = 多个 Pin 的打包 | 中 |
| **从对话生成 Skill** | Agent 完成任务后「保存为 Skill」→ 生成 SKILL.md 草稿 | 中 |
| **技能冲突检测** | 同名不同源时 UI 提示 precedence | 低 |
| **SKILL.md 内嵌校验** | 保存时检查 frontmatter + 必填章节 | 中 |

### B. Agent 体验

| 想法 | 说明 | 优先级 |
|------|------|--------|
| **工具时间线** | 右侧可折叠：每轮 tool/skill 耗时与结果摘要 | 高 |
| **Agent 记忆片段** | 用户标记「记住这个」→ 写入项目 `AGENTS.md` 或 memory MCP | 高 |
| **失败自动降级** | 模型过载已有 failover；扩展为「简化 prompt 重试」 | 中 |
| **并行 read/grep** | 同轮只读工具真并行（当前顺序执行） | 中 |
| **Diff 预览后应用** | `apply_patch` 前在更改面板高亮待应用块 | 高 |
| **任务清单模式** | Agent 输出结构化 checklist，UI 可勾选跟踪 | 中 |

### C. 项目 & 本地控制

| 想法 | 说明 | 优先级 |
|------|------|--------|
| **工作流 YAML** | 项目内 `.warp-ade/workflows/*.yaml`：lint → test → commit 一键跑 | 高 |
| **Git 提交信息生成** | 更改面板「生成 commit message」→ 调模型 | 中 |
| **环境快照** | 记录 node/rust 版本到 session metadata，Agent 知环境 | 低 |
| **文件监听触发 Agent** | 保存某类文件时提示「要运行测试吗」 | 低 |

### D. 模型 & 设置

| 想法 | 说明 | 优先级 |
|------|------|--------|
| **按任务选模型** | Plan 用强推理，执行用快模型（会话级 override） | 高 |
| **CC Switch settings 同步** | replacement-parity 待补 #2 | 高 |
| **Embedding 模型与对话模型分离配置** | 语义搜索独立 Provider | 中 |

### E. 协作 & 可观测

| 想法 | 说明 | 优先级 |
|------|------|--------|
| **会话导出含 Skill  trace** | Markdown 导出标注用了哪些技能 | 低 |
| **Tool/Skill 审计筛选** | Settings 按 skill/tool 过滤审计日志 | 中 |
| **生成成本估算** | 结合已有 provider_usage，按会话展示 Token | 中 |

### 推荐下一批（MVP 之后）

1. 技能模板库 + MCP/Skills 扩展 Tab（本计划 Phase 1–2）
2. Diff 预览后应用 + 工具时间线（Agent 可感知性）
3. 工作流 YAML + 按任务选模型（本地控制深化）

---

## System-Wide Impact

### Interaction Graph

```
Settings 新建 Skill
  → commands::create_skill
  → 写 ~/.claude/skills/{slug}/SKILL.md
  → list_all_skills 刷新

Chat 发送（Agent 模式）
  → send_message → run_agent_turn
  → load_project_context（catalog metadata）
  → stream_agent_completion
  → use_skill("writing-plans") 
  → tools.rs 读文件 → tool audit
  → LiveGenerationView 展示卡片
```

### Error Propagation

- 无效 skill 名 → `use_skill` 返回 tool error，Agent 可换技能或直答
- MCP 服务器 down → 该 server 工具不可用，其余继续（现有行为）
- 导入失败 → UI 展示 stderr，不写半成品目录

### Integration Test Scenarios

1. 禁用某 skill → catalog 不含 → `use_skill` 拒绝
2. Pin 2 个技能 → 会话切换项目 → Pin 清空或按项目隔离
3. 全文注入关闭后 → 大技能不在 prompt，但 `use_skill` 可加载
4. MCP 新增 server → 下轮 Agent 可见 `mcp_*` 工具
5. Plan 模式 → superpowers 技能仍加载；agent `use_skill` 行为符合策略

---

## Acceptance Criteria

### Skills 使用

- [ ] 默认 Agent 路径：prompt 仅含技能 metadata，不含全文
- [ ] `use_skill` 可加载并返回 SKILL.md 正文
- [ ] Live UI 区分 `use_skill` 与普通 read_file
- [ ] 禁用技能不出现在 catalog 且不可 load

### Skills 安装/管理（Phase 2）

- [ ] 向导创建合法 SKILL.md
- [ ] 导入含 SKILL.md 的文件夹成功；否则明确报错
- [ ] 用户技能可删除；项目技能只读展示

### MCP

- [ ] Settings 可 CRUD MCP、测试连接、Cursor 导入
- [ ] 禁用 MCP 后下轮 Agent 无对应工具

### 文档

- [ ] 更新 `agent-tools-gap.md` Skills 行
- [ ] `replacement-parity.md` ④ 待补项勾选 MCP UI + Skills 管理

---

## Dependencies & Risks

| 风险 | 缓解 |
|------|------|
| 模型不主动调 `use_skill` | 强化 system prompt + Pin eager load |
| 与 Claude Code 路径不一致 | 文档说明兼容目录；不强制迁移 |
| Settings  Tab 过多 | 合并为「扩展」单 Tab 两区块 |
| Token 预算仍爆 | catalog 条数上限 + 截断提示 |

---

## Open Questions

1. Pin 存 session 表字段还是 metadata JSON？
2. MVP 是否支持「安装到当前项目 `.claude/skills`」？
3. Plan 模式是否允许 `use_skill`（只读）？

---

## Sources & References

### Internal

- Skills 加载：`src-tauri/src/agent/project_context.rs`
- Plan 技能：`src-tauri/src/agent/plan_mode.rs`
- Agent 循环：`src-tauri/src/agent/loop_runner.rs`
- MCP 后端：`src-tauri/src/mcp/mod.rs`, `commands.rs`
- Chat 上下文：`src/pages/Chat.tsx`（project context hint）
- 四核范围：`docs/plans/2026-06-11-replacement-parity.md`
- 工具矩阵：`docs/plans/2026-06-11-agent-tools-gap.md`

### Research

- Repo 分析：被动注入已实现；MCP UI 缺失为最高 ROI
- SpecFlow：12 条关键流；推荐 build order metadata → use_skill → MCP UI → Skills 管理
