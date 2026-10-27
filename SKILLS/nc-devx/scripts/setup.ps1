# NoteCraft developer setup and check - Windows
# Run from the repository root: .\SKILLS\nc-devx\scripts\setup.ps1
#
# Checks each tool NoteCraft needs. On a machine that is already set up it only reports, all OK.
# What is missing is installed:
#   - the MSVC C++ build tools (the Rust linker on Windows), through winget
#   - rustup, and the Rust named in rust-toolchain.toml with clippy and rustfmt
#   - with -Packaging: a .NET 8 or later SDK and WiX 5 with its UI and Util extensions
# Then it turns the git hooks on and builds xtask.
#
#   -SkipRust    leave Rust alone
#   -SkipBuild   do not build xtask
#   -Packaging   also check what packaging/windows/package.ps1 needs
#   -CheckOnly   report what is missing, change nothing
#
# Installing the build tools or the .NET SDK asks for administrator rights (a UAC prompt).
# Exit status: 0 when everything needed is there, 1 otherwise.

param(
    [switch]$SkipRust,
    [switch]$SkipBuild,
    [switch]$Packaging,
    [switch]$CheckOnly
)

# Not 'Stop': Windows PowerShell 5.1 turns any stderr line from a native tool (rustup, wix) into
# an error. Native calls are judged by their exit code or by checking again afterwards.
$ErrorActionPreference = 'Continue'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$WixVersion = '5.0.2'
$script:Failed = @()

function Ok($text) { Write-Host "OK   $text" -ForegroundColor Green }
function Info($text) { Write-Host "...  $text" -ForegroundColor Yellow }
function Bad($text) { Write-Host "FAIL $text" -ForegroundColor Red; $script:Failed += $text }
function Step($text) { Write-Host ""; Write-Host $text -ForegroundColor Cyan }

function Have($name) { [bool](Get-Command $name -ErrorAction SilentlyContinue) }

# Picks up PATH changes an installer made, so this window can use the new tools.
function Update-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = "$machine;$user"
    foreach ($dir in @((Join-Path $env:USERPROFILE '.cargo\bin'), (Join-Path $env:USERPROFILE '.dotnet\tools'))) {
        if ((Test-Path $dir) -and ($env:Path -split ';' -notcontains $dir)) { $env:Path = "$dir;$env:Path" }
    }
}

function Winget-Install($id, [string[]]$extra) {
    if (-not (Have winget)) {
        Bad "winget is not available to install $id. Install 'App Installer' from the Microsoft Store, or install $id by hand"
        return
    }
    Info "installing $id with winget"
    $wingetArgs = @('install', '--id', $id, '--exact', '--source', 'winget', '--accept-package-agreements', '--accept-source-agreements') + $extra
    & winget @wingetArgs
    Update-Path
}

function Has-MsvcTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $false }
    $found = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$found
}

# The newest .NET SDK major version, or 0 when there is none.
function DotNet-SdkMajor {
    if (-not (Have dotnet)) { return 0 }
    $majors = & dotnet --list-sdks 2>$null | ForEach-Object { if ($_ -match '^(\d+)\.') { [int]$Matches[1] } }
    if (-not $majors) { return 0 }
    return ($majors | Measure-Object -Maximum).Maximum
}

function Wix-Ok { (Have wix) -and ((& wix --version 2>$null) -like "$WixVersion*") }

Write-Host "NoteCraft developer setup (Windows)" -ForegroundColor Cyan
if ($CheckOnly) { Write-Host "Check only: nothing is installed or changed." }
Set-Location $Root
Update-Path

Step "Git"
if (Have git) { Ok (git --version) } else { Bad "git is not installed: winget install --id Git.Git" }

