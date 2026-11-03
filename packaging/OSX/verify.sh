#!/usr/bin/env bash
# Verify the staged package, mounted DMG and extracted CLI without installing either.
# Usage: packaging/OSX/verify.sh [aarch64|x86_64|universal] (DIST may override output)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
for tool in cargo jq shasum; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 1; }
done
VERSION="$(cd "$ROOT" && cargo metadata --no-deps --locked --format-version 1 | jq -er '[.packages[] | select(.name == "notecraft" or .name == "notecraft-cli") | .version] | if length == 2 and (unique | length) == 1 then .[0] else error("GUI and CLI Cargo versions differ or are missing") end')"
source "$ROOT/packaging/OSX/provenance.sh"
build_source="$(source_provenance "$ROOT")"
build_sha="$(printf '%s\n' "$build_source" | head -n 1)"
if [[ "$(printf '%s\n' "$build_source" | sed -n '2p')" = dirty ]]; then build_sha="$build_sha-dirty"; fi
build_number="$(git -C "$ROOT" rev-list --count HEAD)"
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
  expected="$(printf '%s\n%s\n%s\n%s' "$VERSION" "$ARCH" "$hash" "$build_source")"
  [[ "$(cat "$manifest")" = "$expected" ]] || { echo "package provenance mismatch: $binary" >&2; exit 1; }
}
check_architectures() {
  local actual binary="$1" arch
  local -a found
  actual="$(lipo -archs "$binary")"
  read -r -a found <<< "$actual"
  [[ ${#found[@]} -eq "$EXPECTED_COUNT" ]] || { echo "wrong architecture count: $binary ($actual)" >&2; exit 1; }
  for arch in $EXPECTED; do
    [[ " $actual " = *" $arch "* ]] || { echo "missing $arch slice: $binary" >&2; exit 1; }
  done
}
ARCH="${1:-universal}"
case "$ARCH" in
  aarch64) EXPECTED="arm64"; EXPECTED_COUNT=1 ;;
  x86_64) EXPECTED="x86_64"; EXPECTED_COUNT=1 ;;
  universal) EXPECTED="arm64 x86_64"; EXPECTED_COUNT=2 ;;
  *) echo "unsupported architecture: $ARCH" >&2; exit 2 ;;
esac
DIST="${DIST:-$ROOT/dist/macos}"
DMG="$DIST/NoteCraft-$VERSION-macos-$ARCH.dmg"
ZIP="$DIST/NoteCraft-cli-$VERSION-macos-$ARCH.zip"
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
[[ -x "$APP/Contents/MacOS/notecraft" && -x "$APP/Contents/MacOS/notecraft-cli" ]]
[[ -s "$APP/Contents/Resources/notecraft.icns" ]]
for doc in licenses/LICENSE-MIT licenses/LICENSE-APACHE licenses/NOTICE ATTRIBUTION.md licenses/PDF-RENDERING-NOTICES.txt; do
  [[ -s "$APP/Contents/Resources/$doc" ]] || { echo "app notice missing: $doc" >&2; exit 1; }
done
for entry in html/00-start/welcome/index.html cmgfm/00-start/welcome/index.md; do
  [[ -s "$APP/Contents/Resources/demo/DemoNotebook/$entry" ]] || { echo "app demo entry missing: $entry" >&2; exit 1; }
done
# Verify the complete release payload from the mounted artifact, including both formats.
diff -rq "$ROOT/docs/demo/DemoNotebook" "$APP/Contents/Resources/demo/DemoNotebook"
plutil -lint "$APP/Contents/Info.plist"
[[ "$(plutil -extract CFBundleIdentifier raw -o - "$APP/Contents/Info.plist")" = com.alientechsoftware.notecraft ]]
[[ "$(plutil -extract CFBundleExecutable raw -o - "$APP/Contents/Info.plist")" = notecraft ]]
[[ "$(plutil -extract CFBundleShortVersionString raw -o - "$APP/Contents/Info.plist")" = "${VERSION%%-*}" ]]
[[ "$(plutil -extract NoteCraftVersion raw -o - "$APP/Contents/Info.plist")" = "$VERSION" ]]
[[ "$(plutil -extract CFBundleVersion raw -o - "$APP/Contents/Info.plist")" =~ ^[0-9]+$ ]]
[[ "$(plutil -extract CFBundleVersion raw -o - "$APP/Contents/Info.plist")" = "$build_number" ]]
[[ "$(plutil -extract NoteCraftBuildCommit raw -o - "$APP/Contents/Info.plist")" = "$build_sha" ]]
codesign --verify --strict --deep --verbose=2 "$APP"
codesign --verify --strict --verbose=2 "$APP/Contents/MacOS/notecraft-cli"
check_provenance "$MOUNT/BuildProvenance.txt" "$APP/Contents/MacOS/notecraft"
check_architectures "$APP/Contents/MacOS/notecraft"
check_architectures "$APP/Contents/MacOS/notecraft-cli"
ditto "$APP" "$WORK/NoteCraft.app"
codesign --verify --strict --deep --verbose=2 "$WORK/NoteCraft.app"
check_provenance "$MOUNT/BuildProvenance.txt" "$WORK/NoteCraft.app/Contents/MacOS/notecraft"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$WORK/NoteCraft.app/Contents/MacOS/notecraft" notecraft
  check_version "$WORK/NoteCraft.app/Contents/MacOS/notecraft-cli" notecraft-cli
else
  echo "skipping $ARCH execution on $(uname -m)"
fi
hdiutil detach "$MOUNT" >/dev/null
mounted=0

ditto -x -k "$ZIP" "$WORK/extracted"
CLI="$WORK/extracted/NoteCraft-cli-$VERSION-macos-$ARCH/notecraft-cli"
[[ -x "$CLI" ]]
check_architectures "$CLI"
codesign --verify --strict --verbose=2 "$CLI"
check_provenance "${CLI%/*}/BuildProvenance.txt" "$CLI"
if [[ "$ARCH" = universal || ( "$ARCH" = aarch64 && "$(uname -m)" = arm64 ) || \
      ( "$ARCH" = x86_64 && "$(uname -m)" = x86_64 ) ]]; then
  check_version "$CLI" notecraft-cli
fi
for doc in licenses/LICENSE-MIT licenses/LICENSE-APACHE licenses/NOTICE ATTRIBUTION.md licenses/PDF-RENDERING-NOTICES.txt; do
  [[ -s "${CLI%/*}/$doc" ]] || { echo "CLI notice missing: $doc" >&2; exit 1; }
done
echo "verified macOS $ARCH package: $DMG and $ZIP"
