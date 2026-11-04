#!/usr/bin/env bash
# Builds Rowveil.app (universal: Apple Silicon + Intel) and a .dmg.
# Run on macOS after:
#   cargo build --release --target aarch64-apple-darwin
#   cargo build --release --target x86_64-apple-darwin
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
dist="$root/dist"
work="$(mktemp -d)"
mkdir -p "$dist"

lipo -create -output "$work/rowveil" \
  "$root/target/aarch64-apple-darwin/release/rowveil" "$root/target/x86_64-apple-darwin/release/rowveil"
lipo -create -output "$dist/rowveil-server-macos-universal" \
  "$root/target/aarch64-apple-darwin/release/rowveil-server" "$root/target/x86_64-apple-darwin/release/rowveil-server"

# ---- icon
iconset="$work/rowveil.iconset"
mkdir -p "$iconset"
src="$root/assets/icons/rowveil-1024.png"
for s in 16 32 128 256 512; do
  sips -z $s $s "$src" --out "$iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$src" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$work/rowveil.icns"

# ---- app bundle
app="$work/Rowveil.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$work/rowveil" "$app/Contents/MacOS/rowveil"
cp "$work/rowveil.icns" "$app/Contents/Resources/rowveil.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Rowveil</string>
  <key>CFBundleDisplayName</key><string>Rowveil</string>
  <key>CFBundleIdentifier</key><string>io.github.byht1.rowveil</string>
  <key>CFBundleExecutable</key><string>rowveil</string>
  <key>CFBundleIconFile</key><string>rowveil</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>MIT License</string>
</dict>
</plist>
PLIST
# Ad-hoc signature: required for Apple Silicon to start the binary (not a Developer ID).
codesign --force --deep --sign - "$app"

# ---- dmg with a shortcut to /Applications
stage="$work/dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "Rowveil $version" -srcfolder "$stage" -ov -format UDZO "$dist/Rowveil-macOS-universal.dmg"
ls -la "$dist"
