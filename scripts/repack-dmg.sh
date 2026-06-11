#!/usr/bin/env bash
# 在 tauri build 之后，把一键安装脚本打进 DMG
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(node -p "require('${ROOT}/package.json').version")"
ARCH="${WARP_ADE_ARCH:-aarch64}"
APP="${ROOT}/src-tauri/target/release/bundle/macos/warp-ade.app"
DMG_DIR="${ROOT}/src-tauri/target/release/bundle/dmg"
DMG_OUT="${DMG_DIR}/warp-ade_${VERSION}_${ARCH}.dmg"
STAGE="$(mktemp -d)"

cleanup() { rm -rf "${STAGE}"; }
trap cleanup EXIT

if [[ ! -d "${APP}" ]]; then
  echo "错误：请先运行 pnpm tauri build" >&2
  exit 1
fi

echo "→ 准备 DMG 内容"
cp -R "${APP}" "${STAGE}/"
cp "${ROOT}/src-tauri/dmg/安装 warp-ade.command" "${STAGE}/"
cp "${ROOT}/src-tauri/dmg/安装说明.txt" "${STAGE}/"
chmod +x "${STAGE}/安装 warp-ade.command"

echo "→ 重新打包 ${DMG_OUT}"
rm -f "${DMG_OUT}"
hdiutil create \
  -volname "warp-ade ${VERSION}" \
  -srcfolder "${STAGE}" \
  -ov \
  -format UDZO \
  "${DMG_OUT}"

echo "→ 完成：${DMG_OUT}"
