[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$BridgeProject,
    [Parameter(Mandatory)] [string]$Rid,
    [Parameter(Mandatory)] [string]$Version,
    [Parameter(Mandatory)] [string]$OutputRoot,
    [string]$BridgeBinary
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path $OutputRoot).Path
$name = if ($env:OS -eq 'Windows_NT') { 'officeimo-notecraft.exe' } else { 'officeimo-notecraft' }
$target = Join-Path $root 'importers\officeimo'
$publish = Join-Path $target $Rid
New-Item -ItemType Directory -Force -Path $publish | Out-Null

if ($BridgeBinary) {
    $published = Join-Path $publish $name
    Copy-Item -LiteralPath (Resolve-Path $BridgeBinary).Path -Destination $published -Force
} else {
    $project = (Resolve-Path $BridgeProject).Path
    dotnet publish $project -c Release -f net8.0 -r $Rid -p:TargetFrameworks=net8.0 -p:TargetFramework=net8.0 -p:RunAnalyzers=false --self-contained true -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -p:DebugType=None -o $publish
    $published = Join-Path $publish $name
    if (-not (Test-Path $published)) {
        $candidate = Get-ChildItem -LiteralPath $publish -Filter '*.exe' -File | Select-Object -First 1
        if (-not $candidate) { throw "publish did not produce an executable under $publish" }
        Move-Item -LiteralPath $candidate.FullName -Destination $published -Force
    }
}
$binary = $published
$hash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest = [ordered]@{
    bridge = 'OfficeIMO'
    version = $Version
    rid = $Rid
    executable = "importers/officeimo/$Rid/$name"
    sha256 = $hash
    license = 'OfficeIMO and transitive dependency licenses must ship beside this manifest.'
}
$manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $target 'manifest.json') -Encoding utf8
Write-Output "OfficeIMO bundle: $binary"
Write-Output "SHA-256: $hash"
