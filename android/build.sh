#!/usr/bin/env bash
# Builds the NoteCraft Android APK: dist/android/notecraft-<version>-android-arm64.apk
#
#   bash android/build.sh [--skip-build]
#
# Needs (SKILLS/nc-devx/scripts/setup.android.sh installs them): the Rust target
# aarch64-linux-android for the toolchain in rust-toolchain.toml, cargo-ndk, JDK 17 or newer, and
# the Android SDK (ANDROID_HOME or ANDROID_SDK_ROOT, default ~/Android/Sdk) with build-tools,
# the android-35 platform and the NDK.
#
# Signing: with ANDROID_KEYSTORE (a .jks or .p12 file) and ANDROID_KEYSTORE_PASSWORD set, and
# optionally ANDROID_KEY_ALIAS and ANDROID_KEY_PASSWORD, the APK is signed with that key. Without
# them a throwaway key is made for this build only and a warning is printed: such an APK
# installs, but a later build signed with another key cannot update it in place (Android asks to
# uninstall first, which removes the app's notebooks). Releases meant to be updated need the
# same key every time; docs/releasing.md says where it is kept.
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        *) echo "usage: $0 [--skip-build]" >&2; exit 2 ;;
    esac
done

VERSION="$(tr -d '\r' < Cargo.toml | sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"$/\1/p' | head -n 1)"
[[ -n "$VERSION" ]] || { echo "could not read the workspace version from Cargo.toml" >&2; exit 1; }

# Android needs a whole number that grows with every release: MAJOR, MINOR and PATCH in three
# digits each (0.0.10 is 10, 0.1.0 is 1000, 1.2.3 is 1002003). A pre-release tag is not counted.
IFS=. read -r MAJOR MINOR PATCH <<<"${VERSION%%-*}"
VERSION_CODE=$((MAJOR * 1000000 + MINOR * 1000 + PATCH))
(( VERSION_CODE > 0 )) || { echo "version $VERSION gives no Android version code" >&2; exit 1; }

MIN_SDK=26
TARGET_SDK=35
ABI=arm64-v8a
TARGET=aarch64-linux-android
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
BUILD_TOOLS_VERSION="${ARTCRAFT_BUILD_TOOLS_VERSION:-35.0.0}"
BUILD_TOOLS="$SDK/build-tools/$BUILD_TOOLS_VERSION"
PLATFORM="$SDK/platforms/android-$TARGET_SDK/android.jar"
for tool in aapt2 zipalign apksigner; do
    [[ -x "$BUILD_TOOLS/$tool" ]] || { echo "missing $BUILD_TOOLS/$tool; run SKILLS/nc-devx/scripts/setup.android.sh" >&2; exit 1; }
done
[[ -f "$PLATFORM" ]] || { echo "missing $PLATFORM; run SKILLS/nc-devx/scripts/setup.android.sh" >&2; exit 1; }
for tool in python3 keytool; do
    command -v "$tool" >/dev/null || { echo "missing $tool" >&2; exit 1; }
done

DIST="${DIST:-$ROOT/dist/android}"
WORK="$ROOT/target/android-apk"
NAME="notecraft-$VERSION-android-arm64"
rm -rf "$WORK"
mkdir -p "$WORK" "$DIST"

# 1. The library.
if [[ "$SKIP_BUILD" == 0 ]]; then
    command -v cargo-ndk >/dev/null || { echo "missing cargo-ndk; run SKILLS/nc-devx/scripts/setup.android.sh" >&2; exit 1; }
    if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
        NDK_VERSION="${ARTCRAFT_NDK_VERSION:-27.2.12479018}"
        export ANDROID_NDK_HOME="$SDK/ndk/$NDK_VERSION"
    fi
    cargo ndk --target "$ABI" --platform "$MIN_SDK" build --release --locked -p notecraft-android
fi
LIB="$ROOT/target/$TARGET/release/libnotecraft_android.so"
[[ -s "$LIB" ]] || { echo "missing $LIB; build without --skip-build" >&2; exit 1; }

