#!/usr/bin/env bash
# Builds QueryNest.app (universal: Apple Silicon + Intel) and a .dmg.
# Run on macOS after:
#   cargo build --release --target aarch64-apple-darwin
#   cargo build --release --target x86_64-apple-darwin
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
dist="$root/dist"
work="$(mktemp -d)"
mkdir -p "$dist"

lipo -create -output "$work/querynest" \
  "$root/target/aarch64-apple-darwin/release/querynest" "$root/target/x86_64-apple-darwin/release/querynest"
lipo -create -output "$dist/querynest-server-macos-universal" \
  "$root/target/aarch64-apple-darwin/release/querynest-server" "$root/target/x86_64-apple-darwin/release/querynest-server"

# ---- icon
iconset="$work/querynest.iconset"
mkdir -p "$iconset"
src="$root/assets/icons/querynest-1024.png"
for s in 16 32 128 256 512; do
  sips -z $s $s "$src" --out "$iconset/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$src" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$work/querynest.icns"

# ---- app bundle
app="$work/QueryNest.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$work/querynest" "$app/Contents/MacOS/querynest"
cp "$work/querynest.icns" "$app/Contents/Resources/querynest.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>QueryNest</string>
  <key>CFBundleDisplayName</key><string>QueryNest</string>
  <key>CFBundleIdentifier</key><string>io.github.byht1.querynest</string>
  <key>CFBundleExecutable</key><string>querynest</string>
  <key>CFBundleIconFile</key><string>querynest</string>
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
hdiutil create -volname "QueryNest $version" -srcfolder "$stage" -ov -format UDZO "$dist/QueryNest-macOS-universal.dmg"
ls -la "$dist"
