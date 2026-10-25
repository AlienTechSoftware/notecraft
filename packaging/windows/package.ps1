<#
.SYNOPSIS
  Build, sign and package NoteCraft for Windows.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    notecraft-<version>-windows-x64.msi            per-machine installer (WiX v5)
    notecraft-<version>-windows-x64-portable.zip   notecraft.exe + notecraft-cli.exe
    SHA256SUMS.txt                                 checksums of the files above

  The binaries link the C runtime statically (+crt-static), so neither the MSI nor the portable
  zip needs the Visual C++ redistributable. Signing is delegated to sign.ps1 (skipped with a
  warning when no signing secrets are set).

  Only x64 is built. Other targets are untested.

  Needs: Rust (MSVC toolchain), the Windows SDK (signtool.exe, only when signing), and WiX v5
  with its UI and Util extensions (installer dialogs, "Launch NoteCraft" on the finish page):
    dotnet tool install --global wix --version 5.0.2
    wix extension add -g WixToolset.UI.wixext/5.0.2
    wix extension add -g WixToolset.Util.wixext/5.0.2

.EXAMPLE
  pwsh packaging/windows/package.ps1
  pwsh packaging/windows/package.ps1 -SkipBuild
  pwsh packaging/windows/package.ps1 -ZipOnly      # no WiX needed
