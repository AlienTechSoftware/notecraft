#!/usr/bin/env bash
# Build a NoteCraft.app, a separate notebook setup app, DMG and CLI archive on macOS.
# Default: universal. Options: --arch aarch64|x86_64|universal, --out DIR, --no-build, --voice.
# CARGO_TARGET_DIR and DIST may override the build and distribution directories.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ARCH=universal
OUT="${DIST:-$ROOT/dist/macos}"
BUILD=1
VOICE_FEATURE=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) [[ $# -ge 2 ]] || { echo "--arch needs a value" >&2; exit 2; }; ARCH="$2"; shift 2 ;;
    --out) [[ $# -ge 2 ]] || { echo "--out needs a value" >&2; exit 2; }; OUT="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    --voice) VOICE_FEATURE=(--features notecraft/voice); shift ;;
    -h|--help) sed -n '2,4p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

case "$ARCH" in
  aarch64) targets=(aarch64-apple-darwin) ;;
  x86_64) targets=(x86_64-apple-darwin) ;;
  universal) targets=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  *) echo "unsupported architecture: $ARCH" >&2; exit 2 ;;
esac
[[ "$(uname -s)" = Darwin ]] || { echo "macOS is required" >&2; exit 1; }
for tool in cargo git jq shasum lipo iconutil sips plutil codesign ditto hdiutil; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 1; }
done
if [[ "$BUILD" = 1 ]]; then
  command -v rustup >/dev/null || { echo "missing tool: rustup" >&2; exit 1; }
fi
VERSION="$(cd "$ROOT" && cargo metadata --no-deps --locked --format-version 1 | jq -er '[.packages[] | select(.name == "notecraft" or .name == "notecraft-cli") | .version] | if length == 2 and (unique | length) == 1 then .[0] else error("GUI and CLI Cargo versions differ or are missing") end')"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "invalid Cargo version: $VERSION" >&2; exit 1; }
TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
mkdir -p "$TARGET_ROOT/packaging" "$OUT"
TARGET_ROOT="$(cd "$TARGET_ROOT" && pwd)"
OUT="$(cd "$OUT" && pwd)"
DMG="$OUT/NoteCraft-$VERSION-macos-$ARCH.dmg"
CLI_ZIP="$OUT/NoteCraft-cli-$VERSION-macos-$ARCH.zip"
export MACOSX_DEPLOYMENT_TARGET=11.0

sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
check_version() {
  local actual
  actual="$("$1" --version)" || { echo "cannot read executable version: $1" >&2; exit 1; }
  [[ "$actual" = "$2 $VERSION" ]] || { echo "stale executable: $1 reports '$actual'; expected '$2 $VERSION'" >&2; exit 1; }
}
native_target() {
  [[ ( "$1" = aarch64-apple-darwin && "$(uname -m)" = arm64 ) || \
     ( "$1" = x86_64-apple-darwin && "$(uname -m)" = x86_64 ) ]]
}

source "$ROOT/packaging/OSX/provenance.sh"
build_source="$(source_provenance "$ROOT")"
echo "==> NoteCraft $VERSION ($ARCH)"
if [[ "$BUILD" = 1 ]]; then
  rustup target add "${targets[@]}"
  for target in "${targets[@]}"; do
    (cd "$ROOT" && CARGO_TARGET_DIR="$TARGET_ROOT" cargo build --locked --release --target "$target" -p notecraft -p notecraft-cli "${VOICE_FEATURE[@]}")
  done
fi
# Both slices must be built from the same inputs, including local uncommitted work.
check_source_provenance "$build_source" "$(source_provenance "$ROOT")" 0

