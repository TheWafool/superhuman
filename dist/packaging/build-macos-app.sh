#!/bin/sh
# Builds Superhuman.app and Superhuman.dmg on a Mac. Optional: SIGN_ID="Developer ID Application: …"
# signs it; NOTARY_PROFILE=<notarytool keychain profile> notarizes it.
set -eu
cd "$(dirname "$0")/../.."
cargo build --release --locked
APP=target/Superhuman.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/superhuman "$APP/Contents/MacOS/"
# Icon: dist/superhuman.png → superhuman.icns
SET=target/superhuman.iconset; rm -rf "$SET"; mkdir -p "$SET"
for n in 16 32 128 256 512; do
  sips -z $n $n dist/superhuman.png --out "$SET/icon_${n}x${n}.png" >/dev/null
  sips -z $((n*2)) $((n*2)) dist/superhuman.png --out "$SET/icon_${n}x${n}@2x.png" >/dev/null
done
iconutil -c icns "$SET" -o "$APP/Contents/Resources/superhuman.icns"
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Contents/Info.plist" <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Superhuman</string>
<key>CFBundleIdentifier</key><string>org.superhuman.frontend</string>
<key>CFBundleExecutable</key><string>superhuman</string>
<key>CFBundleIconFile</key><string>superhuman</string>
<key>CFBundleShortVersionString</key><string>$VER</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSApplicationCategoryType</key><string>public.app-category.games</string>
</dict></plist>
P
if [ -n "${SIGN_ID:-}" ]; then codesign --force --options runtime --timestamp --sign "$SIGN_ID" "$APP"; fi
rm -f target/Superhuman.dmg
hdiutil create -volname Superhuman -srcfolder "$APP" -ov -format UDZO target/Superhuman.dmg
if [ -n "${SIGN_ID:-}" ] && [ -n "${NOTARY_PROFILE:-}" ]; then
  xcrun notarytool submit target/Superhuman.dmg --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple target/Superhuman.dmg
fi
echo "target/Superhuman.dmg"
