#!/usr/bin/env bash
# Packages the Linux build: a .deb and a portable .tar.gz.
# Run after: cargo build --release   (needs libwebkit2gtk-4.1-dev and libgtk-3-dev)
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
bin="$root/target/release"
dist="$root/dist"
mkdir -p "$dist"

# ---- .deb
pkg="$(mktemp -d)/rowveil"
install -Dm755 "$bin/rowveil" "$pkg/usr/bin/rowveil"
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
Depends: libwebkit2gtk-4.1-0, libgtk-3-0
Maintainer: Rowveil <99541944+byht1@users.noreply.github.com>
Homepage: https://byht1.github.io/rowveil/
Description: Database explorer and query tool
 Rowveil browses, edits and queries PostgreSQL, MySQL/MariaDB and SQLite
 databases in a desktop window. It also includes rowveil-server, which
 serves the same UI to a web browser.
CONTROL
dpkg-deb --build --root-owner-group "$pkg" "$dist/rowveil-linux-amd64.deb"

# ---- portable tar.gz
name="rowveil-$version-linux-x86_64"
tmp="$(mktemp -d)/$name"
mkdir -p "$tmp"
cp "$bin/rowveil" "$bin/rowveil-server" "$tmp/"
cp "$root/packaging/linux/rowveil.desktop" "$tmp/"
cp "$root/assets/icons/rowveil-512.png" "$tmp/rowveil.png"
cp "$root/LICENSE" "$tmp/"
cat > "$tmp/README.txt" <<TXT
Rowveil $version for Linux (x86_64)

  ./rowveil            desktop app
  ./rowveil-server     web server (opens http://127.0.0.1:4983)

The desktop app needs WebKitGTK 4.1 and GTK 3:
  Debian/Ubuntu:  sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0
  Fedora:         sudo dnf install webkit2gtk4.1 gtk3
  Arch:           sudo pacman -S webkit2gtk-4.1 gtk3

Your data: ~/.config/rowveil/ (connections.json, window.json, rowveil.log, webview/)
TXT
tar -C "$(dirname "$tmp")" -czf "$dist/rowveil-linux-x86_64.tar.gz" "$name"
cp "$bin/rowveil-server" "$dist/rowveil-server-linux-x86_64"
ls -la "$dist"
