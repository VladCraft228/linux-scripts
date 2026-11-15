#!/usr/bin/env bash
set -euo pipefail
SOURCE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
[[ ${EUID} -ne 0 ]] || { echo "Run as your regular user, without sudo." >&2; exit 1; }
AUTOSTART=true
CONTEXT_MENU=true
for option in "$@"; do
    case "$option" in
        --no-autostart) AUTOSTART=false ;;
        --no-context-menu) CONTEXT_MENU=false ;;
        *) echo "Usage: bash install.sh [--no-autostart] [--no-context-menu]" >&2; exit 1 ;;
    esac
done
if $CONTEXT_MENU; then command -v python3 >/dev/null || { echo "Python 3 is required for Thunar integration." >&2; exit 1; }; fi
bash "$SOURCE_DIR/build.sh"
TARGET="$HOME/.local/bin/linux-scripts-dock"
mkdir -p -- "$(dirname -- "$TARGET")"
TEMP_TARGET="$(mktemp "${TARGET}.XXXXXX")"
trap 'rm -f -- "$TEMP_TARGET"' EXIT
install -m 755 -- "$SOURCE_DIR/build/linux-scripts-dock" "$TEMP_TARGET"
mv -f -- "$TEMP_TARGET" "$TARGET"
echo "Installed: $TARGET"
        # Desktop Entry Exec quoting: escape reserved characters and literal %.
        ESCAPED="${TARGET//\\/\\\\\\\\}"
        ESCAPED="${ESCAPED//\"/\\\\\\\"}"
        ESCAPED="${ESCAPED//\$/\\\\\$}"
        ESCAPED="${ESCAPED//\`/\\\\\`}"
        ESCAPED="${ESCAPED//%/%%}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPLICATIONS_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/scalable/apps"
mkdir -p -- "$APPLICATIONS_DIR" "$ICON_DIR"
install -m 644 -- "$SOURCE_DIR/assets/linux-scripts-dock.svg" "$ICON_DIR/linux-scripts-dock.svg"
LAUNCHER_TEMP="$(mktemp "$APPLICATIONS_DIR/.linux-scripts-dock.XXXXXX")"
cat > "$LAUNCHER_TEMP" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Linux Scripts Dock
Comment=Customizable application dock for Linux desktops
Exec="$ESCAPED"
Icon=linux-scripts-dock
Terminal=false
StartupNotify=false
Categories=Utility;
StartupWMClass=linux-scripts-dock
DESKTOP
chmod 644 "$LAUNCHER_TEMP"
mv -f -- "$LAUNCHER_TEMP" "$APPLICATIONS_DIR/linux-scripts-dock.desktop"
echo "Application launcher: $APPLICATIONS_DIR/linux-scripts-dock.desktop"
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$APPLICATIONS_DIR" || true; fi
if $AUTOSTART; then
    AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
    ENTRY="$AUTOSTART_DIR/linux-scripts-dock.desktop"
    mkdir -p -- "$AUTOSTART_DIR"
    if [[ ! -e "$ENTRY" ]]; then
        cat > "$ENTRY" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Linux Scripts Dock
Comment=Application dock for XFCE on X11
Exec="$ESCAPED"
Terminal=false
StartupNotify=false
OnlyShowIn=XFCE;
DESKTOP
        echo "Autostart created: $ENTRY"
    else
        echo "Existing autostart entry preserved: $ENTRY"
    fi
fi
if $CONTEXT_MENU; then
    python3 "$SOURCE_DIR/tools/thunar_action.py" install "$TARGET"
    echo "Reopen Thunar to load its updated custom actions."
fi
echo "Run: $TARGET"
echo "If the dock is already running, quit it first to use the new executable."
