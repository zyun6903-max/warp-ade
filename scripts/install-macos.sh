#!/usr/bin/env bash
# 内部分发安装脚本：下载 DMG → 安装到「应用程序」→ 清除隔离标记 → 启动
set -euo pipefail

VERSION="${WARP_ADE_VERSION:-0.1.0}"
ARCH="${WARP_ADE_ARCH:-aarch64}"
DMG_NAME="warp-ade_${VERSION}_${ARCH}.dmg"
INSTALL_DIR="/Applications"
APP_NAME="warp-ade.app"
TARGET="${INSTALL_DIR}/${APP_NAME}"
WORKDIR="$(mktemp -d)"
DMG_PATH="${WORKDIR}/${DMG_NAME}"

# 自定义下载地址（优先）；或本地 DMG：WARP_ADE_DMG=/path/to/file.dmg
DEFAULT_GITHUB="https://github.com/zyun6903-max/warp-ade/releases/download/v${VERSION}/${DMG_NAME}"
DOWNLOAD_URLS=(
  "${WARP_ADE_URL:-}"
  "${DEFAULT_GITHUB}"
  "https://ghfast.top/${DEFAULT_GITHUB}"
  "https://mirror.ghproxy.com/${DEFAULT_GITHUB}"
)

cleanup() {
  if mountpoint="$(hdiutil info 2>/dev/null | grep -F "${WORKDIR}" | awk '{print $1}' | head -1)"; then
    hdiutil detach "${mountpoint}" -quiet 2>/dev/null || true
  fi
  rm -rf "${WORKDIR}"
}
trap cleanup EXIT

download_dmg() {
  local url tmp
  for url in "${DOWNLOAD_URLS[@]}"; do
    [[ -z "${url}" ]] && continue
    echo "→ 尝试下载 ${url}"
    if curl -fL --retry 2 --connect-timeout 15 --max-time 600 \
      -A "warp-ade-installer/1.0" -o "${DMG_PATH}" "${url}"; then
      return 0
    fi
    echo "  下载失败，尝试下一个镜像…"
    rm -f "${DMG_PATH}"
  done
  return 1
}

if [[ -n "${WARP_ADE_DMG:-}" ]]; then
  if [[ ! -f "${WARP_ADE_DMG}" ]]; then
    echo "错误：找不到本地 DMG：${WARP_ADE_DMG}" >&2
    exit 1
  fi
  echo "→ 使用本地 DMG：${WARP_ADE_DMG}"
  cp "${WARP_ADE_DMG}" "${DMG_PATH}"
elif ! download_dmg; then
  cat >&2 <<EOF
错误：所有下载源均失败（常见为 403 / 网络无法访问 GitHub）。

请任选一种方式：
  1. 浏览器打开 Releases 手动下载 DMG，然后执行：
     WARP_ADE_DMG=~/Downloads/${DMG_NAME} bash install-macos.sh
  2. 使用国内镜像拉取脚本后再装本地包：
     curl -fsSL https://cdn.jsdelivr.net/gh/zyun6903-max/warp-ade@main/scripts/install-macos.sh -o install-macos.sh
     WARP_ADE_DMG=~/Downloads/${DMG_NAME} bash install-macos.sh
EOF
  exit 1
fi

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
