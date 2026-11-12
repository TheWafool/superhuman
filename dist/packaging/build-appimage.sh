#!/bin/sh
# Builds an AppImage on Linux. Needs dist/superman.png (256x256 or larger) and linuxdeploy on PATH.
set -eu
cd "$(dirname "$0")/../.."
[ -f dist/superman.png ] || { echo "put the app icon at dist/superman.png first"; exit 1; }
cargo build --release --locked
rm -rf target/AppDir
install -Dm755 target/release/superman-frontend target/AppDir/usr/bin/superman-frontend
install -Dm644 dist/superman.png target/AppDir/usr/share/icons/hicolor/256x256/apps/superman.png
linuxdeploy --appdir target/AppDir -d dist/superman.desktop -i dist/superman.png --output appimage
