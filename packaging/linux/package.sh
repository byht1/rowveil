#!/usr/bin/env bash
# Packages the Linux build: a .deb and a portable .tar.gz.
# Run after building the server (cargo build --release --no-default-features --bin rowveil-server)
# and the desktop app (cargo build --release in desktop/).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
bin="$root/target/release"
desktop="$root/desktop/target/release"
dist="$root/dist"
mkdir -p "$dist"

# ---- .deb
pkg="$(mktemp -d)/rowveil"
install -Dm755 "$desktop/rowveil" "$pkg/usr/bin/rowveil"
install -Dm755 "$bin/rowveil-server" "$pkg/usr/bin/rowveil-server"
install -Dm644 "$root/packaging/linux/rowveil.desktop" "$pkg/usr/share/applications/rowveil.desktop"
for s in 128 256 512; do
  install -Dm644 "$root/assets/icons/rowveil-$s.png" "$pkg/usr/share/icons/hicolor/${s}x${s}/apps/rowveil.png"
done
install -Dm644 "$root/LICENSE" "$pkg/usr/share/doc/rowveil/copyright"
mkdir -p "$pkg/DEBIAN"
cat > "$pkg/DEBIAN/control" <<CONTROL
Package: rowveil
Version: $version
Section: database
Priority: optional
Architecture: amd64
Depends: libxkbcommon0, libxkbcommon-x11-0, libxcb1, libx11-xcb1, libwayland-client0, libfontconfig1, libfreetype6, libvulkan1
Maintainer: Rowveil <99541944+byht1@users.noreply.github.com>
Homepage: https://byht1.github.io/rowveil/
Description: Database explorer and query tool
 Rowveil browses, edits and queries PostgreSQL, MySQL/MariaDB, SQLite, Redis and BigQuery
 databases in a desktop window. It also includes rowveil-server, which
 serves the same UI to a web browser.
CONTROL
dpkg-deb --build --root-owner-group "$pkg" "$dist/rowveil-linux-amd64.deb"

# ---- portable tar.gz
name="rowveil-$version-linux-x86_64"
tmp="$(mktemp -d)/$name"
mkdir -p "$tmp"
cp "$desktop/rowveil" "$bin/rowveil-server" "$tmp/"
cp "$root/packaging/linux/rowveil.desktop" "$tmp/"
cp "$root/assets/icons/rowveil-512.png" "$tmp/rowveil.png"
cp "$root/LICENSE" "$tmp/"
cat > "$tmp/README.txt" <<TXT
Rowveil $version for Linux (x86_64)

  ./rowveil            desktop app
  ./rowveil-server     web server (opens http://127.0.0.1:4983)

The desktop app draws with the GPU (Vulkan) and needs xkbcommon, Wayland or X11, and fontconfig:
  Debian/Ubuntu:  sudo apt install libvulkan1 mesa-vulkan-drivers libxkbcommon-x11-0 libwayland-client0 libfontconfig1
  Fedora:         sudo dnf install vulkan-loader mesa-vulkan-drivers libxkbcommon-x11 wayland fontconfig
  Arch:           sudo pacman -S vulkan-icd-loader libxkbcommon-x11 wayland fontconfig

Your data: ~/.config/rowveil/ (connections.json, saved-queries.json, desktop-prefs.json)
TXT
tar -C "$(dirname "$tmp")" -czf "$dist/rowveil-linux-x86_64.tar.gz" "$name"
cp "$bin/rowveil-server" "$dist/rowveil-server-linux-x86_64"
ls -la "$dist"
