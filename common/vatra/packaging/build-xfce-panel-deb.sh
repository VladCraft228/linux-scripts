#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

if ! pkg-config --exists 'libxfce4panel-2.0 >= 4.14' gtk+-3.0 gio-2.0; then
    echo "Install libxfce4panel-2.0-dev and libgtk-3-dev to build Vatra Panel." >&2
    exit 1
fi

stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
archive="$stage/package"
module="$archive/usr/lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)/xfce4/panel/plugins/libvatra-panel.so"
arch=$(dpkg --print-architecture)
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Expected a numeric Cargo package version." >&2
    exit 1
fi

mkdir -p -- "$(dirname -- "$module")" "$stage/debian" "$archive/DEBIAN"
read -r -a cflags <<< "$(pkg-config --cflags libxfce4panel-2.0 gtk+-3.0 gio-2.0)"
read -r -a libs <<< "$(pkg-config --libs libxfce4panel-2.0 gtk+-3.0 gio-2.0)"
"${CC:-cc}" -std=gnu11 -Wall -Wextra -O2 -fPIC -shared -Wl,-z,defs \
    "${cflags[@]}" packaging/xfce/vatra-panel.c -o "$module" "${libs[@]}"
install -D -m 0644 packaging/xfce/vatra-panel.desktop \
    "$archive/usr/share/xfce4/panel/plugins/vatra-panel.desktop"
install -D -m 0644 ../../LICENSE "$archive/usr/share/doc/vatra-xfce-panel/copyright"

cat > "$stage/debian/control" <<'CONTROL'
Source: vatra-xfce-panel
Section: xfce
Priority: optional
Maintainer: Linux Scripts <VladCraft228@users.noreply.github.com>

Package: vatra-xfce-panel
Architecture: any
Description: Battery indicator for the Xfce panel
CONTROL
detected=$(cd "$stage" && dpkg-shlibdeps -O -e "$module")
if [[ "$detected" != shlibs:Depends=* || -z "${detected#shlibs:Depends=}" ]]; then
    echo "Could not determine Vatra Panel's shared-library dependencies." >&2
    exit 1
fi

cat > "$archive/DEBIAN/control" <<CONTROL
Package: vatra-xfce-panel
Version: $version
Section: xfce
Priority: optional
Architecture: $arch
Maintainer: Linux Scripts <VladCraft228@users.noreply.github.com>
Depends: ${detected#shlibs:Depends=}, vatra, xfce4-panel (>= 4.14), upower
Homepage: https://github.com/VladCraft228/linux-scripts
Description: Battery indicator for the Xfce panel
 Shows battery charge as a themed icon and a full-size text label.
 Click the indicator to open the Vatra power dashboard.
CONTROL

mkdir -p -- dist
dpkg-deb --root-owner-group --build "$archive" "dist/vatra-xfce-panel_${version}_${arch}.deb"
