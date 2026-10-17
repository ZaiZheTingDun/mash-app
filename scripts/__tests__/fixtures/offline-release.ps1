$ErrorActionPreference = 'Stop'
$fixtureRoot = (Get-Location).Path
$env:R2_ENDPOINT = 'https://r2.example.invalid'
$env:R2_BUCKET = 'fixture'
$env:RELEASE_BASE_URL = 'https://cdn.example.invalid'
$env:TAURI_SIGNING_PRIVATE_KEY = 'fixture-key'
$env:R2_PREFIX = 'mash'
$global:uploadLog = Join-Path $fixtureRoot 'uploads.log'
$global:publishedManifest = $null
function rustc { $global:LASTEXITCODE = 0; 'host: x86_64-pc-windows-msvc' }
function cargo { $global:LASTEXITCODE = 0 }
function pnpm {
    if ($args[0] -eq 'tauri') {
        $config = Get-Content (Join-Path $fixtureRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
        $dir = Join-Path $fixtureRoot 'src-tauri/target/release/bundle/nsis'
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        $file = Join-Path $dir "Mash_$($config.version)_x64-setup.exe"
        Set-Content -LiteralPath $file -Value 'fixture installer' -Encoding ascii
        Set-Content -LiteralPath "$file.sig" -Value 'fixture signature' -Encoding ascii
    }
    $global:LASTEXITCODE = 0
}
function poetry {
    if ($args -contains 'build_runtime.py') {
        $dir = Join-Path $fixtureRoot 'sidecar/cv-runtime/dist'
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        $name = "mash-cv-runtime-windows-x86_64-v$env:MASH_CV_RUNTIME_VERSION.zip"
        Set-Content -LiteralPath (Join-Path $dir $name) -Value 'fixture zip' -Encoding ascii
    }
    $global:LASTEXITCODE = 0
}
function aws {
    if ($args[0] -eq 's3' -and $args[1] -eq 'cp') {
        Add-Content -LiteralPath $global:uploadLog -Value $args[3]
        if ($args[3] -like '*/releases/windows-stable/latest.json') { $global:publishedManifest = $args[2] }
    }
    $global:LASTEXITCODE = 0
}
function Invoke-WebRequest {
    param($Uri, $OutFile, $Method, [switch]$UseBasicParsing)
    if ($OutFile) {
        '{"version":"1.1.0","platforms":{"darwin-aarch64":{"url":"https://cdn.example.invalid/mac.tar.gz","signature":"mac signature"}}}' |
            Set-Content -LiteralPath $OutFile -Encoding ascii
    }
}
function Invoke-RestMethod {
    param($Uri)
    Get-Content -LiteralPath $global:publishedManifest -Raw | ConvertFrom-Json
}
& (Join-Path $fixtureRoot 'scripts/windows/release.ps1') 1.1.0 -CvRuntime 1.1.0 -Installer -Publish
