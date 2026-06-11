---
title: "warp-ade 产品范围（轻量四核）"
type: strategy
status: active
date: 2026-06-11
---

# warp-ade 产品范围（轻量四核）

> **定位：** 一个**轻量本地 App**，围绕四核能力演进。**已有功能全部保留，不删不减。**
>
> 「轻量」指的是：**后续新增**时不堆非核心能力，不是把现有代码拆掉。

---

## 原则

| 原则 | 说明 |
|------|------|
| **保留已有** | 已实现的功能、页面、工具、设置项——继续维护，不因「四核」而移除 |
| **四核优先** | 新迭代优先补四核缺口（见下文「待补」） |
| **新增从简** | 不属于四核、且现有能力无法覆盖的新需求 → 默认不做 |

---

## 四核能力（产品重心）

```
┌─────────────────────────────────────────────────────────┐
│  ① 模型配置（CC Switch 核心）                            │
│     自定义 Provider · Base URL · 模型 · 优先级 · Failover │
├─────────────────────────────────────────────────────────┤
│  ② 项目开发（Codex 核心）                                │
│     绑定工作区目录 · 在该项目上下文中跑 Agent · 多步改代码   │
├─────────────────────────────────────────────────────────┤
│  ③ Agent（Cursor 核心）                                  │
│     读/写/改文件 · grep · shell · 工具循环 · 流式对话        │
├─────────────────────────────────────────────────────────┤
│  ④ 插件 / 技能（Claude 核心）                            │
│     MCP Server · Skills 加载 · 与内置工具同一执行链路        │
└─────────────────────────────────────────────────────────┘
```

### ① 模型配置

| 四核内（继续完善） | 后续新增暂不做 |
|-------------------|----------------|
| Provider CRUD、Keychain、优先级、Failover、连接测试 | 系统托盘、Ollama、复杂熔断仪表盘 |
| **待补：** settings.json 与 CC Switch 互通 | 独立代理进程 |

### ② 项目开发

| 四核内 | 后续新增暂不做 |
|--------|----------------|
| 工作区项目、会话绑定目录、Agent 在项目内改代码 | 内置 Terminal、Codex 级容器沙箱 |
| Git 环境面板（已有，保留） | 复杂 Git UI |
| **已做：** CLAUDE.md / AGENTS.md / .cursorrules 自动注入 | Rollout 可视化编辑器 |
| **待补：** 默认 Agent 模式 | |

### ③ Agent

| 四核内 | 后续新增暂不做 |
|--------|----------------|
| 工具循环、流式对话、确认流、工作区策略 | IDE 能力（补全、LSP、内联编辑） |
| 子 Agent spawn_task（已有，保留） | 重型 Subagent 时间线 UI |
| **待补：** 确认流打磨 | @ 符号选择器（可用附件/路径替代） |

### ④ 插件 / 技能

| 四核内 | 后续新增暂不做 |
|--------|----------------|
| MCP 配置与执行（已有，保留） | Hooks 框架、技能市场 |
| **已做：** Skills（`.claude/skills` + `~/.claude/skills`）自动注入 | 多技能编排可视化 |

---

## 已有功能清单（全部保留）

以下**已实现**，属于产品能力的一部分，**不会去掉**：

### 对话 & Agent
- 流式 Markdown 对话、Agent 模式开关
- 内置工具：read / write / patch / delete / glob / grep / codebase_search / web_fetch / web_search / run_command / spawn_task / search_history
- Shell 分级确认、工作区外读写策略
- 工具审计日志、Shell 审计日志
- 粘贴/拖放附件（路径 + read_file 读图）
- 取消生成、partial 消息标记、Failover 提示
- Rolling Summary（Settings 可配）
- **项目上下文自动加载**（CLAUDE.md、Rules、Skills → Agent system prompt）

### 模型 & 路由
- 多 Provider CRUD、拖拽优先级、连接测试、Keychain
- 自动 Failover（429/5xx）

### 项目 & 环境
- 工作区项目、对话容器、会话 CRUD、导出 Markdown
- Git 环境面板（分支、变更、checkout）

### 导入 & 迁移
- Cursor / Claude Code / Codex 扫描与导入
- 批量导入 + 进度条、导入搜索、幂等 re-import
- 导入只读 + **继续对话**（continued_from）

### 扩展
- MCP Server 管理、从 Cursor 导入 MCP
- 语义代码搜索（Embedding 索引、重建）
- Web 搜索（Brave / Tavily，Settings 配置）

### 设置
- Context 预算、Summary 开关、Agent 迭代上限
- Shell 策略、Workspace 外路径策略、语义搜索配置

---

## 后续新增：优先级与克制

**优先做（补四核缺口）：**

| 顺序 | 任务 | 核 |
|------|------|-----|
| 1 | ~~CLAUDE.md / Rules 注入~~ · ~~Skills 加载~~ | ②④ |
| 2 | 读写 `~/.claude/settings.json` | ① |
| 3 | 项目打开默认 Agent | ② |
| 4 | Failover：401 不切换 + 当前 Provider 显示 | ① |

**后续新增暂不做（除非你有明确要求）：**

- IDE 能力（补全、LSP）
- 内置 Terminal、系统托盘
- Cursor state.vscdb 旧历史 backfill
- FTS5 大工程（现有 LIKE 搜索保留，可小步优化）
- Vision API 全量对接（现有附件方案保留）
- Hierarchical Context DAG、Structured memory
- 自动更新、Win/Linux 首发

---

## 实现决策

> **要删已有功能吗？** → 否，除非有 bug 或安全问题。  
> **新功能属于四核吗？** → 否则默认不做。  
> **现有工具/MCP 能覆盖吗？** → 能则不造新的内置能力。

---

## 文档关系

| 文档 | 用途 |
|------|------|
| **本文档** | 四核重心 + 已有能力保留策略 |
| `2026-06-11-feat-cross-platform-ade-plan.md` | 技术架构与 Schema |
| `2026-06-11-agent-tools-gap.md` | Agent 工具对照 |