#>
param(
  [switch] $SkipBuild,
  [switch] $ZipOnly,
  [string] $BridgeProject = (Join-Path $PSScriptRoot '..\..\tools\officeimo\OfficeIMO.NoteCraft.Bridge\OfficeIMO.NoteCraft.Bridge.csproj'),
  [string] $BridgeBinary = $env:NOTECRAFT_OFFICEIMO_BRIDGE
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Arch = 'x64'
$Target = 'x86_64-pc-windows-msvc'

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

# The version lives in one place: [workspace.package] version in the root Cargo.toml.
# NOTECRAFT_VERSION overrides it for a test run (the release workflow sets it).
$Version = $env:NOTECRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
# MSI ProductVersion is numeric (major.minor.build); pre-release tags are dropped there.
$MsiVersion = ($Version -split '-')[0]

$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Write-Output "NoteCraft $Version for Windows $Arch ($Target)"

if (-not $SkipBuild) {
  # Static CRT: no VC++ redistributable needed. Scoped to the target so host build scripts and
  # proc-macros are unaffected.
  $flagVar = 'CARGO_TARGET_' + ($Target.ToUpper() -replace '-', '_') + '_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p notecraft -p notecraft-cli --target $Target }
}

$Bin = Join-Path $TargetDir "$Target\release"

# Check both binaries' PE headers before packaging. Machine (COFF header) must be x64.
# Subsystem (optional header): 2 = Windows GUI, 3 = console. The app must be GUI, so no console
# window opens with it; the CLI must stay console so its output reaches the terminal.
function Get-PeHeader([string] $Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  return @{ Machine = [BitConverter]::ToUInt16($bytes, $pe + 4); Subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C) }
}
foreach ($check in @(@('notecraft.exe', 2), @('notecraft-cli.exe', 3))) {
  $h = Get-PeHeader (Join-Path $Bin $check[0])
  if ($h.Machine -ne 0x8664) { throw "$($check[0]) is for machine 0x$('{0:X}' -f $h.Machine), expected 0x8664 ($Arch)" }
  if ($h.Subsystem -ne $check[1]) { throw "$($check[0]) has PE subsystem $($h.Subsystem), expected $($check[1])" }
  Write-Output "ok $($check[0]): $Arch, PE subsystem $($h.Subsystem)"
}
# Both programs must carry the app icon (a group-icon resource) and the product name, or Explorer
# shows the default file icon and the Properties dialog is empty. build.rs embeds them.
Add-Type -Namespace Nc -Name Res -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr LoadLibraryEx(string file, System.IntPtr h, uint flags);
[System.Runtime.InteropServices.DllImport("kernel32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr FindResource(System.IntPtr m, System.IntPtr name, System.IntPtr type);
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern bool FreeLibrary(System.IntPtr m);
'@
foreach ($name in @('notecraft.exe', 'notecraft-cli.exe')) {
  $file = Join-Path $Bin $name
  $m = [Nc.Res]::LoadLibraryEx($file, [IntPtr]::Zero, 0x22)  # as data file, as image resource
  if ($m -eq [IntPtr]::Zero) { throw "could not open $name to check its resources" }
  $icon = [Nc.Res]::FindResource($m, [IntPtr]1, [IntPtr]14)  # resource 1, RT_GROUP_ICON
  [void][Nc.Res]::FreeLibrary($m)
  if ($icon -eq [IntPtr]::Zero) { throw "$name has no icon resource: build.rs did not embed assets/app-icon/notecraft.ico" }
  $product = (Get-Item $file).VersionInfo.ProductName
  if ($product -ne 'NoteCraft') { throw "$name has product name '$product' in its version information, expected NoteCraft" }
  Write-Output "ok ${name}: icon and version information"
}
$Stage = Join-Path $TargetDir "windows-package\$Arch"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'notecraft.exe'), (Join-Path $Bin 'notecraft-cli.exe') $Stage

# The importer is part of the Windows product, not an optional file that can silently be
# omitted from a release. Build and stage the self-contained OfficeIMO bridge beside NoteCraft.
$BridgeStage = Join-Path $TargetDir 'officeimo-package'
Remove-Item -Recurse -Force $BridgeStage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $BridgeStage | Out-Null
$LASTEXITCODE = 0
& (Join-Path $Root 'tools\officeimo\package.ps1') -BridgeProject $BridgeProject -BridgeBinary $BridgeBinary -Rid 'win-x64' -Version $Version -OutputRoot $BridgeStage
if ($LASTEXITCODE -ne 0) { throw "OfficeIMO bridge packaging failed with exit code $LASTEXITCODE" }
$BridgeExe = Join-Path $BridgeStage 'importers\officeimo\win-x64\officeimo-notecraft.exe'
if (-not (Test-Path $BridgeExe)) { throw "OfficeIMO bridge was not produced: $BridgeExe" }
Copy-Item (Join-Path $BridgeStage 'importers') $Stage -Recurse

# PDF printouts use the same self-contained runtime approach as the OfficeIMO importer.
$PopplerStage = Join-Path $TargetDir 'poppler-package'
Remove-Item -Recurse -Force $PopplerStage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $PopplerStage | Out-Null
& (Join-Path $Root 'tools\poppler\package.ps1') -OutputRoot $PopplerStage
if ($LASTEXITCODE -ne 0) { throw "Poppler packaging failed with exit code $LASTEXITCODE" }
Copy-Item (Join-Path $PopplerStage 'renderers') $Stage -Recurse

& (Join-Path $PSScriptRoot 'sign.ps1') (Join-Path $Stage 'notecraft.exe') (Join-Path $Stage 'notecraft-cli.exe')

$Outputs = @()

# ---- MSI ---------------------------------------------------------------------------------------
if (-not $ZipOnly) {
  # The installer's welcome page shows the licence: build its RTF from LICENSE-MIT and
  # LICENSE-APACHE so it never drifts from them. Hard-wrapped lines are rejoined into paragraphs
  # so the text wraps to the dialog's width.
  function ConvertTo-RtfText([string] $Text) {
    $sb = [System.Text.StringBuilder]::new()
    foreach ($ch in $Text.ToCharArray()) {
      $c = [int] $ch
      if ($c -eq 0x5C -or $c -eq 0x7B -or $c -eq 0x7D) { [void] $sb.Append('\').Append($ch) }
      elseif ($c -gt 127) { [void] $sb.Append("\u$(if ($c -gt 32767) { $c - 65536 } else { $c })?") }
      else { [void] $sb.Append($ch) }
    }
    return $sb.ToString()
  }
  $paragraphs = [System.Collections.Generic.List[string]]::new()
  $paragraphs.Add('\b NoteCraft\b0')
  $paragraphs.Add((ConvertTo-RtfText 'NoteCraft is dual-licensed under the MIT License or the Apache License, Version 2.0, at your option. Both licence texts follow. Bundled fonts and other third-party material keep their own open licences, listed in NOTICE and ATTRIBUTION.md at https://github.com/AlienTechSoftware/notecraft.'))
  foreach ($file in 'LICENSE-MIT', 'LICENSE-APACHE') {
    $title = $true  # each file opens with its licence's name: show it bold
    foreach ($para in ((Get-Content -Raw (Join-Path $Root $file)) -split '\r?\n\s*\r?\n')) {
      $joined = (($para -split '\r?\n') | ForEach-Object { $_.Trim() } | Where-Object { $_ }) -join ' '
      if (-not $joined) { continue }
      $paragraphs.Add($(if ($title) { "\b $(ConvertTo-RtfText $joined)\b0" } else { ConvertTo-RtfText $joined }))
      $title = $false
    }
  }
  $LicenseRtf = Join-Path $Stage 'license.rtf'
  $rtf = '{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fswiss\fcharset0 Tahoma;}}\viewkind4\uc1\pard\sa120\f0\fs16 ' +
    ($paragraphs -join "\par`r`n") + "\par`r`n}`r`n"
  [System.IO.File]::WriteAllText($LicenseRtf, $rtf, [System.Text.Encoding]::ASCII)

  $Msi = Join-Path $Dist "notecraft-$Version-windows-$Arch.msi"
  Invoke-Native 'wix build' {
    wix build (Join-Path $PSScriptRoot 'notecraft.wxs') -arch $Arch `
      -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
      -d "Version=$MsiVersion" -d "BinDir=$Stage" -d "IconPath=$(Join-Path $Root 'assets\app-icon\notecraft.ico')" `
      -d "LicenseRtf=$LicenseRtf" `
      -o $Msi
  }
  # wix writes its debug symbols (.wixpdb) next to the MSI; keep them out of the release assets.
  Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
  & (Join-Path $PSScriptRoot 'sign.ps1') $Msi
  $Outputs += $Msi
}

# ---- portable zip ------------------------------------------------------------------------------
$Portable = Join-Path $TargetDir "windows-package\notecraft-$Version-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage '*.exe') $Portable
Copy-Item (Join-Path $Stage 'importers') $Portable -Recurse
foreach ($f in 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE', 'ATTRIBUTION.md') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Portable }
}
$Zip = Join-Path $Dist "notecraft-$Version-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip
$Outputs += $Zip

# Smoke-test the CLI: it must start and report the version it was built as.
Invoke-Native 'notecraft-cli --version' { & (Join-Path $Stage 'notecraft-cli.exe') --version }

# ---- checksums ---------------------------------------------------------------------------------
$lines = foreach ($f in $Outputs) { "$((Get-FileHash -Algorithm SHA256 $f).Hash.ToLower())  $(Split-Path $f -Leaf)" }
[System.IO.File]::WriteAllText((Join-Path $Dist 'SHA256SUMS.txt'), (($lines -join "`n") + "`n"), [System.Text.UTF8Encoding]::new($false))
Get-Item $Outputs | Format-Table Name, Length
