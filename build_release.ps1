# Build Release script
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
Write-Host "==> Building Horizon Offline Launcher (Release)..." -ForegroundColor Cyan

$mingwBin = "G:\mingw64\bin"
if (Test-Path $mingwBin) {
    $env:PATH = "$mingwBin;C:\Users\7ims (Admin)\.cargo\bin;$env:PATH"
} else {
    $env:PATH = "C:\Users\7ims (Admin)\.cargo\bin;$env:PATH"
}

cargo build --release
python .\embed_icon_pe.py

$builtExe = ".\target\release\horizon-offline-launcher.exe"
if (Test-Path $builtExe) {
    Copy-Item $builtExe -Destination ".\horizon-offline-launcher.exe" -Force
    Write-Host "[OK] Release binary built and copied to root: .\horizon-offline-launcher.exe" -ForegroundColor Green
} else {
    Write-Host "[ERROR] Binary not found at $builtExe" -ForegroundColor Red
}
