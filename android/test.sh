#!/usr/bin/env bash
# Checks the APK android/build.sh wrote: signed, the app's identity and SDK levels, the native
# library for arm64, and both Welcome notebook editions with all 14 languages inside.
#
#   bash android/test.sh
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(tr -d '\r' < "$ROOT/Cargo.toml" | sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"$/\1/p' | head -n 1)"
DIST="${DIST:-$ROOT/dist/android}"
APK="$DIST/notecraft-$VERSION-android-arm64.apk"
[[ -s "$APK" ]] || { echo "missing package: $APK" >&2; exit 1; }
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
BUILD_TOOLS="$SDK/build-tools/${ARTCRAFT_BUILD_TOOLS_VERSION:-35.0.0}"

"$BUILD_TOOLS/apksigner" verify --min-sdk-version 26 "$APK"
badging="$("$BUILD_TOOLS/aapt2" dump badging "$APK")"
grep -q "^package: name='com.alientechsoftware.notecraft' versionCode='[1-9][0-9]*' versionName='$VERSION'" <<<"$badging" \
    || { echo "unexpected package line: $(head -n 1 <<<"$badging")" >&2; exit 1; }
grep -q "^minSdkVersion:'26'" <<<"$badging" || { echo "minSdkVersion is not 26" >&2; exit 1; }
grep -q "^targetSdkVersion:'35'" <<<"$badging" || { echo "targetSdkVersion is not 35" >&2; exit 1; }
grep -q "^native-code: 'arm64-v8a'" <<<"$badging" || { echo "no arm64-v8a native code" >&2; exit 1; }
grep -q "^launchable-activity: name='android.app.NativeActivity'" <<<"$badging" || { echo "no launchable NativeActivity" >&2; exit 1; }

python3 - "$APK" <<'PY'
import io, re, sys, zipfile
apk = zipfile.ZipFile(sys.argv[1])
lib = apk.getinfo("lib/arm64-v8a/libnotecraft_android.so")
assert lib.file_size > 1_000_000, f"the native library is only {lib.file_size} bytes"
assert b".symtab" not in apk.read(lib.filename)[-65536:], "the native library still has its symbol table"
assert not any(n.endswith(".dex") for n in apk.namelist()), "the APK should have no Java code"
payload = zipfile.ZipFile(io.BytesIO(apk.read("assets/welcome-to-notecraft.zip")))
names = payload.namelist()
for path in ("html/_node.toml", "html/00-start/welcome/index.html", "cmgfm/_node.toml", "cmgfm/00-start/welcome/index.md"):
    assert path in names, f"the Welcome payload has no {path}"
for edition in ("html", "cmgfm"):
    languages = {n.split("/")[1] for n in names if n.startswith(edition + "/") and re.fullmatch(r"(1[0-9]|2[0-9])-[a-z][a-z]", n.split("/")[1])}
    assert len(languages) == 14, f"{edition} has {len(languages)} language folders, expected 14: {sorted(languages)}"
print(f"APK contents checked: {len(names)} payload files, native library {lib.file_size} bytes")
PY
echo "android packaging smoke test passed: $APK"
