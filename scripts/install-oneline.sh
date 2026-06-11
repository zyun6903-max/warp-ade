#!/usr/bin/env bash
# 终端安装（最可靠）：下载 DMG 到「下载」文件夹后运行此脚本
set -euo pipefail

DMG="${1:-$(ls -t "${HOME}/Downloads"/warp-ade_*.dmg 2>/dev/null | head -1)}"
TARGET="/Applications/warp-ade.app"

if [[ ! -f "${DMG}" ]]; then
  echo "请先从 GitHub Releases 下载 warp-ade_*.dmg 到「下载」文件夹"
  echo "https://github.com/zyun6903-max/warp-ade/releases/latest"
  exit 1
fi

echo "→ 使用 ${DMG}"
xattr -cr "${DMG}" 2>/dev/null || true

MOUNT="$(hdiutil attach "${DMG}" -nobrowse | awk -F'\t' '/\/Volumes\// {print $3; exit}')"
APP="${MOUNT}/warp-ade.app"

[[ -d "${APP}" ]] || { echo "DMG 内未找到 warp-ade.app"; hdiutil detach "${MOUNT}" -quiet 2>/dev/null; exit 1; }

echo "→ 安装到「应用程序」"
[[ -d "${TARGET}" ]] && rm -rf "${TARGET}"
cp -R "${APP}" "${TARGET}"
xattr -cr "${TARGET}"
hdiutil detach "${MOUNT}" -quiet

echo "→ 完成"
open "${TARGET}"
