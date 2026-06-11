#!/bin/bash
cd "$(dirname "$0")" || exit 1
bash install-macos.sh
read -r -p "按回车键关闭窗口…"
