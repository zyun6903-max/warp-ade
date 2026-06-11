#!/usr/bin/env bash
# 纯离线安装：不访问任何网络。与 DMG 放在同一目录，或通过 WARP_ADE_DMG 指定路径。
set -euo pipefail

INSTALL_DIR="/Applications"
APP_NAME="warp-ade.app"
TARGET="${INSTALL_DIR}/${APP_NAME}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ -n "${WARP_ADE_DMG:-}" ]]; then
  DMG_PATH="${WARP_ADE_DMG}"
elif compgen -G "${SCRIPT_DIR}"/warp-ade_*.dmg > /dev/null; then
  DMG_PATH="$(ls "${SCRIPT_DIR}"/warp-ade_*.dmg | head -1)"
elif compgen -G "${HOME}/Downloads"/warp-ade_*.dmg > /dev/null; then
  DMG_PATH="$(ls "${HOME}/Downloads"/warp-ade_*.dmg | head -1)"
else
  cat >&2 <<EOF
错误：未找到 DMG 文件。

请从 GitHub Releases 下载 warp-ade_*.dmg，与本脚本放在同一目录（或「下载」文件夹），然后重试。
也可指定路径：
  WARP_ADE_DMG=~/Downloads/warp-ade_0.1.0_aarch64.dmg bash install-from-local.sh
EOF
  exit 1
fi

if [[ ! -f "${DMG_PATH}" ]]; then
  echo "错误：找不到 ${DMG_PATH}" >&2
  exit 1
fi

echo "→ 离线安装，使用 ${DMG_PATH}"
echo "→ 清除 DMG 隔离标记"
xattr -cr "${DMG_PATH}" 2>/dev/null || true

echo "→ 挂载并安装到 ${INSTALL_DIR}"
MOUNT_OUTPUT="$(hdiutil attach -nobrowse -readonly "${DMG_PATH}")"
MOUNT_POINT="$(echo "${MOUNT_OUTPUT}" | awk -F'\t' '/\/Volumes\// {print $3; exit}')"
SRC="$(find "${MOUNT_POINT}" -maxdepth 1 -name '*.app' | head -1)"

if [[ -z "${SRC}" || ! -d "${SRC}" ]]; then
  echo "错误：在 DMG 中未找到 .app" >&2
  exit 1
fi

if [[ -d "${TARGET}" ]]; then
  echo "→ 替换已有安装"
  rm -rf "${TARGET}"
fi

cp -R "${SRC}" "${TARGET}"
hdiutil detach "${MOUNT_POINT}" -quiet

echo "→ 清除应用隔离标记"
xattr -cr "${TARGET}"

echo "→ 完成，正在启动 warp-ade"
open "${TARGET}"
