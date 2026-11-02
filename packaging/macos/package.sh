#!/usr/bin/env bash
# Build a standalone NoteCraft.app, drag-to-Applications DMG and separate CLI archive.
# Usage: packaging/macos/package.sh [--arch aarch64|x86_64|universal] [--skip-build]
# DIST and CARGO_TARGET_DIR may override the output and build directories.
# MACOS_SIGN_IDENTITY defaults to "-" (ad-hoc); real Developer ID signing is optional.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HERE="$ROOT/packaging/macos"
ARCH=aarch64
SKIP_BUILD=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) [[ $# -ge 2 ]] || { echo "--arch needs a value" >&2; exit 2; }; ARCH="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$ARCH" in
  aarch64) TARGETS=(aarch64-apple-darwin) ;;
  x86_64) TARGETS=(x86_64-apple-darwin) ;;
  universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  *) echo "unsupported architecture: $ARCH" >&2; exit 2 ;;
esac
[[ "$(uname -s)" = Darwin ]] || { echo "macOS is required" >&2; exit 1; }
for tool in cargo jq shasum lipo iconutil sips plutil codesign ditto hdiutil; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 1; }
done
VERSION="$(cd "$ROOT" && cargo metadata --no-deps --locked --format-version 1 | jq -er '[.packages[] | select(.name == "notecraft" or .name == "notecraft-cli") | .version] | if length == 2 and (unique | length) == 1 then .[0] else error("GUI and CLI Cargo versions differ or are missing") end')"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "invalid Cargo version: $VERSION" >&2; exit 1; }
SHORT_VERSION="${VERSION%%-*}"
BUILD_SHA="$(git -C "$ROOT" rev-parse --short=12 HEAD)"

sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
check_version() {
  local actual
  actual="$("$1" --version)" || { echo "cannot read executable version: $1" >&2; exit 1; }
  [[ "$actual" = "$2 $VERSION" ]] || { echo "stale executable: $1 reports '$actual'; expected '$2 $VERSION'" >&2; exit 1; }
}

TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
DIST="${DIST:-$ROOT/dist/macos}"
mkdir -p "$TARGET_ROOT" "$DIST"
TARGET_ROOT="$(cd "$TARGET_ROOT" && pwd)"
DIST="$(cd "$DIST" && pwd)"
WORK="$(mktemp -d "$TARGET_ROOT/notecraft-macos.XXXXXX")"
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
DMG="$DIST/notecraft-$VERSION-macos-$ARCH.dmg"
CLI_ZIP="$DIST/notecraft-cli-$VERSION-macos-$ARCH.zip"
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
export MACOSX_DEPLOYMENT_TARGET=11.0

echo "==> NoteCraft $VERSION ($ARCH); work: $WORK"
if [[ "$SKIP_BUILD" = 0 ]]; then
  for target in "${TARGETS[@]}"; do
    (cd "$ROOT" && CARGO_TARGET_DIR="$TARGET_ROOT" cargo build --release --locked --target "$target" -p notecraft -p notecraft-cli)
  done
fi

# A skip-build package must use the exact binaries produced by a prior package build.
# The record binds both binaries, their target and the Cargo version; it is never inferred
# from strings found inside a non-native executable.
for target in "${TARGETS[@]}"; do
  gui="$TARGET_ROOT/$target/release/notecraft"
  cli="$TARGET_ROOT/$target/release/notecraft-cli"
  record="$TARGET_ROOT/$target/release/.notecraft-macos-provenance"
  [[ -x "$gui" && -x "$cli" ]] || { echo "missing executable for $target" >&2; exit 1; }
  gui_hash="$(sha256 "$gui")"
  cli_hash="$(sha256 "$cli")"
  expected_record="$(printf '%s\n%s\n%s\n%s' "$VERSION" "$target" "$gui_hash" "$cli_hash")"
  if [[ "$SKIP_BUILD" = 1 ]]; then
    [[ -f "$record" ]] || { echo "missing build provenance for $target; run package.sh without --skip-build first" >&2; exit 1; }
    [[ "$(cat "$record")" = "$expected_record" ]] || { echo "stale or changed executable for $target: build provenance does not match; rebuild without --skip-build" >&2; exit 1; }
  else
    if [[ ( "$target" = aarch64-apple-darwin && "$(uname -m)" = arm64 ) || \
          ( "$target" = x86_64-apple-darwin && "$(uname -m)" = x86_64 ) ]]; then
      check_version "$gui" notecraft
      check_version "$cli" notecraft-cli
    fi
    [[ ! -L "$record" && ( ! -e "$record" || -f "$record" ) ]] || { echo "unsafe provenance path: $record" >&2; exit 1; }
    RECORD_TMP="$(mktemp "$TARGET_ROOT/$target/release/.notecraft-macos-provenance.XXXXXX")"
    printf '%s\n' "$expected_record" > "$RECORD_TMP"
    mv -f "$RECORD_TMP" "$record"
    RECORD_TMP=""
  fi
