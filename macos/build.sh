#!/usr/bin/env bash
# Builds Kswime.app (universal) with no tool beyond Xcode command-line tools
# and rustup.  Run on macOS from the repository root:  macos/build.sh [--install]
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
APP="$ROOT/macos/build/Kswime.app"

# 1. Rust engine, both architectures, merged with lipo
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$t" >/dev/null 2>&1 || true
  cargo build --release --lib --target "$t"
done
mkdir -p target/universal/release
lipo -create target/aarch64-apple-darwin/release/libkswime.a target/x86_64-apple-darwin/release/libkswime.a \
     -output target/universal/release/libkswime.a

# 2. Swift shell
cd macos
swift build -c release --arch arm64 --arch x86_64
BIN=$(swift build -c release --arch arm64 --arch x86_64 --show-bin-path)/KswimeIME

# 3. App bundle
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/KswimeIME"
cp Resources/Info.plist "$APP/Contents/"
cp Resources/*.tiff Resources/NOTICE.txt "$APP/Contents/Resources/"
cp -R Resources/ja.lproj Resources/en.lproj "$APP/Contents/Resources/"
codesign --force --deep --sign - "$APP"
echo "built $APP"

# 4. Install into ~/Library/Input Methods and register without logout
if [[ "${1:-}" == "--install" ]]; then
  DEST="$HOME/Library/Input Methods/Kswime.app"
  pkill -x KswimeIME || true
  rm -rf "$DEST.old"
  [[ -d "$DEST" ]] && mv "$DEST" "$DEST.old"
  cp -R "$APP" "$DEST"
  rm -rf "$DEST.old"
  "$DEST/Contents/MacOS/KswimeIME" --register
  echo "installed. Open System Settings > Keyboard > Input Sources if Kswime is not listed yet."
fi
