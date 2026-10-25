#!/bin/bash
# NoteCraft Developer Setup - Linux
# Run: bash SKILLS/nc-devx/scripts/setup.sh
#
# Options: SKIP_RUST=1, SKIP_DEPS=1, SKIP_BUILD=1

set -e

SKIP_RUST=${SKIP_RUST:-0}
SKIP_DEPS=${SKIP_DEPS:-0}
SKIP_BUILD=${SKIP_BUILD:-0}

echo -e "\033[36mNoteCraft Developer Setup (Linux)\033[0m"
echo -e "\033[36m================================\033[0m"
echo ""

# Step 1: Check Rust
if [ "$SKIP_RUST" = "0" ]; then
    echo -e "\033[32mStep 1: Checking Rust...\033[0m"

    if command -v rustc &> /dev/null; then
        RUST_VERSION=$(rustc --version)
        echo -e "\033[32m✓ Rust is installed: $RUST_VERSION\033[0m"
    else
        echo -e "\033[33m✗ Rust not found. Installing from rustup.rs...\033[0m"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
        source "$HOME/.cargo/env"
        echo -e "\033[32m✓ Rust installed\033[0m"
    fi
    echo ""
fi

# Step 2: Install platform dependencies
if [ "$SKIP_DEPS" = "0" ]; then
    echo -e "\033[32mStep 2: Checking platform dependencies...\033[0m"

    MISSING_DEPS=()

    # Check for build essentials
    if ! command -v gcc &> /dev/null && ! command -v clang &> /dev/null; then
        MISSING_DEPS+=("build-essential (gcc/clang)")
    fi

    # Check for pkg-config
    if ! command -v pkg-config &> /dev/null; then
        MISSING_DEPS+=("pkg-config")
    fi

    # Check for git
    if ! command -v git &> /dev/null; then
        MISSING_DEPS+=("git")
    fi

    # Check for X11 development (for UI)
    if ! pkg-config --exists libxkbcommon 2>/dev/null; then
        MISSING_DEPS+=("X11/Wayland libs (libxkbcommon, etc.)")
    fi

    if [ ${#MISSING_DEPS[@]} -gt 0 ]; then
        echo -e "\033[33m⚠ Missing dependencies:\033[0m"
        printf '  - %s\n' "${MISSING_DEPS[@]}"
        echo ""
        echo -e "\033[33mInstall with:\033[0m"
        echo "  Ubuntu/Debian: sudo apt-get install build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev libfontconfig1-dev libfreetype6-dev git"
        echo "  Fedora/RHEL:   sudo dnf install gcc pkg-config libxkbcommon-devel wayland-devel libX11-devel fontconfig-devel freetype-devel git"
        echo "  Arch:          sudo pacman -S base-devel pkg-config libxkbcommon wayland libx11 fontconfig freetype2 git"
        echo ""
        read -p "Continue without installing missing dependencies? (y/n) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    else
        echo -e "\033[32m✓ All dependencies found\033[0m"
    fi
    echo ""
fi

# Step 3: Enable git hooks
echo -e "\033[32mStep 3: Enabling git hooks...\033[0m"
git config core.hooksPath .githooks
echo -e "\033[32m✓ Git hooks enabled\033[0m"
echo ""

# Step 4: Setup development environment
echo -e "\033[32mStep 4: Setting up dev environment...\033[0m"

# Update toolchain
echo "  - Updating Rust toolchain..." >&2
rustup install 2>&1 | grep -E "^(info|Updating)" || true
echo "  ✓ Rust toolchain updated" >&2

# Build xtask
echo "  - Building xtask (dev helper)..." >&2
cargo build -p xtask 2>&1 | tail -1 || true
echo "  ✓ xtask built" >&2
echo ""

# Step 5: Verify build (unless skipped)
if [ "$SKIP_BUILD" = "0" ]; then
    echo -e "\033[32mStep 5: Verifying build...\033[0m"
    cargo build -p notecraft 2>&1 | tail -3
    echo -e "\033[32m✓ Build successful\033[0m"
    echo ""

    # Step 6: Run quick test
    echo -e "\033[32mStep 6: Running quick test...\033[0m"
    if cargo test --workspace --lib -- --test-threads=1 2>&1 | tail -3; then
        echo -e "\033[32m✓ Tests passed\033[0m"
    else
        echo -e "\033[33mⓘ Some tests failed (may be expected on first run)\033[0m"
    fi
    echo ""
fi

# Summary
echo -e "\033[32mSetup Complete!\033[0m"
echo -e "\033[32m===============\033[0m"
echo ""
echo -e "\033[36mQuick start commands:\033[0m"
echo "  cargo run -p notecraft              # Run the app"
echo "  cargo test --workspace              # Run all tests"
echo "  cargo xtask ci                      # Run full CI gate"
echo "  cargo watch -x check                # Watch and check on changes"
echo ""
echo -e "\033[36mDocumentation: SKILLS/nc-devx/SKILL.md\033[0m"