# 2. The Welcome notebook payload: both generated editions in one asset (android/src/welcome.rs).
mkdir -p "$WORK/assets"
python3 - "$ROOT/docs/demo/DemoNotebook" "$WORK/assets/welcome-to-notecraft.zip" <<'PY'
import os, sys, zipfile
demo, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
    for edition in ("html", "cmgfm"):
        base = os.path.join(demo, edition)
        if not os.path.isdir(base):
            sys.exit(f"missing {base}: run cargo xtask demo")
        for folder, dirs, files in os.walk(base):
            dirs.sort()
            for name in sorted(files):
                path = os.path.join(folder, name)
                arcname = os.path.relpath(path, demo).replace(os.sep, "/")
                info = zipfile.ZipInfo(arcname, date_time=(2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                with open(path, "rb") as source:
                    archive.writestr(info, source.read())
PY

# 3. Resources: the theme and the launcher icon (the app icon, assets/brand/notecraft-256.png).
mkdir -p "$WORK/res"
cp -R "$ROOT/android/res/." "$WORK/res/"
mkdir -p "$WORK/res/mipmap-xxxhdpi"
cp "$ROOT/assets/brand/notecraft-256.png" "$WORK/res/mipmap-xxxhdpi/ic_launcher.png"
"$BUILD_TOOLS/aapt2" compile --dir "$WORK/res" -o "$WORK/resources.zip"
"$BUILD_TOOLS/aapt2" link \
    -o "$WORK/unaligned.apk" \
    -I "$PLATFORM" \
    --manifest "$ROOT/android/AndroidManifest.xml" \
    --min-sdk-version "$MIN_SDK" \
    --target-sdk-version "$TARGET_SDK" \
    --version-code "$VERSION_CODE" \
    --version-name "$VERSION" \
    -A "$WORK/assets" \
    "$WORK/resources.zip"

# 4. The library goes in as lib/<abi>/, without its symbol table (a fifth of its size) and
# deflated: the manifest leaves extractNativeLibs at its default, so Android unpacks it on install
# and the download stays small.
NDK_HOME="${ANDROID_NDK_HOME:-$SDK/ndk/${ARTCRAFT_NDK_VERSION:-27.2.12479018}}"
STRIP="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip"
[[ -x "$STRIP" ]] || { echo "missing $STRIP; set ANDROID_NDK_HOME" >&2; exit 1; }
mkdir -p "$WORK/apk/lib/$ABI"
"$STRIP" --strip-all -o "$WORK/apk/lib/$ABI/libnotecraft_android.so" "$LIB"
(cd "$WORK/apk" && python3 - "$WORK/unaligned.apk" "lib/$ABI/libnotecraft_android.so" <<'PY'
import sys, zipfile
apk, lib = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(apk, "a") as archive:
    archive.write(lib, lib, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
PY
)

# 5. Align and sign (v1 to v3 signatures; no separate v4 .idsig file).
"$BUILD_TOOLS/zipalign" -P 16 -f 4 "$WORK/unaligned.apk" "$WORK/aligned.apk"
KEYSTORE="${ANDROID_KEYSTORE:-}"
if [[ -n "$KEYSTORE" ]]; then
    [[ -f "$KEYSTORE" ]] || { echo "ANDROID_KEYSTORE is set but $KEYSTORE does not exist" >&2; exit 1; }
    [[ -n "${ANDROID_KEYSTORE_PASSWORD:-}" ]] || { echo "ANDROID_KEYSTORE is set but ANDROID_KEYSTORE_PASSWORD is not" >&2; exit 1; }
    STORE_PASS="$ANDROID_KEYSTORE_PASSWORD"
    KEY_PASS="${ANDROID_KEY_PASSWORD:-$ANDROID_KEYSTORE_PASSWORD}"
    ALIAS_ARGS=()
    [[ -n "${ANDROID_KEY_ALIAS:-}" ]] && ALIAS_ARGS=(--ks-key-alias "$ANDROID_KEY_ALIAS")
    echo "signing with the release key in ANDROID_KEYSTORE"
else
    KEYSTORE="$WORK/throwaway.p12"
    STORE_PASS="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
    KEY_PASS="$STORE_PASS"
    ALIAS_ARGS=(--ks-key-alias throwaway)
    keytool -genkeypair -keystore "$KEYSTORE" -storetype PKCS12 -storepass "$STORE_PASS" -keypass "$KEY_PASS" \
        -alias throwaway -keyalg RSA -keysize 3072 -validity 10000 -dname "CN=NoteCraft build $VERSION" >/dev/null 2>&1
    echo "warning: no ANDROID_KEYSTORE; signed with a throwaway key made for this build only." >&2
    echo "warning: a build signed with any other key cannot update this one in place." >&2
fi
# Passwords go through the environment, never the command line.
NOTECRAFT_STORE_PASS="$STORE_PASS" NOTECRAFT_KEY_PASS="$KEY_PASS" "$BUILD_TOOLS/apksigner" sign \
    --ks "$KEYSTORE" --ks-pass env:NOTECRAFT_STORE_PASS --key-pass env:NOTECRAFT_KEY_PASS "${ALIAS_ARGS[@]}" \
    --min-sdk-version "$MIN_SDK" --v4-signing-enabled false --out "$DIST/$NAME.apk" "$WORK/aligned.apk"
rm -f "$WORK/throwaway.p12"
"$BUILD_TOOLS/apksigner" verify --min-sdk-version "$MIN_SDK" "$DIST/$NAME.apk"
echo "wrote $DIST/$NAME.apk (version $VERSION, code $VERSION_CODE)"
