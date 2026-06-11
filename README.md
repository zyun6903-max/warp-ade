# warp-ade

结合 Warp、Codex、CC Switch 的本地 AI 开发环境（ADE）。

Mac-first 桌面应用，基于 Tauri 2 + Rust + React。

## 功能

- 多 Provider API Key / Base URL 配置
- AI 对话与 Agent（开发中）
- Cursor / Claude Code 聊天记录导入
- 本地 SQLite + zstd 持久化存储

## 开发

```bash
pnpm install
pnpm tauri dev
```

## 构建

```bash
pnpm tauri build
```
