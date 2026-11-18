#!/bin/sh
# Builds the Windows release: launcher exe (MSVC, WebView2 loader linked in),
# packs it into the setup and builds Mooncrest-Launcher-Setup-<version>.exe.
#
# On Windows (Git Bash) cargo builds MSVC natively; on Linux it cross-compiles
# with cargo-xwin (cargo install cargo-xwin; needs clang + lld).
set -e
cd "$(dirname "$0")/.."
ROOT=$(pwd)
TARGET=x86_64-pc-windows-msvc
VERSION=$(node -p "require('./package.json').version")
RUNNER=""
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ;; *) RUNNER="--runner cargo-xwin" ;; esac

echo "== 1/3 launcher"
npx tauri build $RUNNER --target $TARGET --no-bundle

echo "== 2/3 payload"
mkdir -p setup/payload
rm -f setup/payload/payload.zip
python3 - "$ROOT/src-tauri/target/$TARGET/release/mooncrest-launcher.exe" "$ROOT/setup/payload/payload.zip" <<'PY'
import sys, zipfile
src, dst = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    z.write(src, "mooncrest-launcher.exe")
PY

echo "== 3/3 setup"
(cd setup && "$ROOT/node_modules/.bin/tauri" build $RUNNER --target $TARGET --no-bundle)
mkdir -p release
cp "setup/src-tauri/target/$TARGET/release/mooncrest-setup.exe" "release/Mooncrest-Launcher-Setup-$VERSION.exe"
ls -la release
