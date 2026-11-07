#!/usr/bin/env bash
# Smoke-test Linux package staging and both generated Welcome notebook formats.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="$(tr -d '\r' < "$ROOT/Cargo.toml" | sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"$/\1/p' | head -n 1)"
[[ -n "$VERSION" ]] || { echo "could not read workspace package version from Cargo.toml" >&2; exit 1; }
DIST="$(mktemp -d)"
trap 'rm -rf "$DIST"' EXIT

bash -n "$ROOT/packaging/linux/package.sh" "$ROOT/packaging/linux/AppRun"
DIST="$DIST" "$ROOT/packaging/linux/package.sh" --formats tar
ARCH="$(uname -m)"
[ "$ARCH" = x86_64 ] && ARCH=x86_64
[ "$ARCH" = aarch64 ] || [ "$ARCH" = x86_64 ] || { echo "unsupported test architecture: $ARCH" >&2; exit 2; }
ARCHIVE="$DIST/notecraft-$VERSION-linux-$ARCH.tar.gz"
[ -s "$ARCHIVE" ] || { echo "missing package: $ARCHIVE" >&2; exit 1; }
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/bin/notecraft" >/dev/null
for path in \
    "share/notecraft/demo/welcome-to-notecraft/html/_node.toml" \
    "share/notecraft/demo/welcome-to-notecraft/html/00-start/welcome/index.html" \
    "share/notecraft/demo/welcome-to-notecraft/cmgfm/_node.toml" \
    "share/notecraft/demo/welcome-to-notecraft/cmgfm/00-start/welcome/index.md"; do
    tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/$path" >/dev/null
done
for format in html cmgfm; do
    count=$(tar -tzf "$ARCHIVE" | awk -F/ -v base="notecraft-$VERSION-linux-$ARCH" -v format="$format" \
        '$1 == base && $2 == "share" && $3 == "notecraft" && $4 == "demo" && $5 == "welcome-to-notecraft" && $6 == format && $7 ~ /^(1[0-9]|2[0-9])-[a-z][a-z]$/ { found[$7] = 1 } END { print length(found) }')
    [ "$count" -eq 13 ] || { echo "$format package contains $count language folders, expected 13" >&2; exit 1; }
done
echo "linux paired-demo packaging smoke test passed: $ARCHIVE"

if command -v nfpm >/dev/null 2>&1 && command -v dpkg-deb >/dev/null 2>&1; then
    PKG_DIST="$(mktemp -d)"
    trap 'rm -rf "$DIST" "$PKG_DIST"' EXIT
    DIST="$PKG_DIST" "$ROOT/packaging/linux/package.sh" --skip-build --formats deb
    DEB="$PKG_DIST/notecraft-$VERSION-linux-$ARCH.deb"
    dpkg-deb -c "$DEB" | grep -F "/usr/share/notecraft/demo/welcome-to-notecraft/html/00-start/welcome/index.html" >/dev/null
    dpkg-deb -c "$DEB" | grep -F "/usr/share/notecraft/demo/welcome-to-notecraft/cmgfm/00-start/welcome/index.md" >/dev/null
    echo "debian paired-demo package checks passed"
fi
