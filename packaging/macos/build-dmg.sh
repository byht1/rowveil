#!/usr/bin/env bash
# Builds Roostbase.app (universal: Apple Silicon + Intel) and a .dmg.
# Run on macOS after:
#   cargo build --release --target aarch64-apple-darwin
#   cargo build --release --target x86_64-apple-darwin
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
dist="$root/dist"
work="$(mktemp -d)"
mkdir -p "$dist"

lipo -create -output "$work/roostbase" \
  "$root/target/aarch64-apple-darwin/release/roostbase" "$root/target/x86_64-apple-darwin/release/roostbase"
lipo -create -output "$dist/roostbase-server-macos-universal" \
  "$root/target/aarch64-apple-darwin/release/roostbase-server" "$root/target/x86_64-apple-darwin/release/roostbase-server"

# ---- icon
iconset="$work/roostbase.iconset"
mkdir -p "$iconset"
src="$root/assets/icons/roostbase-1024.png"
for s in 16 32 128 256 512; do
  sips -z $s $s "$src" --out "$iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$src" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$work/roostbase.icns"

# ---- app bundle
app="$work/Roostbase.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$work/roostbase" "$app/Contents/MacOS/roostbase"
cp "$work/roostbase.icns" "$app/Contents/Resources/roostbase.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Roostbase</string>
  <key>CFBundleDisplayName</key><string>Roostbase</string>
  <key>CFBundleIdentifier</key><string>io.github.byht1.roostbase</string>
  <key>CFBundleExecutable</key><string>roostbase</string>
  <key>CFBundleIconFile</key><string>roostbase</string>
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
hdiutil create -volname "Roostbase $version" -srcfolder "$stage" -ov -format UDZO "$dist/Roostbase-macOS-universal.dmg"
ls -la "$dist"
