# Build Release script
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
Write-Host "==> Building Horizon Offline Launcher (Release, iOS Liquid Glass UI)..." -ForegroundColor Cyan

if (Test-Path "$env:USERPROFILE\tools\w64devkit\bin") {
    $env:PATH = "$env:USERPROFILE\tools\w64devkit\bin;$env:PATH"
}
if (Test-Path "$env:USERPROFILE\.cargo\bin") {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}

cargo build --release

$builtExe = ".\target\release\horizon-offline-launcher.exe"
if (Test-Path $builtExe) {
    Copy-Item $builtExe -Destination ".\horizon-offline-launcher.exe" -Force
    Write-Host "[OK] Release binary built and copied to root: .\horizon-offline-launcher.exe" -ForegroundColor Green
} else {
    Write-Host "[ERROR] Binary not found at $builtExe" -ForegroundColor Red
    exit 1
}
