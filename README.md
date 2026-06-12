# warp-ade

轻量本地 App：**自定义模型配置 + 项目 Agent 开发 + MCP/Skills**。  
四核是产品重心；**已有功能全部保留**，后续新增从简、不堆非核心能力。

Mac-first · Tauri 2 + Rust + React

## 下载（macOS）

| 平台 | 要求 | 下载 |
|------|------|------|
| Apple Silicon (M 系列) | macOS 13+ | [warp-ade_0.1.0_aarch64.dmg](https://github.com/zyun6903-max/warp-ade/releases/latest/download/warp-ade_0.1.0_aarch64.dmg) |

## 界面预览

### 对话 · 项目 Agent

在项目工作区中运行 Agent：读/写文件、grep、Shell、工具循环。

![对话界面](docs/screenshots/chat.png)

### 模型服务

配置 Provider、Base URL、模型列表、优先级与 Failover；支持逐模型测试与用量统计。

![模型服务](docs/screenshots/providers.png)

### 扩展 · Skills 与 MCP

管理 Skills 启用状态，配置 MCP Server，扩展 Agent 工具能力。

![扩展设置](docs/screenshots/extensions.png)

### 导入历史

从 Cursor / Claude Code / Codex 批量导入会话，继续对话。

![导入记录](docs/screenshots/import.png)

## 四核能力

| 核 | 来自 | 做什么 |
|----|------|--------|
| **模型配置** | CC Switch | Provider、Base URL、模型、优先级、Failover、Keychain |
| **项目开发** | Codex | 绑定工作区目录，在项目里跑 Agent 改代码 |
| **Agent** | Cursor | 读/写/改文件、grep、shell、工具循环（不是 IDE） |
| **插件/技能** | Claude | MCP Server、Skills、CLAUDE.md 项目上下文 |

## 已有能力（保留）

- 三源历史导入（Cursor / Claude Code / Codex）· 继续对话 · 批量导入进度
- 语义代码搜索 · Web 搜索 · 附件粘贴 · Git 环境面板（分支 / 提交 / 推送）
- 工具/Shell 审计 · Rolling Summary · 会话导出 · MCP 管理 · 原生 Skills 按需加载

> 范围与迭代原则：[`docs/plans/2026-06-11-replacement-parity.md`](docs/plans/2026-06-11-replacement-parity.md)

## 从源码构建

### 环境要求

| 项目 | 要求 |
|------|------|
| 操作系统 | **macOS 13+**（Ventura 及以上） |
| 架构 | **Mac-first**；Apple Silicon 上构建最省事，Intel Mac 可本地编译 x64 包 |
| 磁盘 | 建议预留 **5GB+**（Rust 依赖与编译缓存） |
| 网络 | 首次构建需联网下载 npm / Rust crates |

### 必装工具

```bash
# Xcode Command Line Tools（macOS 编译必备）
xcode-select --install

# Node.js 20.19+ 或 22.12+（Vite 7 要求；20.17 会报警告）
node -v

# pnpm
npm install -g pnpm

# Rust stable
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

另需 **Git**（克隆仓库；应用内 Git 面板也依赖系统 `git`）。

### 克隆与构建

```bash
git clone https://github.com/zyun6903-max/warp-ade.git
cd warp-ade
pnpm install
pnpm tauri build
```

| 产物 | 路径 |
|------|------|
| `.app` | `src-tauri/target/release/bundle/macos/warp-ade.app` |
| `.dmg` | `src-tauri/target/release/bundle/dmg/warp-ade_0.1.0_aarch64.dmg` |

本地刚编译的 `.app` 通常可直接双击运行；首次 Rust 编译约需 **5～15 分钟**。

### 开发调试

```bash
pnpm install
pnpm tauri dev
```

Vite 开发服务器：`http://localhost:1420/`

### 运行时依赖（非构建）

| 用途 | 说明 |
|------|------|
| 模型 API Key | 在应用内「模型服务」配置 Provider |
| Git | 环境面板的分支 / 提交 / 推送 |
| Node.js / npx | 部分 MCP Server（stdio）可能依赖 |
| 网络 | 调用 LLM API、Web 搜索等 |

### 发布新版本

```bash
git tag v0.1.0
git push origin v0.1.0
```

推送 tag 后 GitHub Actions 会自动构建 macOS 安装包并上传到 Release。

### 更新 README 截图

```bash
pnpm preview --port 4173 &
pnpm screenshots
```
