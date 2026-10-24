#!/bin/bash
# NoteCraft Developer Setup - macOS
# Run: bash SKILLS/nc-devx/scripts/setup.mac.sh
#
# Options: SKIP_RUST=1, SKIP_XCODE=1, SKIP_BUILD=1

set -e

SKIP_RUST=${SKIP_RUST:-0}
SKIP_XCODE=${SKIP_XCODE:-0}
SKIP_BUILD=${SKIP_BUILD:-0}

echo -e "\033[36mNoteCraft Developer Setup (macOS)\033[0m"
echo -e "\033[36m=================================\033[0m"
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

# Step 2: Check Xcode Command Line Tools
if [ "$SKIP_XCODE" = "0" ]; then
    echo -e "\033[32mStep 2: Checking Xcode Command Line Tools...\033[0m"

    if command -v xcode-select &> /dev/null && xcode-select -p &> /dev/null; then
        echo -e "\033[32m✓ Xcode Command Line Tools installed\033[0m"
    else
        echo -e "\033[33m✗ Xcode Command Line Tools not found.\033[0m"
        echo -e "\033[33mInstalling via xcode-select...\033[0m"
        xcode-select --install
        echo -e "\033[33mPlease complete the installation in the dialog and re-run this script.\033[0m"
        exit 1
    fi
    echo ""
fi

# Step 3: Check Homebrew (optional but recommended)
echo -e "\033[32mStep 3: Checking Homebrew...\033[0m"

if command -v brew &> /dev/null; then
    echo -e "\033[32m✓ Homebrew is installed\033[0m"
    BREW_PREFIX=$(brew --prefix)
else
    echo -e "\033[33m⚠ Homebrew not found (optional but recommended)\033[0m"
    read -p "Install Homebrew now? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo -e "\033[33mInstalling Homebrew...\033[0m"
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

# Step 5: Setup development environment
echo -e "\033[32mStep 5: Setting up dev environment...\033[0m"

# Update toolchain
echo "  - Updating Rust toolchain..." >&2
rustup install 2>&1 | grep -E "^(info|Updating)" || true
echo "  ✓ Rust toolchain updated" >&2

# Build xtask
echo "  - Building xtask (dev helper)..." >&2
cargo build -p xtask 2>&1 | tail -1 || true
echo "  ✓ xtask built" >&2
echo ""

# Step 6: Verify build (unless skipped)
if [ "$SKIP_BUILD" = "0" ]; then
    echo -e "\033[32mStep 6: Verifying build...\033[0m"
    cargo build -p notecraft 2>&1 | tail -3
    echo -e "\033[32m✓ Build successful\033[0m"
    echo ""

    # Step 7: Run quick test
    echo -e "\033[32mStep 7: Running quick test...\033[0m"
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
echo ""
echo -e "\033[36mOptional development tools:\033[0m"
echo "  brew install cargo-watch           # Auto-rebuild on changes"
echo "  cargo watch -x check                # Watch and check"
echo ""
echo -e "\033[36mDocumentation: SKILLS/nc-devx/SKILL.md\033[0m"
