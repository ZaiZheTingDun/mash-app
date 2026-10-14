[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Installer)

$ErrorActionPreference = 'Stop'
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$bytes = [IO.File]::ReadAllBytes($installerPath)
if ($bytes.Length -lt 2 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
    throw 'Installer is not a Windows PE file.'
}
$signature = "$installerPath.sig"
if (-not (Test-Path -LiteralPath $signature) -or -not (Get-Content -LiteralPath $signature -Raw).Trim()) {
    throw 'Updater signature is missing or empty.'
}

$installRoot = Join-Path $env:LOCALAPPDATA 'Programs/Mash'
$process = Start-Process -FilePath $installerPath -ArgumentList '/S' -PassThru -WindowStyle Hidden
if (-not $process.WaitForExit(180000)) {
    $process.Kill()
    throw 'NSIS installer did not finish within three minutes.'
}
if ($process.ExitCode -ne 0) { throw "NSIS installer failed with exit code $($process.ExitCode)." }

$app = Join-Path $installRoot 'Mash.exe'
if (-not (Test-Path -LiteralPath $app)) {
    $app = Get-ChildItem -Path (Join-Path $env:LOCALAPPDATA 'Programs') -Filter 'Mash.exe' -File -Recurse -ErrorAction SilentlyContinue |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $app -or -not (Test-Path -LiteralPath $app)) { throw 'Installed Mash.exe was not found.' }

$appProcess = Start-Process -FilePath $app -WorkingDirectory (Split-Path -Parent $app) -PassThru -WindowStyle Hidden
try {
    Start-Sleep -Seconds 8
    $appProcess.Refresh()
    if ($appProcess.HasExited) { throw "Mash exited during launch smoke (exit code $($appProcess.ExitCode))." }
} finally {
    if (-not $appProcess.HasExited) { $appProcess.Kill(); $appProcess.WaitForExit(30000) }
}

$uninstaller = Join-Path (Split-Path -Parent $app) 'uninstall.exe'
if (Test-Path -LiteralPath $uninstaller) {
    $remove = Start-Process -FilePath $uninstaller -ArgumentList '/S' -PassThru -WindowStyle Hidden
    if (-not $remove.WaitForExit(180000)) { $remove.Kill(); throw 'NSIS uninstaller did not finish.' }
    if ($remove.ExitCode -ne 0) { throw "NSIS uninstaller failed with exit code $($remove.ExitCode)." }
}
Write-Output "Installed, launched and removed $app"
