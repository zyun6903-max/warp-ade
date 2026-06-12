# warp-ade

**一个 App，换掉 Cursor + Claude Code + Codex + CC Switch。**

本地原生应用，支持 **macOS · Windows**。模型自己配、项目自己绑、Agent 直接在仓库里改代码——不用 IDE 臃肿，不用切四个工具。

[下载 macOS 版 →](https://github.com/zyun6903-max/warp-ade/releases/latest/download/warp-ade_0.1.0_aarch64.dmg) · [下载 Windows 版 →](https://github.com/zyun6903-max/warp-ade/releases/latest/download/warp-ade_0.1.0_x64.zip)

---

## 界面预览

### 对话 · 项目 Agent

在项目里跑 Agent：读/写/改文件、grep、Shell、多步工具循环，流式输出，边聊边改。

![对话界面](docs/screenshots/chat.png)

### 模型服务

多 Provider 统一管理：Base URL、模型列表、拖拽优先级、自动 Failover、Keychain 存 Key、逐模型连通测试。

![模型服务](docs/screenshots/providers.png)

### 扩展 · Skills 与 MCP

Skills 按需加载，MCP Server 即配即用，Agent 工具能力随时扩展。

![扩展设置](docs/screenshots/extensions.png)

### 导入历史

Cursor / Claude Code / Codex 会话一键迁入，导入后继续对话，换工具不丢上下文。

![导入记录](docs/screenshots/import.png)

---

## 已实现

### 项目 Agent

- 绑定工作区，在项目上下文中多步改代码
- 内置工具：读 / 写 / 补丁 / 删 / glob / grep / 语义搜索 / Web 抓取 / Web 搜索 / Shell / 子任务
- 流式 Markdown 对话，Agent 模式一键切换
- Shell 分级确认，工作区外读写可配策略
- 粘贴、拖放附件；图片随消息发送
- 取消生成、Failover 自动切换、Rolling Summary 长对话压缩
- **CLAUDE.md · AGENTS.md · .cursorrules · Skills 自动注入 Agent 上下文**

### 模型路由

- 多 Provider CRUD，拖拽排优先级
- 429 / 5xx 自动 Failover，少掉线
- API Key 本地加密存储，连接测试 + 用量统计

### 项目与环境

- 工作区项目管理，会话绑定目录
- Git 环境面板：分支、变更、checkout、提交、推送
- 会话导出 Markdown

### 迁移与扩展

- **三源导入**：Cursor / Claude Code / Codex，批量导入 + 进度条，幂等可重复导入
- 导入会话**继续对话**，历史不浪费
- MCP Server 管理，支持从 Cursor 导入配置
- 语义代码搜索（Embedding 索引，可重建）
- Web 搜索（Brave / Tavily 可配）

### 安全与可控

- 工具调用审计、Shell 审计日志
- Context 预算、Agent 迭代上限、Summary 开关——用量你说了算

---

## 下载

| 平台 | 要求 | 下载 |
|------|------|------|
| Apple Silicon (M 系列) | macOS 13+ | [warp-ade_0.1.0_aarch64.dmg](https://github.com/zyun6903-max/warp-ade/releases/latest/download/warp-ade_0.1.0_aarch64.dmg) |
| Windows | Windows 10/11 · x64 | [warp-ade_0.1.0_x64.zip](https://github.com/zyun6903-max/warp-ade/releases/latest/download/warp-ade_0.1.0_x64.zip) |
