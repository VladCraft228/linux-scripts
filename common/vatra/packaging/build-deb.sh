#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
archive="$stage/package"
binary="$archive/usr/bin/vatra"
arch=$(dpkg --print-architecture)
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml)
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Expected a numeric Cargo package version." >&2
    exit 1
fi

# Build for the current Debian host using the committed Cargo.lock.
cargo build --release --locked --target-dir "$stage/target"
install -D -m 0755 "$stage/target/release/vatra" "$binary"
install -D -m 0755 packaging/vatra-tlp-helper "$archive/usr/libexec/vatra-tlp-helper"
install -D -m 0755 packaging/vatra-tray "$archive/usr/libexec/vatra-tray"
install -D -m 0644 packaging/vatra.desktop "$archive/usr/share/applications/vatra.desktop"
install -D -m 0644 assets/vatra.svg "$archive/usr/share/icons/hicolor/scalable/apps/vatra.svg"
install -D -m 0644 ../../LICENSE "$archive/usr/share/doc/vatra/copyright"
install -D -m 0644 README.md "$archive/usr/share/doc/vatra/README.md"
desktop-file-validate "$archive/usr/share/applications/vatra.desktop"

mkdir -- "$stage/debian" "$archive/DEBIAN"
cat > "$stage/debian/control" <<'CONTROL'
Source: vatra
Section: utils
Priority: optional
Maintainer: Linux Scripts <VladCraft228@users.noreply.github.com>

Package: vatra
Architecture: any
Description: Battery and CPU power dashboard
CONTROL
detected=$(cd "$stage" && dpkg-shlibdeps -O -e "$binary")
if [[ "$detected" != shlibs:Depends=* || -z "${detected#shlibs:Depends=}" ]]; then
    echo "Could not determine Vatra's shared-library dependencies." >&2
    exit 1
fi

cat > "$archive/DEBIAN/control" <<CONTROL
Package: vatra
Version: $version
Section: utils
Priority: optional
Architecture: $arch
Maintainer: Linux Scripts <VladCraft228@users.noreply.github.com>
Depends: ${detected#shlibs:Depends=}, fontconfig, libwayland-client0, libwayland-cursor0, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, libxrandr2
Suggests: tlp, pkexec, python3, python3-gi, gir1.2-gtk-3.0
Homepage: https://github.com/VladCraft228/linux-scripts
Description: Battery and CPU power dashboard
 Native Linux dashboard for battery readings, power history and CPU
 energy-performance preferences. Supports optional TLP 1.8 integration.
CONTROL

mkdir -p dist
output="dist/vatra_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$archive" "$output"
