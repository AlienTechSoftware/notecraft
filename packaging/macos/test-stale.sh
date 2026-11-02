#!/usr/bin/env bash
# Negative packaging checks using copies of binaries built by package.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SOURCE_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
ARCH="${1:-aarch64}"
case "$ARCH" in
  aarch64) TARGET=aarch64-apple-darwin ;;
  x86_64) TARGET=x86_64-apple-darwin ;;
  *) echo "test-stale.sh expects aarch64 or x86_64" >&2; exit 2 ;;
esac
SOURCE="$SOURCE_ROOT/$TARGET/release"
for file in notecraft notecraft-cli .notecraft-macos-provenance; do
  [[ -f "$SOURCE/$file" ]] || { echo "run package.sh for $ARCH before test-stale.sh: missing $SOURCE/$file" >&2; exit 1; }
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/notecraft-stale.XXXXXX")"
trap 'rm -rf -- "$WORK"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
DEST="$WORK/target/$TARGET/release"
mkdir -p "$DEST"
cp "$SOURCE/notecraft" "$SOURCE/notecraft-cli" "$SOURCE/.notecraft-macos-provenance" "$DEST/"
RECORD="$DEST/.notecraft-macos-provenance"

expect_rejection() {
  if CARGO_TARGET_DIR="$WORK/target" DIST="$WORK/dist" "$ROOT/packaging/macos/package.sh" --arch "$ARCH" --skip-build > "$WORK/output" 2>&1; then
    echo "stale binary was accepted" >&2
    exit 1
  fi
  if ! grep -Fq 'stale or changed executable' "$WORK/output"; then
    cat "$WORK/output" >&2
    echo "package failed for a reason other than stale executable rejection" >&2
    exit 1
  fi
}

{ printf '0.0.0-stale\n'; tail -n +2 "$SOURCE/.notecraft-macos-provenance"; } > "$RECORD"
expect_rejection
cp "$SOURCE/.notecraft-macos-provenance" "$RECORD"
printf 'changed' >> "$DEST/notecraft"
expect_rejection
echo "stale version and changed binary rejected for $ARCH"
