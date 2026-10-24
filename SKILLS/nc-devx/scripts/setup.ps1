# NoteCraft Developer Setup - Windows (PowerShell)
# Run: .\SKILLS\nc-devx\scripts\setup.ps1
#
# Options:
#   -SkipRust    Skip Rust installation check
#   -SkipTools   Skip platform tools check
#   -SkipBuild   Skip build and test steps

param(
    [switch]$SkipRust = $false,
    [switch]$SkipTools = $false,
    [switch]$SkipBuild = $false
)

Write-Host "NoteCraft Developer Setup (Windows)" -ForegroundColor Cyan
Write-Host "===================================" -ForegroundColor Cyan
Write-Host ""

# Check if running as admin (recommended for some tools)
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")
if (-not $isAdmin) {
    Write-Host "ⓘ Not running as administrator. Some tools may fail to install." -ForegroundColor Yellow
    Write-Host "  Recommendation: Right-click PowerShell and select 'Run as administrator'" -ForegroundColor Yellow
    Write-Host ""
}

# Step 1: Install Rust
if (-not $SkipRust) {
    Write-Host "Step 1: Checking Rust..." -ForegroundColor Green

    if (Get-Command rustc -ErrorAction SilentlyContinue) {
        $rustVersion = rustc --version
        Write-Host "✓ Rust is installed: $rustVersion" -ForegroundColor Green
    } else {
        Write-Host "✗ Rust not found. Installing from rustup.rs..." -ForegroundColor Yellow
        Write-Host "  Opening https://rustup.rs in your browser..." -ForegroundColor Cyan
        Start-Process "https://rustup.rs"
        Write-Host "  Please complete the installation and re-run this script." -ForegroundColor Yellow
        exit 1
    }
    Write-Host ""
}

# Step 2: Install platform tools
if (-not $SkipTools) {
    Write-Host "Step 2: Checking platform tools..." -ForegroundColor Green

    # Check for Visual Studio Build Tools or MSVC
    $msvcInstalled = $null -ne (Get-Command cl.exe -ErrorAction SilentlyContinue)
    if (-not $msvcInstalled) {
        Write-Host "⚠ MSVC (C++ compiler) not found in PATH" -ForegroundColor Yellow
        Write-Host "  Download Visual Studio Build Tools or Community edition:" -ForegroundColor Yellow
        Write-Host "  https://visualstudio.microsoft.com/downloads/" -ForegroundColor Cyan
        Write-Host "  Select 'Desktop development with C++'" -ForegroundColor Cyan
    } else {
        Write-Host "✓ MSVC compiler found" -ForegroundColor Green
    }

    # Check for git
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Host "⚠ Git not found. Install from https://git-scm.com/download/win" -ForegroundColor Yellow
    } else {
        Write-Host "✓ Git found" -ForegroundColor Green
    }

    Write-Host ""
}

# Step 3: Enable git hooks
Write-Host "Step 3: Enabling git hooks..." -ForegroundColor Green
git config core.hooksPath .githooks
if ($LASTEXITCODE -eq 0) {
    Write-Host "✓ Git hooks enabled" -ForegroundColor Green
} else {
    Write-Host "✗ Failed to enable git hooks" -ForegroundColor Red
}
Write-Host ""

# Step 4: Setup development environment
Write-Host "Step 4: Setting up dev environment..." -ForegroundColor Green

# Initialize rustup and update toolchain
Write-Host "  - Updating Rust toolchain..." -ForegroundColor Cyan
rustup install
if ($LASTEXITCODE -ne 0) {
    Write-Host "⚠ rustup install failed, continuing..." -ForegroundColor Yellow
} else {
    Write-Host "  ✓ Rust toolchain updated" -ForegroundColor Green
}

# Install xtask (build helper)
Write-Host "  - Building xtask (dev helper)..." -ForegroundColor Cyan
cargo build -p xtask 2>&1 | Out-Null
if ($LASTEXITCODE -eq 0) {
    Write-Host "  ✓ xtask built" -ForegroundColor Green
} else {
    Write-Host "  ⓘ xtask build skipped" -ForegroundColor Yellow
}

Write-Host ""

# Step 5: Verify build (unless skipped)
if (-not $SkipBuild) {
    Write-Host "Step 5: Verifying build..." -ForegroundColor Green
    cargo build -p notecraft 2>&1 | Select-Object -Last 3
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✓ Build successful" -ForegroundColor Green
    } else {
        Write-Host "✗ Build failed. Check the output above." -ForegroundColor Red
        Write-Host "  Try running: cargo build -p notecraft" -ForegroundColor Cyan
    }
    Write-Host ""

    # Step 6: Run quick test
    Write-Host "Step 6: Running quick test..." -ForegroundColor Green
    cargo test --workspace --lib -- --test-threads=1 2>&1 | Select-Object -Last 3
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✓ Tests passed" -ForegroundColor Green
    } else {
        Write-Host "ⓘ Some tests failed (may be expected on first run)" -ForegroundColor Yellow
    }
    Write-Host ""
}

# Summary
Write-Host "Setup Complete!" -ForegroundColor Green
Write-Host "===============" -ForegroundColor Green
Write-Host ""
Write-Host "Quick start commands:" -ForegroundColor Cyan
Write-Host "  cargo run -p notecraft          # Run the app" -ForegroundColor White
Write-Host "  cargo test --workspace          # Run all tests" -ForegroundColor White
Write-Host "  cargo xtask ci                  # Run full CI gate" -ForegroundColor White
Write-Host "  cargo watch -x check            # Watch and check on file changes" -ForegroundColor White
Write-Host ""
Write-Host "Documentation: SKILLS/nc-devx/SKILL.md" -ForegroundColor Cyan
