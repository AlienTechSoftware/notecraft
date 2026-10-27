#!/bin/bash
# NoteCraft developer setup and check - Linux
# Run from the repository root: bash SKILLS/nc-devx/scripts/setup.sh
#
# Checks each tool NoteCraft needs. On a machine that is already set up it only reports, all OK.
# What is missing is installed:
#   - a C compiler, pkg-config and the window system libraries, through apt, dnf or pacman (sudo)
#   - rustup, and the Rust named in rust-toolchain.toml with clippy and rustfmt
# Then it turns the git hooks on and builds xtask.
#
#   SKIP_RUST=1    leave Rust alone
#   SKIP_DEPS=1    leave the system packages alone
#   SKIP_BUILD=1   do not build xtask
#   CHECK_ONLY=1   report what is missing, change nothing
#
# Exit status: 0 when everything needed is there, 1 otherwise.

SKIP_RUST=${SKIP_RUST:-0}
SKIP_DEPS=${SKIP_DEPS:-0}
SKIP_BUILD=${SKIP_BUILD:-0}
CHECK_ONLY=${CHECK_ONLY:-0}

ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$ROOT" || exit 1
FAILED=()

ok() { echo -e "\033[32mOK   $*\033[0m"; }
info() { echo -e "\033[33m...  $*\033[0m"; }
bad() { echo -e "\033[31mFAIL $*\033[0m"; FAILED+=("$*"); }
step() { echo; echo -e "\033[36m$*\033[0m"; }
have() { command -v "$1" > /dev/null 2>&1; }

# Puts rustup's folder on PATH for this run, if it is there.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

echo -e "\033[36mNoteCraft developer setup (Linux)\033[0m"
[ "$CHECK_ONLY" = "1" ] && echo "Check only: nothing is installed or changed."

step "Git"
if have git; then ok "$(git --version)"; else bad "git is not installed"; fi

if [ "$SKIP_DEPS" = "0" ]; then
    step "System packages"
    missing=()
    have cc || have gcc || have clang || missing+=("a C compiler")
    have pkg-config || missing+=("pkg-config")
    for lib in xkbcommon wayland-client x11 fontconfig freetype2; do
        pkg-config --exists "$lib" 2> /dev/null || missing+=("$lib")
    done
    if [ ${#missing[@]} -eq 0 ]; then
        ok "compiler, pkg-config and window system libraries"
    elif [ "$CHECK_ONLY" = "1" ]; then
        bad "missing: ${missing[*]}"
    else
        info "missing: ${missing[*]}; installing"
        if have apt-get; then
            sudo apt-get update && sudo apt-get install -y build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev libfontconfig1-dev libfreetype6-dev
        elif have dnf; then
            sudo dnf install -y gcc pkg-config libxkbcommon-devel wayland-devel libX11-devel fontconfig-devel freetype-devel
        elif have pacman; then
            sudo pacman -S --needed --noconfirm base-devel pkg-config libxkbcommon wayland libx11 fontconfig freetype2
        else
            bad "no apt-get, dnf or pacman here; install by hand: ${missing[*]}"
        fi
        still=()
        have cc || have gcc || have clang || still+=("a C compiler")
        for lib in xkbcommon wayland-client x11 fontconfig freetype2; do
            pkg-config --exists "$lib" 2> /dev/null || still+=("$lib")
        done
        if [ ${#still[@]} -eq 0 ]; then ok "installed"; else bad "still missing: ${still[*]}"; fi
    fi
fi

if [ "$SKIP_RUST" = "0" ]; then
    step "Rust"
    channel=$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' rust-toolchain.toml)
    if ! have rustup && [ "$CHECK_ONLY" != "1" ]; then
        info "installing rustup from sh.rustup.rs"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal
        [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
    fi
    if have rustup; then
        ok "$(rustup --version 2> /dev/null | head -n 1)"
        if rustup toolchain list 2> /dev/null | grep -q "^$channel-"; then
            ok "Rust $channel (pinned in rust-toolchain.toml)"
        elif [ "$CHECK_ONLY" = "1" ]; then
            bad "Rust $channel is not installed: rustup toolchain install $channel"
        else
            info "installing Rust $channel with clippy and rustfmt"
            if rustup toolchain install "$channel" --profile minimal --component clippy --component rustfmt; then
                ok "Rust $channel"
            else
                bad "rustup could not install Rust $channel"
            fi
        fi
    else
        bad "rustup is not installed (https://rustup.rs)"
    fi
fi

step "Git hooks"
if [ "$(git config core.hooksPath)" = ".githooks" ]; then
    ok "on (core.hooksPath = .githooks)"
elif [ "$CHECK_ONLY" = "1" ]; then
    bad "off: git config core.hooksPath .githooks"
elif git config core.hooksPath .githooks; then
    ok "turned on (core.hooksPath = .githooks)"
else
    bad "could not set core.hooksPath"
fi

if [ "$SKIP_BUILD" = "0" ] && [ "$CHECK_ONLY" != "1" ]; then
    step "xtask"
    if ! have cargo; then
        bad "cargo is not on PATH, so xtask was not built"
    elif cargo build -q -p xtask; then
        ok "builds"
    else
        bad "cargo build -p xtask failed"
    fi
fi

echo
if [ ${#FAILED[@]} -eq 0 ]; then
    echo -e "\033[32mAll green.\033[0m"
    echo "  cargo run -p notecraft      run the app"
    echo "  cargo xtask ci              the full gate"
    echo "Open a new terminal if a tool was installed, so it is on PATH there too."
    exit 0
fi
echo -e "\033[31mNot ready:\033[0m"
printf '  - %s\n' "${FAILED[@]}"
exit 1
