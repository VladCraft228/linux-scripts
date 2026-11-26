#!/usr/bin/env bash
set -euo pipefail
kind=$1
binary=$(realpath -- "$2")
tmp=$(mktemp -d)
pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${pids[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf -- "$tmp"
}
trap cleanup EXIT
export XDG_CONFIG_HOME="$tmp/config" XDG_STATE_HOME="$tmp/state" XDG_RUNTIME_DIR="$tmp/runtime"
export VATRA_TEST_SYSFS="$tmp/sys"
mkdir -m 700 -- "$XDG_RUNTIME_DIR"
battery="$VATRA_TEST_SYSFS/class/power_supply/BAT0"
mkdir -p -- "$battery"
printf '%s\n' Battery > "$battery/type"
printf '%s\n' 68 > "$battery/capacity"
printf '%s\n' Discharging > "$battery/status"
printf '%s\n' 6300000 > "$battery/power_now"
case "$kind" in
    x11)
        unset WAYLAND_DISPLAY WAYLAND_SOCKET
        export XDG_SESSION_TYPE=x11
        trayer >"$tmp/trayer.log" 2>&1 &
        pids+=("$!")
        /usr/bin/python3 - "$PWD/packaging/vatra-tray" <<'PY'
import selectors
import subprocess
import sys

with subprocess.Popen(['/usr/bin/python3', '-I', sys.argv[1]],
                      stdin=subprocess.PIPE, stdout=subprocess.PIPE) as tray:
    try:
        tray.stdin.write(b'68\t0\tbalanced\t0\t0\tffb16b\t1\t0\tVatra 68%\n')
        tray.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(tray.stdout, selectors.EVENT_READ)
            assert selector.select(8), 'GTK tray did not embed under X11'
            assert tray.stdout.readline().strip() == b'embedded'
    finally:
        tray.terminate()
PY
        "$binary" >"$tmp/app.log" 2>&1 &
        pids+=("$!")
        timeout 12s xdotool search --sync --onlyvisible --name '^Vatra$' || { cat "$tmp/app.log"; exit 1; }
        ;;
    wayland)
        unset DISPLAY WAYLAND_SOCKET
        export XDG_SESSION_TYPE=wayland WAYLAND_DISPLAY=vatra-ci
        weston --backend=headless-backend.so --renderer=pixman --socket="$WAYLAND_DISPLAY" \
            --idle-time=0 --no-config --log="$tmp/weston.log" &
        pids+=("$!")
        for ((i=0; i<100; i++)); do
            [[ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]] && break
            sleep .1
        done
        [[ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]] || { cat "$tmp/weston.log"; exit 1; }
        WAYLAND_DEBUG=client "$binary" >"$tmp/app.log" 2>&1 &
        pids+=("$!")
        for ((i=0; i<100; i++)); do
            if grep -qE 'wl_surface@[0-9]+\.attach\(wl_buffer@' "$tmp/app.log" &&
               grep -qE 'wl_surface@[0-9]+\.commit\(' "$tmp/app.log"; then
                exit 0
            fi
            kill -0 "${pids[1]}" 2>/dev/null || break
            sleep .1
        done
        cat "$tmp/weston.log" "$tmp/app.log"
        exit 1
        ;;
    *) exit 2 ;;
esac
