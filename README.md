# warp-ade

轻量本地 App：**自定义模型配置 + 项目 Agent 开发 + MCP/Skills**。  
四核是产品重心；**已有功能全部保留**，后续新增从简、不堆非核心能力。

Mac-first · Tauri 2 + Rust + React

## 下载（macOS）

| 平台 | 要求 | 下载 |
|------|------|------|
| Apple Silicon (M 系列) | macOS 13+ | [warp-ade_0.1.0_aarch64.dmg](https://github.com/zyun6903-max/warp-ade/releases/latest/download/warp-ade_0.1.0_aarch64.dmg) |

### 安装（2 步）

1. **双击打开** DMG  
2. **双击**「**安装 warp-ade.command**」

若提示无法打开：**Control + 点击**「安装 warp-ade.command」→ **打开** → **仍要打开**（只需首次一次）。

安装程序会自动清除隔离标记、复制到「应用程序」并启动。**无需终端、无需额外脚本。**

> 旧版 DMG 没有内置安装程序，请重新下载最新 Release。

首次使用请先在 **模型服务** 中配置 Provider 与 API Key。

> 最新版本与更新说明见 [Releases](https://github.com/zyun6903-max/warp-ade/releases)

<details>
<summary>其他安装方式（终端 / 内网）</summary>

```bash
# 已有 DMG，终端一键安装
bash ~/Downloads/install-from-local.sh
```

```bash
# 手动
xattr -cr ~/Downloads/warp-ade_0.1.0_aarch64.dmg
open ~/Downloads/warp-ade_0.1.0_aarch64.dmg
xattr -cr /Applications/warp-ade.app && open /Applications/warp-ade.app
```

</details>

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

## 开发

```bash
pnpm install
pnpm tauri dev
```

## 构建

```bash
pnpm tauri build
```

产物位于 `src-tauri/target/release/bundle/`（`.app` 与 `.dmg`）。

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
