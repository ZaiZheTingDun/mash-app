[CmdletBinding()]
param(
    [ValidateSet('all', 'runtime', 'code')][string]$Mode = 'all',
    [string]$Python,
    [switch]$SkipInstall
)
. (Join-Path $PSScriptRoot 'common.ps1')
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Run this script on Windows.' }
Push-Location (Join-Path $repoRoot 'sidecar/cv-runtime')
try {
    if ($Mode -ne 'code') {
        if ($Python) {
            Invoke-Native $Python @('build_runtime.py')
        } else {
            if (-not $SkipInstall) { Invoke-Native poetry @('install', '--no-root') }
            Invoke-Native poetry @('run', 'python', 'build_runtime.py')
        }
    }
    if ($Mode -ne 'runtime') {
        $codePython = if ($Python) { $Python } else { 'python' }
        Invoke-Native $codePython @('../cv-code/build_code.py')
    }
} finally { Pop-Location }
