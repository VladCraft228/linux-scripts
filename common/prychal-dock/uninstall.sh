#!/usr/bin/env bash
set -euo pipefail
[[ $EUID -ne 0 ]] || { echo 'Run as your regular user, without sudo.' >&2; exit 1; }
if (($#)); then echo 'Usage: bash uninstall.sh' >&2; exit 1; fi
TARGET="$HOME/.local/bin/prychal-dock"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
if [[ -x $TARGET && -n ${DISPLAY:-} ]]; then "$TARGET" --quit || true; fi
rm -f -- "$TARGET" \
    "$DATA_DIR/applications/prychal-dock.desktop" \
    "$DATA_DIR/icons/hicolor/scalable/apps/prychal-dock.svg" \
    "$CONFIG_DIR/autostart/prychal-dock.desktop"
LEGACY_TARGET="$HOME/.local/bin/linux-scripts-dock-rust"
if [[ -L $LEGACY_TARGET && $(readlink -- "$LEGACY_TARGET") == prychal-dock ]]; then
    rm -f -- "$LEGACY_TARGET"
fi
rm -f -- "$DATA_DIR/applications/linux-scripts-dock-rust.desktop" \
    "$DATA_DIR/icons/hicolor/scalable/apps/linux-scripts-dock-rust.svg" \
    "$CONFIG_DIR/autostart/linux-scripts-dock-rust.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DATA_DIR/applications" || true
fi
echo 'Removed Prychal Dock binary, app launcher, icon and autostart.'
echo 'Settings, pins and imported launchers were preserved. Qt Dock was not modified.'
