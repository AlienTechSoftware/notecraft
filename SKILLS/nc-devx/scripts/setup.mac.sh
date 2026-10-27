#!/bin/bash
# NoteCraft developer setup and check - macOS
# Run from the repository root: bash SKILLS/nc-devx/scripts/setup.mac.sh
#
# Checks each tool NoteCraft needs. On a machine that is already set up it only reports, all OK.
# What is missing is installed:
#   - the Xcode Command Line Tools (the compiler and linker); this opens Apple's installer, and
#     the script is run again once it finishes
#   - rustup, and the Rust named in rust-toolchain.toml with clippy and rustfmt
# Then it turns the git hooks on and builds xtask. Homebrew is not needed.
#
#   SKIP_RUST=1    leave Rust alone
#   SKIP_XCODE=1   leave the Command Line Tools alone
#   SKIP_BUILD=1   do not build xtask
#   CHECK_ONLY=1   report what is missing, change nothing
#
# Exit status: 0 when everything needed is there, 1 otherwise.

SKIP_RUST=${SKIP_RUST:-0}
SKIP_XCODE=${SKIP_XCODE:-0}
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

echo -e "\033[36mNoteCraft developer setup (macOS)\033[0m"
[ "$CHECK_ONLY" = "1" ] && echo "Check only: nothing is installed or changed."

if [ "$SKIP_XCODE" = "0" ]; then
    step "Xcode Command Line Tools"
    if xcode-select -p > /dev/null 2>&1; then
        ok "$(xcode-select -p)"
    elif [ "$CHECK_ONLY" = "1" ]; then
        bad "missing: xcode-select --install"
    else
        info "opening Apple's installer for the Command Line Tools"
        xcode-select --install
        bad "finish the Command Line Tools install in the dialog, then run this script again"
    fi
fi

step "Git"
if have git && git --version > /dev/null 2>&1; then ok "$(git --version)"; else bad "git is not installed (it comes with the Command Line Tools)"; fi

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
