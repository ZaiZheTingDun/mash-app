[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$cache = Join-Path $repoRoot 'src-tauri/target/windows-adb'
$archive = Join-Path $cache 'platform-tools_r36.0.2-win.zip'
$expectedSha = 'b024d4f319d6ad3004de1ba7b96a5c7c5f3512e8b14126308d598b4ab93dcead'
New-Item -ItemType Directory -Path $cache -Force | Out-Null
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri 'https://dl.google.com/android/repository/platform-tools_r36.0.2-win.zip' -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedSha) {
    throw 'Windows ADB archive SHA-256 mismatch; remove the cached archive and retry.'
}
Expand-Archive -LiteralPath $archive -DestinationPath $cache -Force
$destination = Join-Path $repoRoot 'src-tauri/resources/adb'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
foreach ($name in @('adb.exe', 'AdbWinApi.dll', 'AdbWinUsbApi.dll', 'NOTICE.txt')) {
    Copy-Item -LiteralPath (Join-Path $cache "platform-tools/$name") -Destination (Join-Path $destination $name) -Force
}
Write-Output 'Windows ADB 36.0.2 installed and verified.'
