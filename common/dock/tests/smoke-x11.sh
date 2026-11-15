#!/usr/bin/env bash
set -euo pipefail
SOURCE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${1:-$SOURCE_DIR/build/linux-scripts-dock}"
TEMP_DIR="$(mktemp -d)"
cleanup() {
    for pid in "${dock_pid:-}" "${client_pid:-}" "${wm_pid:-}"; do
        if [[ -n "$pid" ]]; then kill "$pid" 2>/dev/null || true; fi
    done
    rm -rf -- "$TEMP_DIR"
}
trap cleanup EXIT
export XDG_RUNTIME_DIR="$TEMP_DIR/runtime"
mkdir -m 700 "$XDG_RUNTIME_DIR"
export XDG_CONFIG_HOME="$TEMP_DIR/config"
export XDG_DATA_HOME="$TEMP_DIR/data"
export XDG_SESSION_TYPE=x11
unset WAYLAND_DISPLAY
export QT_QUICK_BACKEND=software
openbox >"$TEMP_DIR/wm.log" 2>&1 &
wm_pid=$!
for _ in {1..50}; do
    if xprop -root _NET_SUPPORTING_WM_CHECK | grep -q 'window id'; then break; fi
    sleep 0.1
done
xmessage -title DockSmokeTest ready >"$TEMP_DIR/client.log" 2>&1 &
client_pid=$!
launcher="$TEMP_DIR/Cold launcher.desktop"
printf '[Desktop Entry]\nType=Application\nName=Cold Launcher Test\nExec=/usr/bin/touch %s\n' "$TEMP_DIR/not-launched" > "$launcher"
"$BINARY" --pin-desktop "$launcher" >"$TEMP_DIR/dock.log" 2>&1 &
dock_pid=$!
dock_id=""
for _ in {1..50}; do
    if ! kill -0 "$dock_pid" 2>/dev/null; then cat "$TEMP_DIR/dock.log"; exit 1; fi
    dock_id="$(xdotool search --name 'Linux Scripts Dock' 2>/dev/null | head -n 1 || true)"
    if [[ -n "$dock_id" ]]; then break; fi
    sleep 0.1
done
[[ -n "$dock_id" ]] || { cat "$TEMP_DIR/dock.log"; exit 1; }
xprop -id "$dock_id" _NET_WM_WINDOW_TYPE | grep -q '_NET_WM_WINDOW_TYPE_DOCK'
if "$BINARY" >"$TEMP_DIR/second.log" 2>&1; then
    echo "A second instance unexpectedly succeeded." >&2
    exit 1
fi
grep -q 'already running' "$TEMP_DIR/second.log"
if XDG_SESSION_TYPE=wayland "$BINARY" >"$TEMP_DIR/wayland.log" 2>&1; then exit 1; fi
grep -q 'X11 only' "$TEMP_DIR/wayland.log"
sleep 0.3
kill -0 "$dock_pid"
[[ -e "$launcher" && ! -e "$TEMP_DIR/not-launched" ]]
launcher_hash="$(sha256sum "$launcher")"
grep -Rq "${launcher_hash%% *}" "$XDG_CONFIG_HOME"
if ! grep -q 'Local launcher integration unavailable' "$TEMP_DIR/dock.log"; then
    warm_launcher="$TEMP_DIR/Warm launcher.desktop"
    printf '[Desktop Entry]\nType=Application\nName=Warm Launcher Test\nExec=/usr/bin/touch %s\n' "$TEMP_DIR/not-launched" > "$warm_launcher"
    "$BINARY" --pin-desktop "$warm_launcher"
    warm_hash="$(sha256sum "$warm_launcher")"
    grep -Rq "${warm_hash%% *}" "$XDG_CONFIG_HOME"
    [[ -e "$warm_launcher" && ! -e "$TEMP_DIR/not-launched" ]]
    echo "PASS: launcher forwarded to the existing instance."
else
    echo "Local socket binding unavailable; existing-instance forwarding needs a native environment."
fi
if grep -E 'ReferenceError|TypeError|Binding loop|failed to load component|is not installed|Cannot assign' "$TEMP_DIR/dock.log"; then
    cat "$TEMP_DIR/dock.log"
    exit 1
fi
cat "$TEMP_DIR/dock.log"
echo "PASS: cold launcher import without execution, QML startup, X11 dock type, single instance, Wayland rejection."
