#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SOURCE="$SCRIPT_DIR/layout-osd.py"

INSTALL_DIR="$HOME/.local/bin"
AUTOSTART_DIR="$HOME/.config/autostart"
TARGET="$INSTALL_DIR/layout-osd.py"
DESKTOP_FILE="$AUTOSTART_DIR/layout-osd.desktop"

echo "== Layout OSD Installer =="

# Check environment
if [[ -z "${DISPLAY:-}" ]]; then
    echo "Error: X11 session required."
    exit 1
fi

if [[ ! -f "$SOURCE" ]]; then
    echo "Error: layout-osd.py not found."
    exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
    echo "Error: Python 3 is required."
    exit 1
fi

# Check Python and GTK dependencies
if ! python3 -c '
import gi
gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
from gi.repository import Gdk, GLib, Gtk
import ctypes
ctypes.CDLL("libX11.so.6")
' >/dev/null 2>&1; then
    echo "Missing GTK3 / X11 dependencies."
    echo "On Debian / MX Linux run:"
    echo "sudo apt install python3-gi gir1.2-gtk-3.0 libx11-6 x11-xkb-utils"
    exit 1
fi

if ! command -v setxkbmap >/dev/null 2>&1; then
    echo "Error: setxkbmap is missing."
    exit 1
fi

# Install
mkdir -p "$INSTALL_DIR" "$AUTOSTART_DIR"

# Stop previous instance only when updating the installed script
if [[ -f "$TARGET" ]] && command -v pgrep >/dev/null 2>&1; then
    while IFS= read -r pid; do
        [[ -n "$pid" ]] || continue
        [[ "$pid" == "$$" ]] && continue
        kill "$pid" 2>/dev/null || true
    done < <(pgrep -f "^(/usr/bin/)?python3? $TARGET$" || true)
fi

install -m 755 "$SOURCE" "$TARGET"

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Layout OSD
Comment=Keyboard layout and Caps Lock notifications
Exec=/usr/bin/python3 $TARGET
Icon=input-keyboard
Terminal=false
X-GNOME-Autostart-enabled=true
OnlyShowIn=XFCE;
EOF

echo "Installed: $TARGET"
echo "Autostart: $DESKTOP_FILE"

# Start immediately when running in XFCE
if [[ "${XDG_CURRENT_DESKTOP:-}" == *XFCE* ]]; then
    nohup /usr/bin/python3 "$TARGET" \
        >/dev/null 2>&1 </dev/null &
    echo "Layout OSD started."
else
    echo "Log into XFCE to start Layout OSD."
fi

echo "Installation complete!"
