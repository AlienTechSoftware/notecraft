# NoteCraft Developer Setup - Windows
# Run: .\SKILLS\nc-devx\scripts\setup.ps1

param([switch]$SkipBuild = $false)

Write-Host "NoteCraft Developer Setup (Windows)" -ForegroundColor Cyan
Write-Host "===================================" -ForegroundColor Cyan
Write-Host ""

Write-Host "Step 1: Checking Rust..." -ForegroundColor Green
if (Get-Command rustc -ErrorAction SilentlyContinue) {
    $ver = rustc --version
    Write-Host "OK Rust installed" -ForegroundColor Green
} else {
    Write-Host "FAIL Rust not found. Install from https://rustup.rs" -ForegroundColor Red
}
Write-Host ""

Write-Host "Step 2: Enabling git hooks..." -ForegroundColor Green
git config core.hooksPath .githooks 2>$null
if ($LASTEXITCODE -eq 0) {
    Write-Host "OK Git hooks enabled" -ForegroundColor Green
} else {
    Write-Host "FAIL Git hooks setup failed" -ForegroundColor Red
}
Write-Host ""

Write-Host "Step 3: Building xtask (dev helper)..." -ForegroundColor Green
cargo build -p xtask 2>&1 | Out-Null
if ($LASTEXITCODE -eq 0) {
    Write-Host "OK xtask built successfully" -ForegroundColor Green
} else {
    Write-Host "INFO xtask skipped (may already exist)" -ForegroundColor Yellow
}
Write-Host ""

Write-Host "Setup Complete!" -ForegroundColor Green
Write-Host "===============" -ForegroundColor Green
Write-Host ""
Write-Host "Quick Start:" -ForegroundColor Cyan
Write-Host "  cargo run -p notecraft        (Run the app)" -ForegroundColor White
Write-Host "  cargo test --workspace        (Run tests)" -ForegroundColor White
Write-Host "  cargo xtask ci                (Full CI gate)" -ForegroundColor White
Write-Host ""
