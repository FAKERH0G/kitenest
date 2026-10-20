#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
command -v cargo >/dev/null
command -v linuxdeploy >/dev/null
command -v appimagetool >/dev/null

cargo build --manifest-path "$root/Cargo.toml" --locked --release --all-features
appdir="$(mktemp -d "${TMPDIR:-/tmp}/kitenest-appdir.XXXXXX")"
trap 'rm -rf -- "$appdir"' EXIT
mkdir -p "$appdir/usr/bin"
install -m 0755 "$root/target/release/kitenest" "$appdir/usr/bin/kitenest"
install -m 0755 "$root/target/release/kitenest-gui" "$appdir/usr/bin/kitenest-gui"
install -m 0755 "$root/target/release/kitenest-privileged" \
    "$appdir/usr/bin/kitenest-privileged"
sed 's/^Exec=.*/Exec=AppRun/' \
    "$root/data/org.kitenest.PackageManager.desktop" \
    >"$appdir/org.kitenest.PackageManager.desktop"

linuxdeploy \
    --appdir "$appdir" \
    --plugin gtk \
    --executable "$appdir/usr/bin/kitenest-gui" \
    --executable "$appdir/usr/bin/kitenest" \
    --executable "$appdir/usr/bin/kitenest-privileged" \
    --desktop-file "$appdir/org.kitenest.PackageManager.desktop" \
    --icon-file "$root/data/icons/hicolor/scalable/apps/org.kitenest.PackageManager.svg"

architecture="$(uname -m)"
appimagetool "$appdir" "$root/KiteNest-$architecture.AppImage"
