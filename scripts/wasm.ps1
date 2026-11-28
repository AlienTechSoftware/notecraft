# wasm.ps1 - build the browser (WASM) version of NoteCraft and serve it on this computer.
#
# Called by run.cmd:  run.cmd --wasm build | start | stop
#
#   build  Clones the current commit to local\wasm\src (the working tree is never changed), then
#          runs the website pipeline's steps there: the web patches, trunk, wasm-opt, the integrity
#          rehash and the demo bundle. The result is local\wasm\dist.
#   start  Serves local\wasm\dist on http://127.0.0.1:8080/ in the background.
#   stop   Stops that server.
#
# Needs: trunk (cargo install trunk), Python 3 with pyyaml, and wasm-opt. wasm-opt is found on PATH,
# or downloaded once into local\wasm\tools (binaryen version_133, as the pipeline pins it).

param(
    [Parameter(Mandatory = $true)][ValidateSet('build', 'start', 'stop')][string]$Action,
    [int]$Port = 8080
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$wasm = Join-Path $repo 'local\wasm'
$src = Join-Path $wasm 'src'
$dist = Join-Path $wasm 'dist'
$pidFile = Join-Path $wasm 'server.pid'
New-Item -ItemType Directory -Force -Path $wasm | Out-Null

function Get-WasmOpt {
    $found = Get-Command wasm-opt -ErrorAction SilentlyContinue
    if ($found) { return $found.Source }
    $tools = Join-Path $wasm 'tools'
    $exe = Join-Path $tools 'binaryen-version_133\bin\wasm-opt.exe'
    if (-not (Test-Path $exe)) {
        New-Item -ItemType Directory -Force -Path $tools | Out-Null
        $archive = Join-Path $tools 'binaryen.tar.gz'
        Invoke-WebRequest -Uri 'https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-windows.tar.gz' -OutFile $archive -UseBasicParsing
        tar -xzf $archive -C $tools
    }
    return $exe
}

function Invoke-Build {
    if (-not (Test-Path (Join-Path $src '.git'))) {
        git clone --quiet --local $repo $src
    }
    # The clone follows the current commit of this checkout.
    $head = git -C $repo rev-parse HEAD
    git -C $src fetch --quiet $repo HEAD
    git -C $src checkout --quiet --force $head
    git -C $src clean -fdq

    Push-Location $src
    try {
        python .github/web-build/apply-web-diff.py . .github/web-build/web.diff
        python .github/web-build/apply-web-patches.py . .github/web-build/patches.yaml

        # Wasm SIMD, as the pipeline sets it.
        New-Item -ItemType Directory -Force -Path '.cargo' | Out-Null
        Add-Content -Path '.cargo\config.toml' -Value "[target.wasm32-unknown-unknown]`nrustflags = [`"-Ctarget-feature=+simd128`"]"

        $demo = Join-Path $src 'website\demo'
        if (Test-Path $demo) { Remove-Item -Recurse -Force $demo }
        Push-Location 'apps\notecraft-web'
        try { trunk build --release --dist $demo --public-url ./ } finally { Pop-Location }

        $wasmOpt = Get-WasmOpt
        foreach ($file in Get-ChildItem (Join-Path $demo '*.wasm')) {
            & $wasmOpt -O3 --strip-debug --enable-bulk-memory --enable-bulk-memory-opt --enable-call-indirect-overlong `
                --enable-multivalue --enable-mutable-globals --enable-nontrapping-float-to-int --enable-reference-types `
                --enable-sign-ext --enable-simd --enable-tail-call $file.FullName -o $file.FullName
        }
        python .github/web-build/rehash_wasm_integrity.py website/demo/index.html website/demo

        $version = (python -c "import tomllib; print(tomllib.load(open('Cargo.toml','rb'))['workspace']['package']['version'])").Trim()
        python .github/web-build/bundle_demo.py docs/demo/DemoNotebook website/demo/welcome-notebook.zip --version $version
    } finally {
        Pop-Location
    }

    if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
    Copy-Item -Recurse (Join-Path $src 'website\demo') $dist
    Write-Host "Built the web version into $dist"
}

function Get-ServerPid {
    if (-not (Test-Path $pidFile)) { return $null }
    $id = [int](Get-Content $pidFile -Raw)
    if (Get-Process -Id $id -ErrorAction SilentlyContinue) { return $id }
    return $null
}

function Start-WasmServer {
    if (-not (Test-Path $dist)) { throw "Nothing built yet: run.cmd --wasm build first." }
    if (Get-ServerPid) { Write-Host "Already running: http://127.0.0.1:$Port/"; return }
    $server = Start-Process -FilePath 'python' -ArgumentList '-m', 'http.server', "$Port", '--bind', '127.0.0.1' -WorkingDirectory $dist -PassThru -WindowStyle Hidden
    Set-Content -Path $pidFile -Value $server.Id
    Start-Sleep -Seconds 2
    Write-Host "Serving the web version at http://127.0.0.1:$Port/ (stop with run.cmd --wasm stop)"
}

function Stop-WasmServer {
    $id = Get-ServerPid
    if ($id) {
        Stop-Process -Id $id -Force
        Write-Host "Stopped the web server."
    } else {
        Write-Host "The web server is not running."
    }
    if (Test-Path $pidFile) { Remove-Item $pidFile }
}

switch ($Action) {
    'build' { Invoke-Build }
    'start' { Start-WasmServer }
    'stop'  { Stop-WasmServer }
}
