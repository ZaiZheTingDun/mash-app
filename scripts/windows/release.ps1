<#
.SYNOPSIS
Print a read-only Windows release plan, or execute it with -Publish.
.EXAMPLE
./scripts/windows/release.ps1 0.16.0 -CvRuntime 0.3.0 -Installer -Push -Publish
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$AppVersion,
    [string]$CvRuntime = '',
    [string]$Platform,
    [string]$Channel = $(if ($env:RELEASE_CHANNEL) { $env:RELEASE_CHANNEL } else { 'windows-stable' }),
    [ValidatePattern('^[A-Za-z0-9._/-]+$')][string]$Remote = 'origin',
    [string]$Python,
    [switch]$Installer,
    [switch]$Push,
    [switch]$SkipTests,
    [switch]$Publish
)
. (Join-Path $PSScriptRoot 'common.ps1')
Push-Location $repoRoot
try {
    $native = Get-WindowsPlatform
    if (-not $Platform) { $Platform = $native }
    if ($Platform -ne $native) { throw "Platform must match the native build host ($native)." }
    if ($Channel -notmatch '^[A-Za-z0-9._-]+$') { throw 'Invalid channel.' }
    # Windows PowerShell 5.1 drops empty native arguments; use sentinels.
    $runtimeArgument = if ($CvRuntime) { $CvRuntime } else { '-' }
    $mode = if ($Publish) { 'publish' } else { 'plan' }
    $plan = Invoke-Native node @($releaseState, 'plan', $AppVersion, $runtimeArgument, $Platform, $mode) | ConvertFrom-Json
    Write-Host "App: $($plan.current.app) -> $AppVersion ($Platform); channel: $Channel"
    Write-Host "Runtime changes: $($plan.runtimeChanged)"
    Write-Host "CV runtime: $CvRuntime; installer: $Installer; push: $Push"
    if (-not $Publish) {
        if (-not $SkipTests) { Write-Host '  pnpm build; pnpm test:scripts; cargo test; CV pytest when releasing CV' }
        if (-not $plan.sameVersion -and $plan.changelog -eq 'unreleased') { Write-Host "  finalize and commit CHANGELOG.md for v$AppVersion" }
        if ($CvRuntime) { Write-Host "  scripts/windows/release-cv-runtime.ps1 $CvRuntime -Platform $Platform" }
        if (-not $plan.sameVersion) { Write-Host "  scripts/windows/bump-app-version.ps1 $AppVersion" }
        Write-Host "  scripts/windows/release-tauri-updater.ps1 -Channel $Channel"
        if ($Installer) { Write-Host '  scripts/windows/release-installer.ps1 -SkipBuild' }
        if ($Push) { Write-Host "  git push $Remote HEAD; push and verify release tags" }
        Write-Host 'No files, tags, artifacts or remote refs changed. Add -Publish to execute.'
        return
    }
    Assert-CleanWorktree
    Assert-ReleaseEnvironment -Signing
    foreach ($command in @('pnpm', 'cargo', 'git', 'node')) { Get-Command $command -ErrorAction Stop | Out-Null }
    if ((-not $SkipTests -or $CvRuntime) -and -not $Python) { Get-Command poetry -ErrorAction Stop | Out-Null }
    if (-not $SkipTests) {
        Invoke-Native pnpm @('build')
        Invoke-Native pnpm @('test:scripts')
        Invoke-Native cargo @('test', '--manifest-path', 'src-tauri/Cargo.toml')
        Push-Location (Join-Path $repoRoot 'sidecar/cv-runtime')
        try {
            if ($Python) { Invoke-Native $Python @('-m', 'pytest') }
            else {
                Invoke-Native poetry @('install', '--no-root')
                Invoke-Native poetry @('run', 'pytest')
            }
        } finally { Pop-Location }
    }
    if (-not $plan.sameVersion -and $plan.changelog -eq 'unreleased') {
        Invoke-Native node @($releaseState, 'changelog', $AppVersion, (Get-Date -Format 'yyyy-MM-dd'))
        Invoke-Native git @('add', '--', 'CHANGELOG.md')
        Invoke-Native git @('commit', '-m', "docs: add v$AppVersion release notes")
    }
    if ($CvRuntime) { & (Join-Path $PSScriptRoot 'release-cv-runtime.ps1') $CvRuntime -Platform $Platform -Python $Python }
    if (-not $plan.sameVersion) { & (Join-Path $PSScriptRoot 'bump-app-version.ps1') $AppVersion }
    & (Join-Path $PSScriptRoot 'release-tauri-updater.ps1') -Channel $Channel
    if ($Installer) { & (Join-Path $PSScriptRoot 'release-installer.ps1') -SkipBuild }
    if ($Push) {
        Invoke-Native git @('push', $Remote, 'HEAD')
        # Also push the app tag when adding Windows to an existing local release.
        $tags = @(@($plan.tags) + "v$AppVersion" | Select-Object -Unique)
        Invoke-Native git (@('push', $Remote) + $tags)
        foreach ($tag in $tags) { Invoke-Native git @('ls-remote', '--exit-code', '--tags', $Remote, "refs/tags/$tag") }
    }
    Assert-CleanWorktree
    Write-Host "Release complete: v$AppVersion ($Platform)"
} finally { Pop-Location }
