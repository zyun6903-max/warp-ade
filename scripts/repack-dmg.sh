#!/usr/bin/env bash
# 打包 DMG：warp-ade.app + 安装.command（2 个文件，无教程）
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(node -p "require('${ROOT}/package.json').version")"
ARCH="${WARP_ADE_ARCH:-aarch64}"
APP="${ROOT}/src-tauri/target/release/bundle/macos/warp-ade.app"
DMG_OUT="${ROOT}/src-tauri/target/release/bundle/dmg/warp-ade_${VERSION}_${ARCH}.dmg"
STAGE="$(mktemp -d)"

cleanup() { rm -rf "${STAGE}"; }
trap cleanup EXIT

[[ -d "${APP}" ]] || { echo "请先运行 pnpm tauri build" >&2; exit 1; }

cp -R "${APP}" "${STAGE}/"
cp "${ROOT}/src-tauri/dmg/安装.command" "${STAGE}/"
chmod +x "${STAGE}/安装.command"

rm -f "${DMG_OUT}"
hdiutil create -volname "warp-ade" -srcfolder "${STAGE}" -ov -format UDZO "${DMG_OUT}"
echo "→ ${DMG_OUT}"
