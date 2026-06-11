#!/usr/bin/env bash
# 打包 DMG：warp-ade.app + 安装 warp-ade.app（ad-hoc 签名）
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(node -p "require('${ROOT}/package.json').version")"
ARCH="${WARP_ADE_ARCH:-aarch64}"
APP="${ROOT}/src-tauri/target/release/bundle/macos/warp-ade.app"
INSTALLER_SRC="${ROOT}/src-tauri/dmg/Install.app"
DMG_OUT="${ROOT}/src-tauri/target/release/bundle/dmg/warp-ade_${VERSION}_${ARCH}.dmg"
STAGE="$(mktemp -d)"

cleanup() { rm -rf "${STAGE}"; }
trap cleanup EXIT

[[ -d "${APP}" ]] || { echo "请先运行 pnpm tauri build" >&2; exit 1; }

cp -R "${APP}" "${STAGE}/"
cp -R "${INSTALLER_SRC}" "${STAGE}/安装 warp-ade.app"
chmod +x "${STAGE}/安装 warp-ade.app/Contents/MacOS/install"

if [[ -f "${ROOT}/src-tauri/icons/icon.icns" ]]; then
  mkdir -p "${STAGE}/安装 warp-ade.app/Contents/Resources"
  cp "${ROOT}/src-tauri/icons/icon.icns" "${STAGE}/安装 warp-ade.app/Contents/Resources/icon.icns"
fi

codesign -s - --force --deep "${STAGE}/安装 warp-ade.app" 2>/dev/null || true

rm -f "${DMG_OUT}"
hdiutil create -volname "warp-ade" -srcfolder "${STAGE}" -ov -format UDZO "${DMG_OUT}"
echo "→ ${DMG_OUT}"
