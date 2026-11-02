#!/usr/bin/env bash
# Verify the staged package, mounted DMG and extracted CLI without installing either.
# Usage: packaging/macos/verify.sh [aarch64|x86_64|universal]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
for tool in cargo jq shasum; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 1; }
done
VERSION="$(cd "$ROOT" && cargo metadata --no-deps --locked --format-version 1 | jq -er '[.packages[] | select(.name == "notecraft" or .name == "notecraft-cli") | .version] | if length == 2 and (unique | length) == 1 then .[0] else error("GUI and CLI Cargo versions differ or are missing") end')"
sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
check_version() {
  local actual
  actual="$("$1" --version)" || { echo "cannot read executable version: $1" >&2; exit 1; }
  [[ "$actual" = "$2 $VERSION" ]] || { echo "stale executable: $1 reports '$actual'; expected '$2 $VERSION'" >&2; exit 1; }
}
check_provenance() {
  local manifest="$1" binary="$2" expected hash
  [[ -f "$manifest" ]] || { echo "missing package provenance: $manifest" >&2; exit 1; }
  hash="$(sha256 "$binary")"
  expected="$(printf '%s\n%s\n%s' "$VERSION" "$ARCH" "$hash")"
  [[ "$(cat "$manifest")" = "$expected" ]] || { echo "package provenance mismatch: $binary" >&2; exit 1; }
}
ARCH="${1:-aarch64}"
case "$ARCH" in
  aarch64) EXPECTED="arm64" ;;
  x86_64) EXPECTED="x86_64" ;;
  universal) EXPECTED="arm64 x86_64" ;;
  *) echo "unsupported architecture: $ARCH" >&2; exit 2 ;;
esac
DIST="${DIST:-$ROOT/dist/macos}"
DMG="$DIST/notecraft-$VERSION-macos-$ARCH.dmg"
ZIP="$DIST/notecraft-cli-$VERSION-macos-$ARCH.zip"
[[ -f "$DMG" && -f "$ZIP" ]] || { echo "package artifacts are missing" >&2; exit 1; }
WORK="$(mktemp -d "${TMPDIR:-/tmp}/notecraft-verify.XXXXXX")"
MOUNT="$WORK/mount"
mkdir -p "$MOUNT"
mounted=0
finish() {
  local status=$?
  trap - EXIT
  if [[ "$mounted" = 1 ]]; then
    if ! hdiutil detach "$MOUNT" >/dev/null; then
      echo "cannot detach $MOUNT; temporary directory retained" >&2
      exit 1
    fi
  fi
  rm -rf -- "$WORK" || status=1
  exit "$status"
}
trap finish EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

hdiutil verify "$DMG"
codesign --verify --strict --verbose=2 "$DMG"
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT" "$DMG" >/dev/null
mounted=1
[[ "$(readlink "$MOUNT/Applications")" = /Applications ]]
APP="$MOUNT/NoteCraft.app"
[[ -x "$APP/Contents/MacOS/NoteCraft" ]]
[[ -s "$APP/Contents/Resources/NoteCraft.icns" ]]
for doc in LICENSE-MIT LICENSE-APACHE NOTICE ATTRIBUTION.md licenses/PDF-RENDERING-NOTICES.txt; do
  [[ -s "$APP/Contents/Resources/$doc" ]] || { echo "app notice missing: $doc" >&2; exit 1; }
done
plutil -lint "$APP/Contents/Info.plist"
[[ "$(plutil -extract CFBundleIdentifier raw -o - "$APP/Contents/Info.plist")" = com.alientechsoftware.notecraft ]]
[[ "$(plutil -extract CFBundleShortVersionString raw -o - "$APP/Contents/Info.plist")" = "${VERSION%%-*}" ]]
[[ "$(plutil -extract NoteCraftVersion raw -o - "$APP/Contents/Info.plist")" = "$VERSION" ]]
codesign --verify --strict --deep --verbose=2 "$APP"
check_provenance "$MOUNT/BuildProvenance.txt" "$APP/Contents/MacOS/NoteCraft"
for arch in $EXPECTED; do
  [[ " $(lipo -archs "$APP/Contents/MacOS/NoteCraft") " = *" $arch "* ]]
done
ditto "$APP" "$WORK/NoteCraft.app"
codesign --verify --strict --deep --verbose=2 "$WORK/NoteCraft.app"
check_provenance "$MOUNT/BuildProvenance.txt" "$WORK/NoteCraft.app/Contents/MacOS/NoteCraft"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$WORK/NoteCraft.app/Contents/MacOS/NoteCraft" notecraft
else
  echo "skipping $ARCH execution on $(uname -m)"
fi
hdiutil detach "$MOUNT" >/dev/null
mounted=0

ditto -x -k "$ZIP" "$WORK/extracted"
CLI="$WORK/extracted/notecraft-cli-$VERSION-macos-$ARCH/notecraft-cli"
[[ -x "$CLI" ]]
for arch in $EXPECTED; do
  [[ " $(lipo -archs "$CLI") " = *" $arch "* ]]
done
codesign --verify --strict --verbose=2 "$CLI"
check_provenance "${CLI%/*}/BuildProvenance.txt" "$CLI"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$CLI" notecraft-cli
fi
for doc in LICENSE-MIT LICENSE-APACHE NOTICE ATTRIBUTION.md licenses/PDF-RENDERING-NOTICES.txt; do
  [[ -s "${CLI%/*}/$doc" ]] || { echo "CLI notice missing: $doc" >&2; exit 1; }
done
echo "verified macOS $ARCH package: $DMG and $ZIP"