WORK="$(mktemp -d "$TARGET_ROOT/packaging/notecraft-osx.XXXXXX")"
RECORD_TMP=""
cleanup() {
  local status=$?
  trap - EXIT
  if [[ -n "$RECORD_TMP" ]]; then rm -f -- "$RECORD_TMP" || status=1; fi
  rm -rf -- "$WORK" || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
APP="$WORK/NoteCraft.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/licenses"
mkdir -p "$APP/Contents/Resources/demo"
ditto "$ROOT/docs/demo/DemoNotebook" "$APP/Contents/Resources/demo/welcome-to-notecraft"
for entry in html/_node.toml html/00-start/welcome/index.html cmgfm/_node.toml cmgfm/00-start/welcome/index.md; do
  [[ -s "$APP/Contents/Resources/demo/welcome-to-notecraft/$entry" ]] || { echo "demo resource is missing $entry" >&2; exit 1; }
done
for format in html cmgfm; do
  languages=0
  for folder in "$APP/Contents/Resources/demo/welcome-to-notecraft/$format"/*; do
    [[ -d "$folder" ]] || continue
    name="${folder##*/}"
    if [[ "$name" =~ ^(1[0-9]|2[0-9])-[a-z]{2}$ ]]; then languages=$((languages + 1)); fi
  done
  [[ "$languages" = 13 ]] || { echo "$format demo has $languages language folders, expected 13" >&2; exit 1; }
done

# --no-build accepts only binaries recorded by a successful package build. The record
# binds both executable hashes to the Cargo version and source identity, including foreign slices.
for target in "${targets[@]}"; do
  gui="$TARGET_ROOT/$target/release/notecraft"
  cli="$TARGET_ROOT/$target/release/notecraft-cli"
  record="$TARGET_ROOT/$target/release/.notecraft-osx-provenance"
  [[ -x "$gui" && -x "$cli" ]] || { echo "missing executable for $target" >&2; exit 1; }
  expected_arch="${target%%-*}"
  [[ "$expected_arch" = aarch64 ]] && expected_arch=arm64
  for binary in "$gui" "$cli"; do
    [[ "$(lipo -archs "$binary")" = "$expected_arch" ]] || { echo "wrong architecture: $binary" >&2; exit 1; }
  done
  gui_hash="$(sha256 "$gui")"
  cli_hash="$(sha256 "$cli")"
  expected_record="$(printf '%s\n%s\n%s\n%s' "$VERSION" "$target" "$gui_hash" "$cli_hash")"
  if [[ "$BUILD" = 0 ]]; then
    [[ -f "$record" && ! -L "$record" ]] || { echo "missing build provenance for $target; build this target with package.sh first" >&2; exit 1; }
    [[ "$(head -n 4 "$record")" = "$expected_record" ]] || { echo "stale or changed executable for $target; rebuild without --no-build" >&2; exit 1; }
    check_source_provenance "$(tail -n +5 "$record")" "$build_source" 1
  else
    if native_target "$target"; then
      check_version "$gui" notecraft
      check_version "$cli" notecraft-cli
    fi
    [[ ! -L "$record" && ( ! -e "$record" || -f "$record" ) ]] || { echo "unsafe provenance path: $record" >&2; exit 1; }
    RECORD_TMP="$(mktemp "$TARGET_ROOT/$target/release/.notecraft-osx-provenance.XXXXXX")"
    printf '%s\n%s\n' "$expected_record" "$build_source" > "$RECORD_TMP"
    mv -f "$RECORD_TMP" "$record"
    RECORD_TMP=""
  fi
done

for binary in notecraft notecraft-cli; do
  slices=()
  for target in "${targets[@]}"; do
    candidate="$TARGET_ROOT/$target/release/$binary"
    [ -x "$candidate" ] || { echo "missing executable: $candidate" >&2; exit 1; }
    slices+=("$candidate")
  done
  if [[ "$ARCH" = universal ]]; then
    lipo -create "${slices[@]}" -output "$APP/Contents/MacOS/$binary"
  else
    cp "${slices[0]}" "$APP/Contents/MacOS/$binary"
  fi
  chmod 755 "$APP/Contents/MacOS/$binary"
  lipo -info "$APP/Contents/MacOS/$binary"
done

