#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

desktop="${XDG_CURRENT_DESKTOP:-}:${DESKTOP_SESSION:-}"
desktop=":${desktop,,}:"
desktop="${desktop//;/:}"
with_panel=false
[[ "$desktop" != *:xfce:* ]] || with_panel=true
package_dir=
while (( $# )); do
    case "$1" in
        --with-xfce-panel) with_panel=true ;;
        --tray-only) with_panel=false ;;
        --from-dir)
            if (( $# < 2 )); then
                echo "Specify the directory containing the Debian packages." >&2
                exit 2
            fi
            shift
            package_dir=$(realpath -- "$1")
            ;;
        -h|--help)
            echo "Usage: bash packaging/install-deb.sh [--with-xfce-panel | --tray-only] [--from-dir DIRECTORY]"
            exit 0
            ;;
        *) printf 'Unknown option: %s\n' "$1" >&2; exit 2 ;;
    esac
    shift
done

arch=$(dpkg --print-architecture)
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Expected a numeric Cargo package version." >&2
    exit 1
fi

if [[ -z "$package_dir" ]]; then
    if $with_panel && ! pkg-config --print-errors --exists 'libxfce4panel-2.0 >= 4.14' gtk+-3.0; then
        echo "Install libxfce4panel-2.0-dev, libxfce4ui-2-dev and libgtk-3-dev, or use --tray-only." >&2
        exit 1
    fi
    bash packaging/build-deb.sh
    if $with_panel; then
        bash packaging/build-xfce-panel-deb.sh
    fi
    package_dir="$PWD/dist"
fi

packages=("$package_dir/vatra_${version}_${arch}.deb")
if $with_panel; then
    packages+=("$package_dir/vatra-xfce-panel_${version}_${arch}.deb")
fi
for package in "${packages[@]}"; do
    if [[ ! -r "$package" ]]; then
        printf 'Missing package: %s\n' "$package" >&2
        exit 1
    fi
done

sudo apt install --reinstall "${packages[@]}"
