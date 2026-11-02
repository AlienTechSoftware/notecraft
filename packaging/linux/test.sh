#!/usr/bin/env bash
# Smoke-test the dependency-light Linux tarball packaging path.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="$(sed -n 's/^version = "\([0-9][0-9.]*\(-[^\"]*\)\?\)"$/\1/p' "$ROOT/Cargo.toml" | head -n 1)"
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
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/applications/com.alientechsoftware.notecraft.desktop" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/icons/hicolor/256x256/apps/com.alientechsoftware.notecraft.png" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/notecraft/demo/DemoNotebook/html/00-start/welcome/index.html" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/notecraft/demo/DemoNotebook/cmgfm/00-start/welcome/index.md" >/dev/null
echo "linux packaging smoke test passed: $ARCHIVE"

if command -v nfpm >/dev/null 2>&1 && command -v dpkg-deb >/dev/null 2>&1; then
    PKG_DIST="$(mktemp -d)"
    trap 'rm -rf "$DIST" "$PKG_DIST"' EXIT
    DIST="$PKG_DIST" "$ROOT/packaging/linux/package.sh" --skip-build --formats deb
    dpkg-deb -c "$PKG_DIST/notecraft-$VERSION-linux-$ARCH.deb" |
        grep -F "/usr/share/notecraft/demo/DemoNotebook/html/00-start/welcome/index.html" >/dev/null
    echo "debian package demo check passed"
fi
