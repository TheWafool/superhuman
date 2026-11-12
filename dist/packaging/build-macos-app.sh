#!/bin/sh
# Builds Superman.app and Superman.dmg on a Mac. Optional: SIGN_ID="Developer ID Application: …"
# signs it; NOTARY_PROFILE=<notarytool keychain profile> notarizes it.
set -eu
cd "$(dirname "$0")/../.."
cargo build --release --locked
APP=target/Superman.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/superman-frontend "$APP/Contents/MacOS/"
# Icon: dist/superman.png → superman.icns
SET=target/superman.iconset; rm -rf "$SET"; mkdir -p "$SET"
for n in 16 32 128 256 512; do
  sips -z $n $n dist/superman.png --out "$SET/icon_${n}x${n}.png" >/dev/null
  sips -z $((n*2)) $((n*2)) dist/superman.png --out "$SET/icon_${n}x${n}@2x.png" >/dev/null
done
iconutil -c icns "$SET" -o "$APP/Contents/Resources/superman.icns"
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Contents/Info.plist" <<P
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Superman</string>
<key>CFBundleIdentifier</key><string>org.superman.frontend</string>
<key>CFBundleExecutable</key><string>superman-frontend</string>
<key>CFBundleIconFile</key><string>superman</string>
<key>CFBundleShortVersionString</key><string>$VER</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSApplicationCategoryType</key><string>public.app-category.games</string>
</dict></plist>
P
if [ -n "${SIGN_ID:-}" ]; then codesign --force --options runtime --timestamp --sign "$SIGN_ID" "$APP"; fi
rm -f target/Superman.dmg
hdiutil create -volname Superman -srcfolder "$APP" -ov -format UDZO target/Superman.dmg
if [ -n "${SIGN_ID:-}" ] && [ -n "${NOTARY_PROFILE:-}" ]; then
  xcrun notarytool submit target/Superman.dmg --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple target/Superman.dmg
fi
echo "target/Superman.dmg"
