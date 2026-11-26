#!/usr/bin/env bash
set -euo pipefail

if (( EUID == 0 )); then
    echo "Run the Vatra installer as your regular user." >&2
    exit 1
fi

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
bin="$HOME/.local/bin/vatra"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
[[ "$data_dir" == /* ]] || data_dir="$HOME/.local/share"
applications="$data_dir/applications"
icons="$data_dir/icons/hicolor/scalable/apps"
libexec="$HOME/.local/libexec"

# The desktop entry parser decodes the string before parsing Exec arguments.
desktop_argument() {
    local input=$1 output='' char i
    for ((i=0; i<${#input}; i++)); do
        char=${input:i:1}
        case "$char" in
            \\) output+='\\\\' ;;
            '"' ) output+='\\"' ;;
            '$') output+='\\$' ;;
            '`') output+='\\`' ;;
            '%') output+='%%' ;;
            $'\n'|$'\r') return 1 ;;
            *) output+="$char" ;;
        esac
    done
    printf '"%s"' "$output"
}

exec_arg=$(desktop_argument "$bin")
command -v fc-match >/dev/null || { echo "Fontconfig (fc-match) is required." >&2; exit 1; }
if ! pkg-config --exists gio-2.0; then
    echo "GIO build files are required (Debian/MX: libglib2.0-dev; Arch: glib2)." >&2
    exit 1
fi
cargo build --release --locked
mkdir -p -- "$(dirname -- "$bin")" "$applications" "$icons" "$libexec"
desktop_tmp=$(mktemp "$applications/.vatra.XXXXXX")
binary_tmp=''
trap 'rm -f -- "$desktop_tmp" "${binary_tmp:-}"' EXIT
binary_tmp=$(mktemp "$bin.XXXXXX")
build_dir=${CARGO_TARGET_DIR:-target}
install -m 0755 -- "$build_dir/release/vatra" "$binary_tmp"
cat > "$desktop_tmp" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Vatra
Comment=Monitor battery power and CPU power profiles
Exec=$exec_arg
Icon=vatra
StartupWMClass=vatra
Terminal=false
Categories=System;Monitor;
DESKTOP
chmod 0644 -- "$desktop_tmp"
install -m 0644 -- assets/vatra.svg "$icons/vatra.svg"
install -m 0755 -- packaging/vatra-tray "$libexec/vatra-tray"
mv -fT -- "$binary_tmp" "$bin"
mv -fT -- "$desktop_tmp" "$applications/vatra.desktop"
printf 'Installed Vatra. Open it from the application menu or run %s.\n' "$bin"
