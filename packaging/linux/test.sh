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
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/bin/notecraft-setup" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/notecraft/demo/welcome-to-notecraft/html/_node.toml" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/notecraft/demo/welcome-to-notecraft/cmgfm/_node.toml" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/notecraft/demo/welcome-to-notecraft/html/00-start/welcome/index.html" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/share/notecraft/demo/welcome-to-notecraft/cmgfm/00-start/welcome/index.md" >/dev/null
echo "linux packaging smoke test passed: $ARCHIVE"

# A package without optional User Assistance files still carries the app and setup helper.
DIST="$DIST" "$ROOT/packaging/linux/package.sh" --skip-build --omit-demo --formats tar
[ -s "$ARCHIVE" ] || { echo "missing no-demo package: $ARCHIVE" >&2; exit 1; }
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/bin/notecraft" >/dev/null
tar -tzf "$ARCHIVE" | grep -Fx "notecraft-$VERSION-linux-$ARCH/bin/notecraft-setup" >/dev/null
if tar -tzf "$ARCHIVE" | grep -F "/share/notecraft/demo/" >/dev/null; then
    echo "omitted User Assistance payload is present in the Linux package" >&2
    exit 1
fi
echo "linux no-demo packaging smoke test passed"

if command -v nfpm >/dev/null 2>&1 && command -v dpkg-deb >/dev/null 2>&1; then
    PKG_DIST="$(mktemp -d)"
    trap 'rm -rf "$DIST" "$PKG_DIST"' EXIT
    DIST="$PKG_DIST" "$ROOT/packaging/linux/package.sh" --skip-build --formats deb
    DEB="$PKG_DIST/notecraft-$VERSION-linux-$ARCH.deb"
    dpkg-deb -c "$DEB" | grep -F "/usr/bin/notecraft-setup" >/dev/null
    dpkg-deb -c "$DEB" |
        grep -F "/usr/share/notecraft/demo/welcome-to-notecraft/html/_node.toml" >/dev/null
    dpkg-deb -c "$DEB" |
        grep -F "/usr/share/notecraft/demo/welcome-to-notecraft/cmgfm/_node.toml" >/dev/null
    dpkg-deb -c "$DEB" |
        grep -F "/usr/share/notecraft/demo/welcome-to-notecraft/html/00-start/welcome/index.html" >/dev/null
    OMIT_DIST="$(mktemp -d)"
    DIST="$OMIT_DIST" "$ROOT/packaging/linux/package.sh" --skip-build --omit-demo --formats deb
    OMIT_DEB="$OMIT_DIST/notecraft-$VERSION-linux-$ARCH.deb"
    dpkg-deb -c "$OMIT_DEB" | grep -F "/usr/bin/notecraft-setup" >/dev/null
    if dpkg-deb -c "$OMIT_DEB" | grep -F "/usr/share/notecraft/demo/" >/dev/null; then
        echo "omitted User Assistance payload is present in the Debian package" >&2
        exit 1
    fi
    echo "debian package setup-helper and optional-payload checks passed"
fi
