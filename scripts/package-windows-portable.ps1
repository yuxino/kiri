# Package the already-built Windows executable without starting a second build.
if ($env:OS -ne 'Windows_NT' -or -not $env:RUNNER_TEMP) {
  throw 'Run on a Windows CI runner'
}
$ErrorActionPreference = 'Stop'
$version = (Get-Content src-tauri/tauri.conf.json | ConvertFrom-Json).version
$portableDir = Join-Path $env:RUNNER_TEMP 'kiri-portable'
$verifyDir = Join-Path $env:RUNNER_TEMP 'kiri-portable-verify'
$archive = "Kiri-$version-Windows-x64-Portable.zip"
$executable = 'src-tauri/target/release/kiri.exe'
if (-not (Test-Path $executable)) { throw 'Missing Windows release executable' }
New-Item -ItemType Directory -Force -Path $portableDir | Out-Null
Copy-Item $executable (Join-Path $portableDir 'kiri.exe') -Force
New-Item -ItemType File -Force -Path (Join-Path $portableDir 'kiri.portable') | Out-Null
Compress-Archive -Path (Join-Path $portableDir '*') -DestinationPath $archive -Force
Expand-Archive -Path $archive -DestinationPath $verifyDir -Force
$expected = (Get-FileHash $executable).Hash
$actual = (Get-FileHash (Join-Path $verifyDir 'kiri.exe')).Hash
$contents = @(Get-ChildItem $verifyDir -File | ForEach-Object Name | Sort-Object)
if ($expected -ne $actual -or ($contents -join ',') -ne 'kiri.exe,kiri.portable') {
  throw 'Portable archive verification failed'
}
Write-Host "Verified $archive against the release executable"
