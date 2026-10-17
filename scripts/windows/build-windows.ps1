[CmdletBinding()]
param(
    [switch]$BuildCv,
    [string]$Python,
    [switch]$Unsigned
)
. (Join-Path $PSScriptRoot 'common.ps1')
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Run this script on Windows.' }
Push-Location $repoRoot
try {
    & (Join-Path $PSScriptRoot 'prepare-windows-adb.ps1')
    if ($BuildCv) {
        if ($Python) {
            Push-Location (Join-Path $repoRoot 'sidecar/cv-runtime')
            try { & $Python -m pytest } finally { Pop-Location }
            if ($LASTEXITCODE -ne 0) { throw 'CV tests failed.' }
        } else {
            Push-Location (Join-Path $repoRoot 'sidecar/cv-runtime')
            try {
                poetry install --no-root
                if ($LASTEXITCODE -ne 0) { throw 'Installing CV dependencies failed.' }
                poetry run pytest
                if ($LASTEXITCODE -ne 0) { throw 'CV tests failed.' }
            } finally { Pop-Location }
        }
        & (Join-Path $PSScriptRoot 'build-cv.ps1') -Python $Python -SkipInstall
    }
    pnpm test:scripts
    if ($LASTEXITCODE -ne 0) { throw 'Release-script tests failed.' }
    cargo test --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    if ($Unsigned) {
        $configPath = Join-Path $repoRoot 'src-tauri/target/windows-unsigned.json'
        New-Item -ItemType Directory -Path (Split-Path -Parent $configPath) -Force | Out-Null
        '{"bundle":{"createUpdaterArtifacts":false}}' | Set-Content -LiteralPath $configPath -Encoding ascii
        pnpm tauri build --bundles nsis --config $configPath
    } else {
        pnpm tauri build --bundles nsis
    }
    if ($LASTEXITCODE -ne 0) { throw 'Building the Windows installer failed.' }
} finally { Pop-Location }
