# warp-ade

轻量本地 App：**自定义模型配置 + 项目 Agent 开发 + MCP/Skills**。  
四核是产品重心；**已有功能全部保留**，后续新增从简、不堆非核心能力。

Mac-first · Tauri 2 + Rust + React

## 下载（macOS）

| 平台 | 要求 | 下载 |
|------|------|------|
| Apple Silicon (M 系列) | macOS 13+ | [warp-ade_0.1.0_aarch64.dmg](https://github.com/zyun6903-max/warp-ade/releases/latest/download/warp-ade_0.1.0_aarch64.dmg) |

> ⚠️ **不要直接双击安装**。当前版本未签名，从 GitHub 下载后 macOS 会误报「**已损坏，无法打开**」——安装包没问题，见下方正确步骤。

### 同事安装步骤（能下载 GitHub，但双击报错时）

**原因**：浏览器下载会加隔离标记 + 应用未 Apple 签名 → Gatekeeper 拦截，不是网络问题。

**推荐：下载 DMG + 运行安装脚本（2 步）**

1. 从 [Releases](https://github.com/zyun6903-max/warp-ade/releases/latest) 下载 `warp-ade_0.1.0_aarch64.dmg` 到「下载」文件夹  
2. 再下载 [install-from-local.sh](https://raw.githubusercontent.com/zyun6903-max/warp-ade/main/scripts/install-from-local.sh) 到同一文件夹  
3. 打开终端，执行：

```bash
bash ~/Downloads/install-from-local.sh
```

脚本会自动找到「下载」里的 DMG → 清除隔离标记 → 安装到「应用程序」→ 启动。

也可双击 `install-from-local.command`（需先把 `.command` 和 DMG 放在同一目录）。

**或手动 3 条命令**（不用脚本）：

```bash
xattr -cr ~/Downloads/warp-ade_0.1.0_aarch64.dmg
open ~/Downloads/warp-ade_0.1.0_aarch64.dmg
# 拖入「应用程序」后：
xattr -cr /Applications/warp-ade.app && open /Applications/warp-ade.app
```

**不用终端**：对 `warp-ade.app` 使用 **Control + 点击 → 打开 → 仍要打开**（DMG 拖入「应用程序」后操作一次）。

首次使用请先在 **模型服务** 中配置 Provider 与 API Key。

> 最新版本与更新说明见 [Releases](https://github.com/zyun6903-max/warp-ade/releases)

### 其他情况

#### 在线一键安装（能 curl 且能下载外网时）

```bash
curl -fsSL https://cdn.jsdelivr.net/gh/zyun6903-max/warp-ade@main/scripts/install-macos.sh | bash
```

#### 公司内网完全上不了 GitHub（port 443 连不上）

由管理员打包 `warp-ade_*.dmg` + `install-from-local.sh` 通过 U 盘/内网发给同事，执行 `bash install-from-local.sh`（无需网络）。

#### 分发注意

| 做法 | 结果 |
|------|------|
| 同事自己从 GitHub Releases 下载 + 安装脚本 | ✅ 推荐 |
| 只双击 DMG / 只拖入应用程序后双击 | ❌ 几乎必报「已损坏」 |
| 钉钉 / 微信转发 `.dmg` | ❌ 隔离标记更重 |
| Apple 签名 + 公证（$99/年） | ✅ 可像普通软件直接双击 |

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
