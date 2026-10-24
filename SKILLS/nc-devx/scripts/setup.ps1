# NoteCraft Developer Setup - Windows (PowerShell)
# Run: .\SKILLS\nc-devx\scripts\setup.ps1

param(
    [switch]$SkipRust = $false,
    [switch]$SkipTools = $false
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
        Write-Host "Installing Rust from rustup.rs..." -ForegroundColor Yellow
        Write-Host "This will open a browser to download rustup."
        Start-Process "https://rustup.rs"
        Write-Host "Please complete the Rust installation and re-run this script." -ForegroundColor Yellow
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
        Write-Host "ⓘ MSVC (C++ compiler) not found in PATH" -ForegroundColor Yellow
        Write-Host "  Install Visual Studio Build Tools or Visual Studio Community edition" -ForegroundColor Yellow
        Write-Host "  https://visualstudio.microsoft.com/downloads/" -ForegroundColor Cyan
    } else {
        Write-Host "✓ MSVC compiler found" -ForegroundColor Green
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

# Step 4: Verify Rust build
Write-Host "Step 4: Verifying build..." -ForegroundColor Green
cargo build -p notecraft 2>&1 | Select-Object -Last 5
if ($LASTEXITCODE -eq 0) {
    Write-Host "✓ Build successful" -ForegroundColor Green
} else {
    Write-Host "✗ Build failed. Check the output above." -ForegroundColor Red
}
Write-Host ""

# Step 5: Run quick test
Write-Host "Step 5: Running quick test..." -ForegroundColor Green
cargo test --workspace -- --test-threads=1 2>&1 | Tail -n 3
if ($LASTEXITCODE -eq 0) {
    Write-Host "✓ Tests passed" -ForegroundColor Green
} else {
    Write-Host "ⓘ Some tests failed or timed out (may be OK for first run)" -ForegroundColor Yellow
}
Write-Host ""

# Summary
Write-Host "Setup Complete!" -ForegroundColor Green
Write-Host "===============" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Run the app:     cargo run -p notecraft" -ForegroundColor White
Write-Host "  2. Run tests:       cargo test --workspace" -ForegroundColor White
Write-Host "  3. Check gates:     cargo xtask ci" -ForegroundColor White
Write-Host ""
Write-Host "For more info, see SKILLS/nc-devx/SKILL.md" -ForegroundColor Cyan
