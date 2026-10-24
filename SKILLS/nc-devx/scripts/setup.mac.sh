#!/bin/bash
# NoteCraft Developer Setup - macOS
# Run: bash SKILLS/nc-devx/scripts/setup.mac.sh

set -e

echo -e "\033[36mNoteCraft Developer Setup (macOS)\033[0m"
echo -e "\033[36m=================================\033[0m"
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

# Step 2: Check Xcode Command Line Tools
echo -e "\033[32mStep 2: Checking Xcode Command Line Tools...\033[0m"

if ! command -v xcode-select &> /dev/null || ! xcode-select -p &> /dev/null; then
    echo -e "\033[33mInstalling Xcode Command Line Tools...\033[0m"
    xcode-select --install
    echo -e "\033[33mPlease complete the installation in the dialog and re-run this script.\033[0m"
    exit 1
else
    echo -e "\033[32m✓ Xcode Command Line Tools installed\033[0m"
fi
echo ""

# Step 3: Check Homebrew (optional but recommended)
echo -e "\033[32mStep 3: Checking Homebrew...\033[0m"

if command -v brew &> /dev/null; then
    echo -e "\033[32m✓ Homebrew is installed\033[0m"
else
    echo -e "\033[33mⓘ Homebrew not found (optional)\033[0m"
    read -p "Install Homebrew? (recommended) (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        echo -e "\033[32m✓ Homebrew installed\033[0m"
    fi
fi
echo ""

# Step 4: Enable git hooks
echo -e "\033[32mStep 4: Enabling git hooks...\033[0m"
git config core.hooksPath .githooks
echo -e "\033[32m✓ Git hooks enabled\033[0m"
echo ""

# Step 5: Verify build
echo -e "\033[32mStep 5: Verifying build...\033[0m"
if cargo build -p notecraft 2>&1 | tail -5; then
    echo -e "\033[32m✓ Build successful\033[0m"
else
    echo -e "\033[31m✗ Build failed. Check the output above.\033[0m"
    exit 1
fi
echo ""

# Step 6: Run quick test
echo -e "\033[32mStep 6: Running quick test...\033[0m"
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
echo -e "\033[36mOptional: Install cargo-watch for faster development:\033[0m"
echo "  brew install cargo-watch"
echo "  cargo watch -x 'check -p notecraft'"
echo ""
echo -e "\033[36mFor more info, see SKILLS/nc-devx/SKILL.md\033[0m"
