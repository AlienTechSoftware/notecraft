#!/usr/bin/env bash
# Build a universal NoteCraft.app and drag-to-Applications DMG on macOS.
# Signing is optional for local builds; notarization requires Apple credentials.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="$(sed -n '/^\[workspace\.package\]/,/^\[/ { s/^version = "\([^"]*\)"/\1/p; }' "$ROOT/Cargo.toml" | head -n1)"
[ -n "$VERSION" ] || { echo "could not read workspace version" >&2; exit 1; }

OUT="${DIST:-$ROOT/dist/macos}"
BUILD=1
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="${2:?--out needs a value}"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    -h|--help) sed -n '2,6p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

targets=(aarch64-apple-darwin x86_64-apple-darwin)
if [ "$BUILD" -eq 1 ]; then
  rustup target add "${targets[@]}"
  for target in "${targets[@]}"; do
    (cd "$ROOT" && MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --locked --release --target "$target" -p notecraft -p notecraft-cli)
  done
fi

WORK="${CARGO_TARGET_DIR:-$ROOT/target}/packaging/macos"
APP="$WORK/NoteCraft.app"
rm -rf "$WORK"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/licenses" "$OUT"
OUT="$(cd "$OUT" && pwd)"

for binary in notecraft notecraft-cli; do
  slices=()
  for target in "${targets[@]}"; do
    candidate="$ROOT/target/$target/release/$binary"
    [ -x "$candidate" ] || { echo "missing executable: $candidate" >&2; exit 1; }
    slices+=("$candidate")
  done
  lipo -create "${slices[@]}" -output "$APP/Contents/MacOS/$binary"
  lipo -info "$APP/Contents/MacOS/$binary"
done

build_number="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
sed -e "s|@VERSION@|${VERSION%%[-+]*}|g" -e "s|@BUILD@|$build_number|g" \
  "$ROOT/packaging/OSX/Info.plist.in" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

# Build the .icns from the checked-in PNG family; this keeps the source asset portable.
ICONSET="$WORK/notecraft.iconset"
mkdir -p "$ICONSET"
for spec in "16:32" "32:64" "128:256" "256:512" "512:1024"; do
  size="${spec%%:*}"; source_size="${spec##*:}"
  sips -z "$size" "$size" "$ROOT/assets/brand/notecraft-512.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z "$source_size" "$source_size" "$ROOT/assets/brand/notecraft-512.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil --convert icns "$ICONSET" --output "$APP/Contents/Resources/notecraft.icns"

for file in LICENSE-MIT LICENSE-APACHE NOTICE; do
  cp "$ROOT/$file" "$APP/Contents/Resources/licenses/$file"
done

identity="${APPLE_SIGNING_IDENTITY:-}"
if [ -n "$identity" ]; then
  codesign --force --timestamp --options runtime --entitlements "$ROOT/packaging/OSX/entitlements.plist" --sign "$identity" "$APP/Contents/MacOS/notecraft-cli"
  codesign --force --timestamp --options runtime --entitlements "$ROOT/packaging/OSX/entitlements.plist" --sign "$identity" "$APP"
  codesign --verify --strict --verbose=2 "$APP"
else
  echo "warning: APPLE_SIGNING_IDENTITY is unset; creating an ad-hoc signed test app" >&2
  codesign --force --deep --sign - "$APP"
fi

DMG_ROOT="$WORK/dmg"
mkdir -p "$DMG_ROOT"
cp -R "$APP" "$DMG_ROOT/"
ln -s /Applications "$DMG_ROOT/Applications"
DMG="$OUT/NoteCraft-$VERSION-macos-universal.dmg"
rm -f "$DMG"
hdiutil create -volname "NoteCraft $VERSION" -srcfolder "$DMG_ROOT" -ov -format UDZO "$DMG"

if [ -n "$identity" ]; then
  codesign --force --timestamp --sign "$identity" "$DMG"
  if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$DMG"
  else
    echo "warning: signed but not notarized; Apple credentials are unset" >&2
  fi
fi
echo "wrote $DMG"