done

mkdir -p "$WORK/bin"
for bin in notecraft notecraft-cli; do
  inputs=()
  for target in "${TARGETS[@]}"; do
    input="$TARGET_ROOT/$target/release/$bin"
    [[ -x "$input" ]] || { echo "missing executable: $input" >&2; exit 1; }
    expected="${target%%-*}"
    [[ "$expected" = aarch64 ]] && expected=arm64
    [[ " $(lipo -archs "$input") " = *" $expected "* ]] || { echo "wrong architecture: $input" >&2; exit 1; }
    inputs+=("$input")
  done
  if [[ "$ARCH" = universal ]]; then
    lipo -create -output "$WORK/bin/$bin" "${inputs[@]}"
  else
    cp "${inputs[0]}" "$WORK/bin/$bin"
  fi
  echo "==> $bin: $(lipo -archs "$WORK/bin/$bin")"
done

sign() {
  if [[ "$IDENTITY" = - ]]; then
    codesign --force --sign - --timestamp=none "$@"
  else
    codesign --force --sign "$IDENTITY" --options runtime --timestamp "$@"
  fi
}

# iconutil needs the complete iconset. Every size comes from the existing attributed brand PNG.
ICONSET="$WORK/NoteCraft.iconset"
mkdir -p "$ICONSET"
for item in '16 16x16' '32 16x16@2x' '32 32x32' '64 32x32@2x' \
            '128 128x128' '256 128x128@2x' '256 256x256' \
            '512 256x256@2x' '512 512x512' '1024 512x512@2x'; do
  read -r size name <<< "$item"
  sips -z "$size" "$size" "$ROOT/assets/brand/notecraft-512.png" \
    --out "$ICONSET/icon_${name}.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$WORK/NoteCraft.icns"

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 755 "$WORK/bin/notecraft" "$APP/Contents/MacOS/NoteCraft"
install -m 644 "$WORK/NoteCraft.icns" "$APP/Contents/Resources/NoteCraft.icns"
for doc in LICENSE-MIT LICENSE-APACHE NOTICE ATTRIBUTION.md; do
  install -m 644 "$ROOT/$doc" "$APP/Contents/Resources/$doc"
done
ditto "$ROOT/licenses" "$APP/Contents/Resources/licenses"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
  -e "s/@BUILD_SHA@/$BUILD_SHA/g" "$HERE/Info.plist.in" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
plutil -lint "$APP/Contents/Info.plist"
sign "$APP/Contents/MacOS/NoteCraft"
sign "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$APP/Contents/MacOS/NoteCraft" notecraft
else
  echo "==> skipping $ARCH execution on $(uname -m); run verify.sh on a native host"
fi

STAGE="$WORK/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/NoteCraft.app"
app_hash="$(sha256 "$STAGE/NoteCraft.app/Contents/MacOS/NoteCraft")"
printf '%s\n%s\n%s\n' "$VERSION" "$ARCH" "$app_hash" > "$STAGE/BuildProvenance.txt"
ln -s /Applications "$STAGE/Applications"
echo "==> creating $DMG"
hdiutil create -srcfolder "$STAGE" -volname NoteCraft -fs HFS+ -format UDZO \
  -imagekey zlib-level=9 -ov "$DMG"
sign "$DMG"
codesign --verify --strict --verbose=2 "$DMG"

CLI_DIR="$WORK/notecraft-cli-$VERSION-macos-$ARCH"
mkdir -p "$CLI_DIR"
install -m 755 "$WORK/bin/notecraft-cli" "$CLI_DIR/notecraft-cli"
for doc in LICENSE-MIT LICENSE-APACHE NOTICE ATTRIBUTION.md; do
  install -m 644 "$ROOT/$doc" "$CLI_DIR/$doc"
done
ditto "$ROOT/licenses" "$CLI_DIR/licenses"
sign "$CLI_DIR/notecraft-cli"
cli_hash="$(sha256 "$CLI_DIR/notecraft-cli")"
printf '%s\n%s\n%s\n' "$VERSION" "$ARCH" "$cli_hash" > "$CLI_DIR/BuildProvenance.txt"
codesign --verify --strict --verbose=2 "$CLI_DIR/notecraft-cli"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$CLI_DIR/notecraft-cli" notecraft-cli
fi
echo "==> creating $CLI_ZIP"
ditto -c -k --keepParent "$CLI_DIR" "$CLI_ZIP"

echo "==> package ready"
ls -lh "$DMG" "$CLI_ZIP"
if [[ "$IDENTITY" = - ]]; then
  echo "Ad-hoc signed for local testing; public distribution requires Developer ID signing and notarization."
fi
