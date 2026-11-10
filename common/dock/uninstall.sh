#!/usr/bin/env bash
set -euo pipefail
if [[ ${EUID} -eq 0 ]]; then
    echo "Run as your regular user, without sudo." >&2
    exit 1
fi
rm -f -- "$HOME/.local/bin/linux-scripts-dock"
echo "Removed the installed executable. Quit any running prototype manually."
