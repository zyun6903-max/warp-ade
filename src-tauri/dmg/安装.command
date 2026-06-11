#!/bin/bash
# 双击运行：清除隔离标记 → 安装到「应用程序」→ 启动
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
APP="${DIR}/warp-ade.app"
TARGET="/Applications/warp-ade.app"

[[ -d "${APP}" ]] || { osascript -e 'display alert "安装失败" message "未找到 warp-ade.app"'; exit 1; }

xattr -cr "${DIR}" 2>/dev/null || true
[[ -d "${TARGET}" ]] && rm -rf "${TARGET}"
cp -R "${APP}" "${TARGET}"
xattr -cr "${TARGET}"
open "${TARGET}"
