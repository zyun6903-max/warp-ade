#!/usr/bin/env bash
# 内部分发安装脚本：下载 DMG → 安装到「应用程序」→ 清除隔离标记 → 启动
set -euo pipefail

VERSION="${WARP_ADE_VERSION:-0.1.0}"
ARCH="${WARP_ADE_ARCH:-aarch64}"
DMG_NAME="warp-ade_${VERSION}_${ARCH}.dmg"
DOWNLOAD_URL="${WARP_ADE_URL:-https://github.com/zyun6903-max/warp-ade/releases/download/v${VERSION}/${DMG_NAME}}"
INSTALL_DIR="/Applications"
APP_NAME="warp-ade.app"
TARGET="${INSTALL_DIR}/${APP_NAME}"
WORKDIR="$(mktemp -d)"
DMG_PATH="${WORKDIR}/${DMG_NAME}"

cleanup() {
  if mountpoint="$(hdiutil info 2>/dev/null | grep -F "${WORKDIR}" | awk '{print $1}' | head -1)"; then
    hdiutil detach "${mountpoint}" -quiet 2>/dev/null || true
  fi
  rm -rf "${WORKDIR}"
}
trap cleanup EXIT

echo "→ 下载 ${DOWNLOAD_URL}"
curl -fsSL -o "${DMG_PATH}" "${DOWNLOAD_URL}"

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
