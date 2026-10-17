[CmdletBinding()]
param([Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version, [string]$Platform, [string]$Python)
. (Join-Path $PSScriptRoot 'common.ps1')
Push-Location $repoRoot
$previousVersion = $env:MASH_CV_RUNTIME_VERSION
try {
    $native = Get-WindowsPlatform
    if (-not $Platform) { $Platform = $native }
    if ($Platform -ne $native) { throw "Platform must match the native host ($native)." }
    Assert-CleanWorktree
    Assert-ReleaseEnvironment
    $tag = "cv-runtime/$Platform/$Version"
    Assert-NewTag $tag
    $current = (Invoke-Native node @($releaseState, 'versions', $Platform) | ConvertFrom-Json).runtime
    if ($current -match '^\d+\.\d+\.\d+$' -and [version]$Version -lt [version]$current) { throw 'CV runtime version cannot go backwards.' }
    $env:MASH_CV_RUNTIME_VERSION = $Version
    & (Join-Path $PSScriptRoot 'build-cv.ps1') -Mode runtime -Python $Python
    $artifact = "mash-cv-runtime-$Platform-v$Version.zip"
    $file = Join-Path $repoRoot "sidecar/cv-runtime/dist/$artifact"
    $sha = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    $key = "$(Get-ReleasePrefix)/runtime/mash-cv/runtime/$Platform/$artifact"
    Publish-Artifact $file $key
    Invoke-Native node @($releaseState, 'version', 'runtime', $Version, $Platform)
    Invoke-Native node @($releaseState, 'artifact', 'runtime', $Version, $Platform, "$($env:RELEASE_BASE_URL.TrimEnd('/'))/$key", $sha)
    Invoke-Native git @('add', '--', 'versions.toml', 'src-tauri/resources/runtime-manifest.json')
    Invoke-Native git @('commit', '-m', "chore(release): publish mash-cv runtime $Version ($Platform)")
    Invoke-Native git @('tag', $tag)
    Write-Host "SHA-256: $sha"
} finally {
    $env:MASH_CV_RUNTIME_VERSION = $previousVersion
    Pop-Location
}
