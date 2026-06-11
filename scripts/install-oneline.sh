#!/usr/bin/env bash
# 终端安装：下载 DMG 到「下载」文件夹后运行
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

# 兼容中文系统 hdiutil 输出
ATTACH_OUT="$(hdiutil attach "${DMG}" -nobrowse 2>&1)"
MOUNT="$(echo "${ATTACH_OUT}" | grep -E '/Volumes/' | awk -F'\t' '{print $NF}' | tail -1)"
if [[ -z "${MOUNT}" || ! -d "${MOUNT}" ]]; then
  MOUNT="$(echo "${ATTACH_OUT}" | grep -oE '/Volumes/[^[:space:]]+' | tail -1)"
fi

if [[ -z "${MOUNT}" || ! -d "${MOUNT}" ]]; then
  echo "错误：无法挂载 DMG"
  echo "${ATTACH_OUT}"
  exit 1
fi

APP="${MOUNT}/warp-ade.app"
if [[ ! -d "${APP}" ]]; then
  APP="$(find "${MOUNT}" -maxdepth 1 -name 'warp-ade.app' -type d 2>/dev/null | head -1)"
fi

if [[ ! -d "${APP}" ]]; then
  echo "错误：DMG 内未找到 warp-ade.app（挂载点：${MOUNT}）"
  ls -la "${MOUNT}" || true
  hdiutil detach "${MOUNT}" -quiet 2>/dev/null || true
  exit 1
fi

echo "→ 安装到「应用程序」"
[[ -d "${TARGET}" ]] && rm -rf "${TARGET}"
cp -R "${APP}" "${TARGET}"
xattr -cr "${TARGET}"
hdiutil detach "${MOUNT}" -quiet 2>/dev/null || true

echo "→ 完成"
open "${TARGET}"
