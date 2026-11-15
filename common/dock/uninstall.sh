#!/usr/bin/env bash
set -euo pipefail
[[ ${EUID} -ne 0 ]] || { echo "Run as your regular user, without sudo." >&2; exit 1; }
SOURCE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
python3 "$SOURCE_DIR/tools/thunar_action.py" remove
rm -f -- "$HOME/.local/bin/linux-scripts-dock"
rm -f -- "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/linux-scripts-dock.desktop"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f -- "$DATA_DIR/applications/linux-scripts-dock.desktop"
rm -f -- "$DATA_DIR/icons/hicolor/scalable/apps/linux-scripts-dock.svg"
echo "Removed Dock, its app launcher, icon and autostart/Thunar entries. Quit any running copy."
echo "Pinned launchers and settings are preserved."
