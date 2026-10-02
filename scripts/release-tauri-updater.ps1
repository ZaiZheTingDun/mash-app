[CmdletBinding()]
param([string]$Channel = 'stable')
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
foreach ($name in @('R2_ENDPOINT', 'R2_BUCKET', 'RELEASE_BASE_URL', 'TAURI_SIGNING_PRIVATE_KEY')) {
    if (-not [Environment]::GetEnvironmentVariable($name)) { throw "$name is required." }
}
if ($Channel -notmatch '^[A-Za-z0-9._-]+$') { throw 'Invalid channel.' }
function Invoke-AwsCopy([string]$Source, [string]$Key, [string]$Cache, [switch]$Json) {
    $arguments = @('s3', 'cp', $Source, "s3://$env:R2_BUCKET/$Key", '--endpoint-url', $env:R2_ENDPOINT, '--cache-control', $Cache)
    if ($env:AWS_PROFILE) { $arguments += @('--profile', $env:AWS_PROFILE) }
    if ($Json) { $arguments += @('--content-type', 'application/json') }
    aws @arguments
    if ($LASTEXITCODE -ne 0) { throw "Uploading $Key failed." }
}
function Get-OptionalManifest([string]$Url, [string]$Path) {
    try { Invoke-WebRequest -Uri $Url -OutFile $Path }
    catch {
        if ($_.Exception.Response.StatusCode -eq 404) { return }
        throw
    }
}
Push-Location $repoRoot
try {
    if (git status --porcelain) { throw 'Worktree must be clean before publishing.' }
    $tag = git describe --tags --exact-match
    if ($LASTEXITCODE -ne 0 -or $tag -notmatch '^v\d+\.\d+\.\d+([+-][0-9A-Za-z.-]+)?$') { throw 'HEAD must be at an app release tag.' }
    $version = $tag.Substring(1)
    $config = Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
    if ($config.version -ne $version) { throw 'App configuration and tag versions differ.' }
    $hostInfo = rustc -vV
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine the native Rust target.' }
    $target = if ($hostInfo -match 'host: x86_64-pc-windows-msvc') { 'windows-x86_64' }
              elseif ($hostInfo -match 'host: aarch64-pc-windows-msvc') { 'windows-aarch64' }
              else { throw 'Unsupported Windows Rust target.' }
    & (Join-Path $PSScriptRoot 'build-windows.ps1')
    $bundle = Join-Path $repoRoot 'src-tauri/target/release/bundle'
    $artifact = node scripts/updater-artifact.mjs $bundle 'src-tauri/tauri.conf.json' $target
    if ($LASTEXITCODE -ne 0) { throw 'Selecting the current updater artifact failed.' }
    $signatureFile = Get-Item -LiteralPath "$artifact.sig"
    $signature = (Get-Content -LiteralPath $signatureFile.FullName -Raw).Trim()
    $prefix = if ($env:R2_PREFIX) { $env:R2_PREFIX.Trim('/') } else { 'mash' }
    $baseUrl = $env:RELEASE_BASE_URL.TrimEnd('/')
    $artifactKey = "$prefix/releases/versions/$tag/$target/$(Split-Path -Leaf $artifact)"
    $versionKey = "$prefix/releases/versions/$tag/latest.json"
    $channelKey = "$prefix/releases/$Channel/latest.json"
    $work = Join-Path $repoRoot "src-tauri/target/release-updater/$tag/$target/$([guid]::NewGuid())"
    New-Item -ItemType Directory -Path $work -Force | Out-Null
    $previous = Join-Path $work 'previous-version.json'
    $previousChannel = Join-Path $work 'previous-channel.json'
    $latest = Join-Path $work 'latest.json'
    Get-OptionalManifest "$baseUrl/$versionKey" $previous
    Get-OptionalManifest "$baseUrl/$channelKey" $previousChannel
    node scripts/updater-manifest.mjs $latest $version ([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')) $target "$baseUrl/$artifactKey" $signature $previous $previousChannel
    if ($LASTEXITCODE -ne 0) { throw 'Generating the merged updater manifest failed.' }
    Invoke-AwsCopy $artifact $artifactKey 'public, max-age=31536000, immutable'
    Invoke-AwsCopy $signatureFile.FullName "$artifactKey.sig" 'public, max-age=31536000, immutable'
    Invoke-AwsCopy $latest $versionKey 'no-cache' -Json
    Invoke-AwsCopy $latest $channelKey 'no-cache' -Json
    $remote = Invoke-RestMethod -Uri "$baseUrl/$channelKey"
    $platform = $remote.platforms.$target
    if ($remote.version -ne $version -or $platform.url -ne "$baseUrl/$artifactKey" -or $platform.signature -ne $signature) {
        throw 'Published updater manifest validation failed.'
    }
    Invoke-WebRequest -Uri "$baseUrl/$artifactKey" -Method Head | Out-Null
    Write-Output "Published $tag ($target): $baseUrl/$channelKey"
} finally { Pop-Location }
