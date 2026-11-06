#!/usr/bin/env bash
# NoteCraft Android development prerequisites (Linux/WSL2).
# Run from any directory: bash SKILLS/nc-devx/scripts/setup.android.sh [--check-only]
set -Eeuo pipefail

CHECK_ONLY="${CHECK_ONLY:-0}"
if [[ "${1:-}" == "--check-only" ]]; then
    CHECK_ONLY=1
elif [[ -n "${1:-}" ]]; then
    echo "Usage: $0 [--check-only]" >&2
    exit 2
fi

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
[[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"

ANDROID_TARGET="aarch64-linux-android"
CARGO_NDK_VERSION="${CARGO_NDK_VERSION:-4.1.2}"
ANDROID_API="${ARTCRAFT_TARGET_SDK:-35}"
BUILD_TOOLS_VERSION="${ARTCRAFT_BUILD_TOOLS_VERSION:-35.0.0}"
NDK_VERSION="${ARTCRAFT_NDK_VERSION:-27.2.12479018}"
SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Android/Sdk}}"
FAILED=()
TEMP_DIRS=()

cleanup_temps() {
    local temp_dir
    for temp_dir in "${TEMP_DIRS[@]}"; do
        [[ -n "$temp_dir" ]] && rm -rf -- "$temp_dir"
    done
}
trap cleanup_temps EXIT

