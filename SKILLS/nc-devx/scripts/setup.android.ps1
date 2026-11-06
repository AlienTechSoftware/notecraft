# Android setup launcher for Windows. Android builds run in Linux/WSL2.
param(
    [string]$Distro = 'Ubuntu-24.04',
    [switch]$CheckOnly
)

$ErrorActionPreference = 'Continue'
$scriptPath = (Resolve-Path (Join-Path $PSScriptRoot 'setup.android.sh')).Path
if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    Write-Error 'WSL is not installed. Install WSL2 and an Ubuntu distribution, then rerun this script.'
    exit 1
}

$linuxScript = (& wsl.exe -d $Distro --exec wslpath -a $scriptPath 2>$null | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($linuxScript)) {
    Write-Error "Could not access the Android setup script in WSL distribution '$Distro'."
    exit 1
}

$wslArgs = @('-d', $Distro, '--', 'bash', $linuxScript)
if ($CheckOnly) { $wslArgs += '--check-only' }
& wsl.exe @wslArgs
exit $LASTEXITCODE
