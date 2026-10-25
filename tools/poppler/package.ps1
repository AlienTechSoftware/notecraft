<#
.SYNOPSIS
  Downloads and stages the pinned Windows Poppler runtime used for printouts.
#>
param(
  [Parameter(Mandatory = $true)]
  [string] $OutputRoot
)
$ErrorActionPreference = 'Stop'
$Version = '26.09.0-0'
$Url = "https://github.com/oschwartz10612/poppler-windows/releases/download/v$Version/Release-$Version.zip"
$Sha256 = '7a6f256a0ddf7536182246a5733331bf4677cbcc34f4663774947ad34556c8d0'
$download = Join-Path $env:TEMP "notecraft-poppler-$Version.zip"
$extract = Join-Path $env:TEMP "notecraft-poppler-$Version"
$destination = Join-Path $OutputRoot 'renderers\poppler'

if (-not (Test-Path -LiteralPath $download)) { Invoke-WebRequest -Uri $Url -OutFile $download }
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $download).Hash.ToLowerInvariant() -ne $Sha256) {
  throw "Poppler archive checksum mismatch: $download"
}
Remove-Item -Recurse -Force -LiteralPath $extract -ErrorAction SilentlyContinue
Expand-Archive -LiteralPath $download -DestinationPath $extract
$library = Get-ChildItem -LiteralPath $extract -Directory -Recurse | Where-Object Name -eq 'Library' | Select-Object -First 1
if (-not $library) { throw "Poppler archive did not contain a Library folder" }
Remove-Item -Recurse -Force -LiteralPath $destination -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $destination | Out-Null
Copy-Item -LiteralPath $library.FullName -Destination $destination -Recurse
Write-Output "Poppler $Version staged at $destination"
