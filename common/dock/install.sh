#!/usr/bin/env bash
set -euo pipefail
SOURCE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if [[ ${EUID} -eq 0 ]]; then
    echo "Run as your regular user, without sudo." >&2
    exit 1
fi
bash "$SOURCE_DIR/build.sh"
TARGET="$HOME/.local/bin/linux-scripts-dock"
mkdir -p -- "$(dirname -- "$TARGET")"
# Atomic replacement also works while the old executable is still running.
TEMP_TARGET="$(mktemp "${TARGET}.XXXXXX")"
trap 'rm -f -- "$TEMP_TARGET"' EXIT
install -m 755 -- "$SOURCE_DIR/build/linux-scripts-dock" "$TEMP_TARGET"
mv -f -- "$TEMP_TARGET" "$TARGET"
echo "Installed: $TARGET"
echo "Run it manually: $TARGET"
echo "No autostart entry was created."
