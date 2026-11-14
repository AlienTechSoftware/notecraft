#!/usr/bin/env bash
# Prepares a GitHub-hosted Ubuntu runner for the Android build: JDK 21, the SDK packages the
# build uses, the Rust target for the pinned toolchain and cargo-ndk. The runner image already
# has the Android command-line tools under ANDROID_HOME. A developer machine uses
# SKILLS/nc-devx/scripts/setup.android.sh instead.
set -Eeuo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
: "${GITHUB_ENV:?this script is for GitHub Actions runners}"
: "${ANDROID_HOME:?the runner has no ANDROID_HOME}"

NDK_VERSION="27.2.12479018"
export JAVA_HOME="${JAVA_HOME_21_X64:-$JAVA_HOME}"
echo "JAVA_HOME=$JAVA_HOME" >> "$GITHUB_ENV"
sdkmanager="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
(yes || true) | "$sdkmanager" --licenses > /dev/null
"$sdkmanager" --install "platforms;android-35" "build-tools;35.0.0" "ndk;$NDK_VERSION" > /dev/null
echo "ANDROID_NDK_HOME=$ANDROID_HOME/ndk/$NDK_VERSION" >> "$GITHUB_ENV"

rustup install
rustup target add aarch64-linux-android
cargo install cargo-ndk --version 4.1.2 --locked
