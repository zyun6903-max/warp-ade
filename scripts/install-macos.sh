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
  local url err
  for url in "${DOWNLOAD_URLS[@]}"; do
    [[ -z "${url}" ]] && continue
    echo "→ 尝试下载 ${url}"
    err="$(mktemp)"
    if curl -fL --retry 2 --connect-timeout 15 --max-time 600 \
      -A "warp-ade-installer/1.0" -o "${DMG_PATH}" "${url}" 2>"${err}"; then
      rm -f "${err}"
      return 0
    fi
    echo "  失败：$(tail -1 "${err}")"
    rm -f "${err}" "${DMG_PATH}"
  done
  return 1
}

# 脚本与 DMG 同目录时（非 curl 管道），优先离线安装
SCRIPT_PATH="${BASH_SOURCE[0]:-}"
if [[ -z "${WARP_ADE_DMG:-}" && -n "${SCRIPT_PATH}" && "${SCRIPT_PATH}" != "bash" && -f "${SCRIPT_PATH}" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${SCRIPT_PATH}")" && pwd)"
  if compgen -G "${SCRIPT_DIR}"/warp-ade_*.dmg > /dev/null; then
    WARP_ADE_DMG="$(ls "${SCRIPT_DIR}"/warp-ade_*.dmg | head -1)"
  fi
fi

if [[ -n "${WARP_ADE_DMG:-}" ]]; then
  if [[ ! -f "${WARP_ADE_DMG}" ]]; then
    echo "错误：找不到本地 DMG：${WARP_ADE_DMG}" >&2
    exit 1
  fi
  echo "→ 使用本地 DMG：${WARP_ADE_DMG}"
  cp "${WARP_ADE_DMG}" "${DMG_PATH}"
elif ! download_dmg; then
  cat >&2 <<EOF
错误：无法从网络下载 DMG。

常见原因：
  · curl: (7) Failed to connect ... port 443  → 公司防火墙/内网拦截 HTTPS，无法访问 GitHub
  · curl: (35) SSL connect error              → 需配置公司代理：export HTTPS_PROXY=http://代理:端口
  · HTTP 403                                  → 部分 CDN 被墙

【推荐】离线安装（无需任何网络）：
  1. 你下载好 DMG，与 install-from-local.sh 一起打包发给同事（U 盘/内网共享）
  2. 同事执行：bash install-from-local.sh

或已有 DMG 时：
  WARP_ADE_DMG=~/Downloads/${DMG_NAME} bash install-from-local.sh
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
