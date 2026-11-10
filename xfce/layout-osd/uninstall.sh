#!/usr/bin/env bash
set -euo pipefail
TARGET="$HOME/.local/bin/layout-osd.py"
DESKTOP_FILE="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/layout-osd.desktop"
if [[ -f "$TARGET" ]]; then
    if grep -q 'instance.lock' "$TARGET"; then
        python3 "$TARGET" --stop
    else
        echo "Legacy version: stop layout-osd.py through XFCE Task Manager first."
        exit 1
    fi
fi
rm -f -- "$DESKTOP_FILE" "$TARGET"
echo "Layout OSD uninstalled."
