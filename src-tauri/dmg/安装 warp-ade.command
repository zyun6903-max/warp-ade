#!/bin/bash
# 在 DMG 内运行：清除隔离标记 → 安装到「应用程序」→ 启动
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
APP="${DIR}/warp-ade.app"
TARGET="/Applications/warp-ade.app"

if [[ ! -d "${APP}" ]]; then
  osascript -e 'display alert "未找到 warp-ade.app" message "请重新下载安装包。"'
  exit 1
fi

echo "→ 清除隔离标记"
xattr -cr "${DIR}" 2>/dev/null || true

echo "→ 安装到「应用程序」"
if [[ -d "${TARGET}" ]]; then
  rm -rf "${TARGET}"
fi
cp -R "${APP}" "${TARGET}"
xattr -cr "${TARGET}"

echo "→ 启动 warp-ade"
open "${TARGET}"

osascript -e 'display notification "warp-ade 已安装到「应用程序」" with title "安装完成"'
