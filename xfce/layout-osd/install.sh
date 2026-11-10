#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SOURCE="$SCRIPT_DIR/layout-osd.py"

INSTALL_DIR="$HOME/.local/bin"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
TARGET="$INSTALL_DIR/layout-osd.py"
DESKTOP_FILE="$AUTOSTART_DIR/layout-osd.desktop"

echo "== Layout OSD Installer =="

# Check environment
if [[ "${XDG_SESSION_TYPE:-}" == "wayland" || -n "${WAYLAND_DISPLAY:-}" || -z "${DISPLAY:-}" ]]; then
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

# Stop the installed version, if it supports the control command.
# Legacy processes are matched by exact script path and current user.
if [[ -f "$TARGET" ]]; then
    if grep -q 'instance.lock' "$TARGET"; then
        python3 "$TARGET" --stop
        # Wait for the process to release its lock before starting the replacement.
        python3 - <<'WAIT'
import fcntl
import os
from pathlib import Path
import time
path = Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache"))) / "layout-osd/instance.lock"
with path.open("a+") as lock:
    for _ in range(50):
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            break
        except BlockingIOError:
            time.sleep(0.1)
    else:
        raise SystemExit("Layout OSD did not stop; installation cancelled.")
WAIT
    else
        python3 - "$TARGET" <<'LEGACY'
import os
from pathlib import Path
import signal
import sys
import time
target = Path(sys.argv[1]).resolve()
processes = []
for proc in Path("/proc").iterdir():
    if not proc.name.isdigit():
        continue
    try:
        if proc.stat().st_uid != os.getuid():
            continue
        args = (proc / "cmdline").read_bytes().split(b"\0")
        if len(args) > 1 and args[0].split(b"/")[-1].startswith(b"python"):
            script = Path(os.fsdecode(args[1]))
            if not script.is_absolute():
                script = (proc / "cwd").resolve() / script
            if script.resolve() == target:
                os.kill(int(proc.name), signal.SIGTERM)
                processes.append(proc)
    except (OSError, ValueError):
        continue
for _ in range(50):
    if all(not proc.exists() or b") Z" in (proc / "stat").read_bytes() for proc in processes):
        break
    time.sleep(0.1)
else:
    raise SystemExit("Legacy Layout OSD did not stop; installation cancelled.")
LEGACY
    fi
fi

install -m 755 "$SOURCE" "$TARGET"

# Escape the executable argument for the Desktop Entry Exec grammar.
EXEC_TARGET="$(python3 - "$TARGET" <<'ESCAPE'
import sys
value = sys.argv[1]
for char in ('\\', '"', '`', '$'):
    value = value.replace(char, '\\' + char)
print(value.replace('%', '%%').replace('\\', '\\\\'))
ESCAPE
)"
cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Layout OSD
Comment=Keyboard layout and Caps Lock notifications
Exec=python3 "$EXEC_TARGET"
Icon=input-keyboard
Terminal=false
X-GNOME-Autostart-enabled=true
OnlyShowIn=XFCE;
EOF

echo "Installed: $TARGET"
echo "Autostart: $DESKTOP_FILE"

# Start immediately when running in XFCE
if [[ "${XDG_CURRENT_DESKTOP:-}" == *XFCE* ]]; then
    nohup python3 "$TARGET" \
        >/dev/null 2>&1 </dev/null &
    echo "Layout OSD started."
else
    echo "Log into XFCE to start Layout OSD."
fi

echo "Installation complete!"