Step "MSVC C++ build tools"
if (Has-MsvcTools) {
    Ok "found"
} elseif ($CheckOnly) {
    Bad "missing. Install the Visual Studio Build Tools with the 'Desktop development with C++' workload"
} else {
    Winget-Install 'Microsoft.VisualStudio.2022.BuildTools' @('--override', '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
    if (Has-MsvcTools) { Ok "installed" } else { Bad "the C++ build tools are still missing after the install" }
}

if (-not $SkipRust) {
    Step "Rust"
    $channel = (Select-String -Path (Join-Path $Root 'rust-toolchain.toml') -Pattern '^\s*channel\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
    if (-not (Have rustup) -and -not $CheckOnly) {
        $init = Join-Path $env:TEMP 'rustup-init.exe'
        Info "downloading rustup-init.exe from static.rust-lang.org"
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $init
        & $init -y --default-toolchain none --profile minimal
        Remove-Item -Force $init -ErrorAction SilentlyContinue
        Update-Path
    }
    if (Have rustup) {
        Ok ((& rustup --version 2>$null) | Select-Object -First 1)
        $installed = & rustup toolchain list 2>$null | Where-Object { $_ -like "$channel-*" }
        if ($installed) {
            Ok "Rust $channel (pinned in rust-toolchain.toml)"
        } elseif ($CheckOnly) {
            Bad "Rust $channel is not installed: rustup toolchain install $channel"
        } else {
            Info "installing Rust $channel with clippy and rustfmt"
            & rustup toolchain install $channel --profile minimal --component clippy --component rustfmt
            if ($LASTEXITCODE -eq 0) { Ok "Rust $channel" } else { Bad "rustup could not install Rust $channel" }
        }
    } else {
        Bad "rustup is not installed (https://rustup.rs)"
    }
}

if ($Packaging) {
    Step ".NET SDK (for the OfficeIMO bridge and WiX)"
    $major = DotNet-SdkMajor
    if ($major -ge 8) {
        Ok ".NET SDK $major"
    } elseif ($CheckOnly) {
        Bad "no .NET 8 or later SDK: winget install --id Microsoft.DotNet.SDK.8"
    } else {
        Winget-Install 'Microsoft.DotNet.SDK.8' @()
        if ((DotNet-SdkMajor) -ge 8) { Ok ".NET SDK installed" } else { Bad ".NET SDK is still missing after the install" }
    }

    Step "WiX $WixVersion"
    if (Wix-Ok) {
        Ok "wix $WixVersion"
    } elseif ($CheckOnly) {
        Bad "WiX $WixVersion is missing: dotnet tool install --global wix --version $WixVersion"
    } elseif ((DotNet-SdkMajor) -ge 8) {
        Info "installing WiX $WixVersion as a .NET tool"
        if (Have wix) { & dotnet tool update --global wix --version $WixVersion } else { & dotnet tool install --global wix --version $WixVersion }
        Update-Path
        if (Wix-Ok) { Ok "wix $WixVersion" } else { Bad "WiX $WixVersion is still missing after the install" }
    } else {
        Bad "WiX needs the .NET SDK first"
    }
    if (Have wix) {
        $extensions = (& wix extension list -g 2>$null) -join "`n"
        foreach ($ext in @('WixToolset.UI.wixext', 'WixToolset.Util.wixext')) {
            if ($extensions -match [regex]::Escape("$ext $WixVersion")) {
                Ok "$ext $WixVersion"
            } elseif ($CheckOnly) {
                Bad "$ext is missing: wix extension add -g $ext/$WixVersion"
            } else {
                & wix extension add -g "$ext/$WixVersion"
                if ($LASTEXITCODE -eq 0) { Ok "$ext $WixVersion" } else { Bad "could not add $ext" }
            }
        }
    }
}

Step "Git hooks"
$hooks = git config core.hooksPath
if ($hooks -eq '.githooks') {
    Ok "on (core.hooksPath = .githooks)"
} elseif ($CheckOnly) {
    Bad "off: git config core.hooksPath .githooks"
} else {
    git config core.hooksPath .githooks
    if ($LASTEXITCODE -eq 0) { Ok "turned on (core.hooksPath = .githooks)" } else { Bad "could not set core.hooksPath" }
}

if (-not $SkipBuild -and -not $CheckOnly) {
    Step "xtask"
    if (Have cargo) {
        & cargo build -q -p xtask
        if ($LASTEXITCODE -eq 0) { Ok "builds" } else { Bad "cargo build -p xtask failed" }
    } else {
        Bad "cargo is not on PATH, so xtask was not built"
    }
}

Write-Host ""
if ($script:Failed.Count -eq 0) {
    Write-Host "All green." -ForegroundColor Green
    Write-Host "  cargo run -p notecraft      run the app"
    Write-Host "  cargo xtask ci              the full gate"
    Write-Host "Open a new terminal if a tool was installed, so it is on PATH there too."
    exit 0
}
Write-Host "Not ready:" -ForegroundColor Red
$script:Failed | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
exit 1
