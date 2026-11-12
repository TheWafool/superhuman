#!/bin/sh
# Builds an AppImage on Linux. Needs linuxdeploy on PATH.
set -eu
cd "$(dirname "$0")/../.."
cargo build --release --locked
rm -rf target/AppDir
install -Dm755 target/release/superhuman target/AppDir/usr/bin/superhuman
install -Dm644 dist/superhuman.png target/AppDir/usr/share/icons/hicolor/512x512/apps/superhuman.png
linuxdeploy --appdir target/AppDir -d dist/superhuman.desktop -i dist/superhuman.png --output appimage
