#!/usr/bin/env bash
set -euo pipefail

if (( EUID == 0 )); then
    echo "Run the Vatra uninstaller as your regular user." >&2
    exit 1
fi

data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
[[ "$data_dir" == /* ]] || data_dir="$HOME/.local/share"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
[[ "$config_dir" == /* ]] || config_dir="$HOME/.config"
rm -f -- "$config_dir/autostart/vatra-tray.desktop" "$HOME/.local/libexec/vatra-tray" "$HOME/.local/bin/vatra" "$data_dir/applications/vatra.desktop" \
    "$data_dir/icons/hicolor/scalable/apps/vatra.svg"
echo "Removed the Vatra launcher and executable."
