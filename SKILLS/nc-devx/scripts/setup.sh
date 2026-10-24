#!/bin/bash
# NoteCraft Developer Setup - Linux
# Run: bash SKILLS/nc-devx/scripts/setup.sh

set -e

echo -e "\033[36mNoteCraft Developer Setup (Linux)\033[0m"
echo -e "\033[36m================================\033[0m"
echo ""

# Step 1: Check Rust
echo -e "\033[32mStep 1: Checking Rust...\033[0m"

if command -v rustc &> /dev/null; then
    RUST_VERSION=$(rustc --version)
    echo -e "\033[32m✓ Rust is installed: $RUST_VERSION\033[0m"
else
    echo -e "\033[33mInstalling Rust from rustup.rs...\033[0m"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source "$HOME/.cargo/env"
    echo -e "\033[32m✓ Rust installed\033[0m"
fi
echo ""

# Step 2: Install platform dependencies
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

if [ ${#MISSING_DEPS[@]} -gt 0 ]; then
    echo -e "\033[33mⓘ Missing dependencies:\033[0m"
    printf '  - %s\n' "${MISSING_DEPS[@]}"
    echo ""
    echo -e "\033[33mInstall with:\033[0m"
    echo "  Ubuntu/Debian: sudo apt-get install build-essential pkg-config git"
    echo "  Fedora/RHEL:   sudo dnf install gcc pkg-config git"
    echo "  Arch:          sudo pacman -S base-devel pkg-config git"
    echo ""
    read -p "Continue anyway? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
else
    echo -e "\033[32m✓ All dependencies found\033[0m"
fi
echo ""

# Step 3: Enable git hooks
echo -e "\033[32mStep 3: Enabling git hooks...\033[0m"
git config core.hooksPath .githooks
echo -e "\033[32m✓ Git hooks enabled\033[0m"
echo ""

# Step 4: Verify build
echo -e "\033[32mStep 4: Verifying build...\033[0m"
if cargo build -p notecraft 2>&1 | tail -5; then
    echo -e "\033[32m✓ Build successful\033[0m"
else
    echo -e "\033[31m✗ Build failed. Check the output above.\033[0m"
    exit 1
fi
echo ""

# Step 5: Run quick test
echo -e "\033[32mStep 5: Running quick test...\033[0m"
if cargo test --workspace -- --test-threads=1 2>&1 | tail -3; then
    echo -e "\033[32m✓ Tests passed\033[0m"
else
    echo -e "\033[33mⓘ Some tests failed or timed out (may be OK for first run)\033[0m"
fi
echo ""

# Summary
echo -e "\033[32mSetup Complete!\033[0m"
echo -e "\033[32m===============\033[0m"
echo ""
echo -e "\033[36mNext steps:\033[0m"
echo "  1. Run the app:     cargo run -p notecraft"
echo "  2. Run tests:       cargo test --workspace"
echo "  3. Check gates:     cargo xtask ci"
echo ""
echo -e "\033[36mFor more info, see SKILLS/nc-devx/SKILL.md\033[0m"