build_number="$(git -C "$ROOT" rev-list --count HEAD)"
build_sha="$(printf '%s\n' "$build_source" | head -n 1)"
if [[ "$(printf '%s\n' "$build_source" | sed -n '2p')" = dirty ]]; then build_sha="$build_sha-dirty"; fi
sed -e "s|@VERSION@|${VERSION%%[-+]*}|g" -e "s|@FULL_VERSION@|$VERSION|g" \
  -e "s|@BUILD@|$build_number|g" -e "s|@BUILD_SHA@|$build_sha|g" \
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
  install -m 644 "$ROOT/$file" "$APP/Contents/Resources/licenses/$file"
done
install -m 644 "$ROOT/ATTRIBUTION.md" "$APP/Contents/Resources/ATTRIBUTION.md"
ditto "$ROOT/licenses" "$APP/Contents/Resources/licenses"

identity="${APPLE_SIGNING_IDENTITY:-}"
sign() {
  if [[ -n "$identity" ]]; then
    if [[ "$1" = *.dmg ]]; then
      codesign --force --timestamp --sign "$identity" "$1"
    else
      codesign --force --timestamp --options runtime --entitlements "$ROOT/packaging/OSX/entitlements.plist" --sign "$identity" "$1"
    fi
  else
    codesign --force --timestamp=none --sign - "$1"
  fi
}
sign "$APP/Contents/MacOS/notecraft-cli"
sign "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"
codesign --verify --strict --verbose=2 "$APP/Contents/MacOS/notecraft-cli"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$APP/Contents/MacOS/notecraft" notecraft
  check_version "$APP/Contents/MacOS/notecraft-cli" notecraft-cli
else
  echo "==> skipping $ARCH execution on $(uname -m); verify on a native host"
fi

DMG_ROOT="$WORK/dmg"
mkdir -p "$DMG_ROOT"
ditto "$APP" "$DMG_ROOT/NoteCraft.app"
install -m 644 "$ROOT/packaging/OSX/DMG Read Me.txt" "$DMG_ROOT/Read Me First.txt"
app_hash="$(sha256 "$DMG_ROOT/NoteCraft.app/Contents/MacOS/notecraft")"
printf '%s\n%s\n%s\n%s\n' "$VERSION" "$ARCH" "$app_hash" "$build_source" > "$DMG_ROOT/BuildProvenance.txt"
ln -s /Applications "$DMG_ROOT/Applications"
hdiutil create -volname "NoteCraft $VERSION" -srcfolder "$DMG_ROOT" -ov -format UDZO "$DMG"
sign "$DMG"
codesign --verify --strict --verbose=2 "$DMG"

CLI_DIR="$WORK/NoteCraft-cli-$VERSION-macos-$ARCH"
mkdir -p "$CLI_DIR/licenses"
install -m 755 "$APP/Contents/MacOS/notecraft-cli" "$CLI_DIR/notecraft-cli"
for file in LICENSE-MIT LICENSE-APACHE NOTICE; do
  install -m 644 "$ROOT/$file" "$CLI_DIR/licenses/$file"
done
install -m 644 "$ROOT/ATTRIBUTION.md" "$CLI_DIR/ATTRIBUTION.md"
ditto "$ROOT/licenses" "$CLI_DIR/licenses"
sign "$CLI_DIR/notecraft-cli"
cli_hash="$(sha256 "$CLI_DIR/notecraft-cli")"
printf '%s\n%s\n%s\n%s\n' "$VERSION" "$ARCH" "$cli_hash" "$build_source" > "$CLI_DIR/BuildProvenance.txt"
codesign --verify --strict --verbose=2 "$CLI_DIR/notecraft-cli"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$CLI_DIR/notecraft-cli" notecraft-cli
fi
ditto -c -k --keepParent "$CLI_DIR" "$CLI_ZIP"
check_source_provenance "$build_source" "$(source_provenance "$ROOT")" 0

if [[ -n "$identity" ]]; then
  if [[ -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
    xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$DMG"
  else
    echo "warning: signed but not notarized; Apple credentials are unset" >&2
  fi
else
  echo "warning: ad-hoc signed test packages; public distribution requires Developer ID and notarization" >&2
fi
echo "wrote $DMG and $CLI_ZIP"