ok() { printf 'OK   %s\n' "$*"; }
info() { printf '...  %s\n' "$*"; }
bad() { printf 'FAIL %s\n' "$*"; FAILED+=("$*"); }
step() { printf '\n%s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
get_java_major() { java -version 2>&1 | sed -n '1s/.*version "\([0-9][0-9]*\).*/\1/p'; }
get_javac_major() { javac -version 2>&1 | sed -n '1s/.*javac \([0-9][0-9]*\).*/\1/p'; }

channel="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' rust-toolchain.toml | head -n 1)"
if [[ -z "$channel" ]]; then
    echo "Could not read the Rust channel from rust-toolchain.toml." >&2
    exit 1
fi

if [[ "$CHECK_ONLY" == "1" ]]; then
    echo "Check only: nothing will be installed or changed."
fi

step "Android host prerequisites"
if [[ "$(uname -s)" != "Linux" ]]; then
    bad "Run this script in Linux or WSL2; the Android builder uses Linux tooling."
fi

missing_commands=()
for tool in git curl unzip zip tar python3 java javac rustup cargo cc; do
    have "$tool" || missing_commands+=("$tool")
done

python_ok=0
if have python3 && python3 -c 'import sys,tomllib; raise SystemExit(sys.version_info < (3,11))' 2>/dev/null; then
    python_ok=1
fi
if [[ "$python_ok" == "0" ]]; then
    missing_commands+=("Python 3.11+ with tomllib")
fi

java_ok=0
if have java && have javac; then
    java_major="$(get_java_major)"
    javac_major="$(get_javac_major)"
    [[ "$java_major" == "21" && "$javac_major" == "21" ]] && java_ok=1
fi
if [[ "$java_ok" == "0" ]]; then
    missing_commands+=("JDK 21 (java and javac)")
fi
if have python3 && ! python3 -c 'import PIL' >/dev/null 2>&1; then
    missing_commands+=("Python Pillow")
fi

as_root() {
    if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        "$@"
    elif have sudo; then
        sudo "$@"
    else
        return 1
    fi
}

install_system_packages() {
    if have apt-get; then
        as_root apt-get update && as_root apt-get install -y git curl unzip zip tar python3 python3-pil build-essential pkg-config openjdk-21-jdk
    elif have dnf; then
        as_root dnf install -y git curl unzip zip tar python3 python3-pillow gcc pkgconf-pkg-config java-21-openjdk-devel
    elif have pacman; then
        as_root pacman -S --needed --noconfirm git curl unzip zip tar python python-pillow base-devel pkgconf jdk21-openjdk
    else
        return 1
    fi
}

if [[ "${#missing_commands[@]}" -eq 0 ]]; then
    ok "Git, C build tools, Python 3.11+ with Pillow, curl, zip tools, and JDK 21"
elif [[ "$CHECK_ONLY" == "1" ]]; then
    bad "missing: ${missing_commands[*]}"
else
    info "installing host packages with the system package manager"
    if ! install_system_packages; then
        bad "could not install host packages; run with sudo or as root, then install Git, C build tools, Python 3.11+ with Pillow, curl, zip tools, and JDK 21"
    fi
fi

if [[ "$CHECK_ONLY" != "1" ]]; then
    python_ok=0
    if have python3 && python3 -c 'import sys,tomllib; raise SystemExit(sys.version_info < (3,11))' 2>/dev/null; then
        python_ok=1
    fi
    [[ "$python_ok" == "1" ]] || bad "Python 3.11+ with tomllib is still missing after package installation"
    have python3 && python3 -c 'import PIL' >/dev/null 2>&1 || bad "Python Pillow is still missing after package installation"
    have java && have javac || bad "JDK 21 is still missing after package installation"
fi

if have javac; then
    javac_path="$(readlink -f "$(command -v javac)")"
    if [[ "$javac_path" == */bin/javac ]]; then
        export JAVA_HOME="${javac_path%/bin/javac}"
    fi
fi

if [[ "$CHECK_ONLY" != "1" ]]; then
    if [[ ! -x "$HOME/.cargo/bin/rustup" ]] && ! have rustup; then
        if have curl && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal; then
            [[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
        else
            bad "could not install rustup"
        fi
    fi
    [[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
    export PATH="$HOME/.cargo/bin:$PATH"
fi

step "Rust Android target"
if ! have rustup; then
    bad "rustup is missing"
elif ! rustup toolchain list 2>/dev/null | grep -Eq "^${channel}(-|$)"; then
    if [[ "$CHECK_ONLY" == "1" ]]; then
        bad "Rust $channel is missing: rustup toolchain install $channel --profile minimal --component clippy --component rustfmt"
    elif rustup toolchain install "$channel" --profile minimal --component clippy --component rustfmt; then
        ok "installed Rust $channel with clippy and rustfmt"
    else
        bad "could not install Rust $channel"
    fi
fi

if have rustup && rustup toolchain list 2>/dev/null | grep -Eq "^${channel}(-|$)"; then
    if rustup target list --toolchain "$channel" --installed | grep -Fqx "$ANDROID_TARGET"; then
        ok "$ANDROID_TARGET for Rust $channel"
    elif [[ "$CHECK_ONLY" == "1" ]]; then
        bad "missing $ANDROID_TARGET for Rust $channel: rustup target add --toolchain $channel $ANDROID_TARGET"
    elif rustup target add --toolchain "$channel" "$ANDROID_TARGET"; then
        ok "installed $ANDROID_TARGET for Rust $channel"
    else
        bad "could not install $ANDROID_TARGET for Rust $channel"
    fi
fi

if [[ "$CHECK_ONLY" != "1" ]] && have rustup; then
    export RUSTUP_TOOLCHAIN="$channel"
fi

step "cargo-ndk"
if have cargo && RUSTUP_TOOLCHAIN="$channel" cargo ndk -V >/dev/null 2>&1; then
    ok "cargo-ndk $(RUSTUP_TOOLCHAIN="$channel" cargo ndk -V)"
elif [[ "$CHECK_ONLY" == "1" ]]; then
    bad "cargo-ndk $CARGO_NDK_VERSION is missing or unusable"
elif ! have curl || ! have tar; then
    bad "cargo-ndk needs curl and tar to install"
elif [[ "$(uname -m)" == "x86_64" ]]; then
    mkdir -p "$HOME/.cargo/bin"
    cargo_ndk_tmp="$(mktemp -d "$HOME/.cargo/.cargo-ndk.XXXXXX")"
    TEMP_DIRS+=("$cargo_ndk_tmp")
    cargo_ndk_url="https://github.com/bbqsrc/cargo-ndk/releases/download/v${CARGO_NDK_VERSION}/cargo-ndk-x86_64-unknown-linux-musl-v${CARGO_NDK_VERSION}.tgz"
    if curl -fL --retry 3 -o "$cargo_ndk_tmp/cargo-ndk.tgz" "$cargo_ndk_url" \
        && tar -xzf "$cargo_ndk_tmp/cargo-ndk.tgz" --strip-components=1 -C "$HOME/.cargo/bin" \
        && chmod +x "$HOME/.cargo/bin"/cargo-ndk* \
        && RUSTUP_TOOLCHAIN="$channel" cargo ndk -V >/dev/null 2>&1; then
        ok "cargo-ndk $(RUSTUP_TOOLCHAIN="$channel" cargo ndk -V)"
    else
        bad "could not install or run cargo-ndk $CARGO_NDK_VERSION"
    fi
else
    info "installing cargo-ndk from crates.io for Rust $channel"
    if RUSTUP_TOOLCHAIN="$channel" cargo install cargo-ndk --version "$CARGO_NDK_VERSION" --locked; then
        ok "cargo-ndk $CARGO_NDK_VERSION"
    else
        bad "could not install cargo-ndk $CARGO_NDK_VERSION"
    fi
fi

sdk_real="$(realpath -m "$SDK_ROOT")"
export ANDROID_HOME="$SDK_ROOT" ANDROID_SDK_ROOT="$SDK_ROOT"
export PATH="$SDK_ROOT/platform-tools:$SDK_ROOT/cmdline-tools/latest/bin:$HOME/.cargo/bin:$PATH"
if [[ "$sdk_real" == /mnt/* ]]; then
    bad "SDK path is on a WSL-mounted drive ($sdk_real); use the Linux filesystem, such as \$HOME/Android/Sdk"
fi

sdkmanager="$SDK_ROOT/cmdline-tools/latest/bin/sdkmanager"
sdk_missing=()
[[ -x "$sdkmanager" ]] || sdk_missing+=("Android command-line tools")
[[ -d "$SDK_ROOT/platforms/android-$ANDROID_API" ]] || sdk_missing+=("platform android-$ANDROID_API")
[[ -d "$SDK_ROOT/build-tools/$BUILD_TOOLS_VERSION" ]] || sdk_missing+=("build-tools $BUILD_TOOLS_VERSION")
[[ -d "$SDK_ROOT/ndk/$NDK_VERSION" ]] || sdk_missing+=("NDK $NDK_VERSION")
[[ -x "$SDK_ROOT/platform-tools/adb" ]] || sdk_missing+=("platform-tools (adb)")
[[ -f "$SDK_ROOT/licenses/android-sdk-license" ]] || sdk_missing+=("Android SDK license acceptance")

step "Android SDK"
if [[ "$CHECK_ONLY" == "1" ]]; then
    if [[ "${#sdk_missing[@]}" -eq 0 ]]; then
        ok "SDK platform $ANDROID_API, build-tools $BUILD_TOOLS_VERSION, NDK $NDK_VERSION, adb"
    else
        bad "SDK $SDK_ROOT is missing: ${sdk_missing[*]}"
    fi
elif [[ "$sdk_real" != /mnt/* ]]; then
    mkdir -p "$SDK_ROOT"
    export ANDROID_HOME="$SDK_ROOT" ANDROID_SDK_ROOT="$SDK_ROOT"
    if [[ ! -x "$sdkmanager" ]]; then
        if ! have python3 || ! have curl || ! have unzip; then
            bad "install Python 3, curl, and unzip before downloading Android command-line tools"
        else
            cmdline_url="$(python3 - <<'PY'
import re
import urllib.request

best = 0
for index in ("repository2-3.xml", "repository2-1.xml"):
    try:
        with urllib.request.urlopen(f"https://dl.google.com/android/repository/{index}", timeout=60) as response:
            data = response.read().decode("utf-8", "replace")
    except Exception:
        continue
    for number in re.findall(r"commandlinetools-linux-(\d+)_latest\.zip", data):
        best = max(best, int(number))
if best:
    print(f"https://dl.google.com/android/repository/commandlinetools-linux-{best}_latest.zip")
PY
            )"
            if [[ -z "$cmdline_url" ]]; then
                bad "could not find Android command-line tools in Google's repository index"
            else
                sdk_tmp="$(mktemp -d "$SDK_ROOT/.cmdline-tools.XXXXXX")"
                TEMP_DIRS+=("$sdk_tmp")
                if curl -fL --retry 3 -o "$sdk_tmp/tools.zip" "$cmdline_url" \
                    && unzip -q "$sdk_tmp/tools.zip" -d "$sdk_tmp" \
                    && [[ -d "$sdk_tmp/cmdline-tools" ]]; then
                    mkdir -p "$SDK_ROOT/cmdline-tools"
                    if [[ -e "$SDK_ROOT/cmdline-tools/latest" ]]; then
                        bad "refusing to replace existing $SDK_ROOT/cmdline-tools/latest"
                    elif mv "$sdk_tmp/cmdline-tools" "$SDK_ROOT/cmdline-tools/latest"; then
                        sdkmanager="$SDK_ROOT/cmdline-tools/latest/bin/sdkmanager"
                        chmod +x "$sdkmanager" "$SDK_ROOT/cmdline-tools/latest/bin/"* 2>/dev/null || true
                        ok "Android command-line tools"
                    else
                        bad "could not move Android command-line tools into $SDK_ROOT; keep the SDK under the Linux filesystem"
                    fi
                else
                    bad "could not download or extract Android command-line tools"
                fi
            fi
        fi
    fi

    if [[ -x "$sdkmanager" ]]; then
        java_major=""
        javac_major=""
        if have java && have javac; then
            java_major="$(get_java_major)"
            javac_major="$(get_javac_major)"
        fi
        if [[ "$java_major" != "21" || "$javac_major" != "21" ]]; then
            bad "JDK 21 is required to run sdkmanager"
        else
            if sdkmanager --sdk_root="$SDK_ROOT" --licenses; then
                if sdkmanager --sdk_root="$SDK_ROOT" \
                    "platform-tools" "platforms;android-$ANDROID_API" "build-tools;$BUILD_TOOLS_VERSION" "ndk;$NDK_VERSION"; then
                    sdk_missing=()
                    [[ -d "$SDK_ROOT/platforms/android-$ANDROID_API" ]] || sdk_missing+=("platform $ANDROID_API")
                    [[ -d "$SDK_ROOT/build-tools/$BUILD_TOOLS_VERSION" ]] || sdk_missing+=("build-tools $BUILD_TOOLS_VERSION")
                    [[ -d "$SDK_ROOT/ndk/$NDK_VERSION" ]] || sdk_missing+=("NDK $NDK_VERSION")
                    [[ -x "$SDK_ROOT/platform-tools/adb" ]] || sdk_missing+=("adb")
                    if [[ "${#sdk_missing[@]}" -eq 0 ]]; then
                        ok "Android platform $ANDROID_API, build-tools $BUILD_TOOLS_VERSION, NDK $NDK_VERSION, platform-tools"
                    else
                        bad "sdkmanager returned success but did not install: ${sdk_missing[*]}"
                    fi
                else
                    bad "sdkmanager could not install the requested Android packages"
                fi
            else
                bad "Android SDK licenses were not accepted"
            fi
        fi
    fi
fi

step "Result"
if [[ "${#FAILED[@]}" -eq 0 ]]; then
    ok "Android build prerequisites are ready"
    echo "SDK: $SDK_ROOT"
    echo "Build: RUSTUP_TOOLCHAIN=$channel bash <android-builder>/build.sh -notecraft"
    echo "Device: adb devices -l"
    echo "For future shells, export ANDROID_HOME and ANDROID_SDK_ROOT to the SDK path and add platform-tools to PATH."
    exit 0
fi

printf 'Not ready:\n'
printf '  - %s\n' "${FAILED[@]}"
exit 1
