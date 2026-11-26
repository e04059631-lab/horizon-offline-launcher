# Packs the compiled launcher binary with the embedded Horizon 26.3 HORIZON_PAYLOAD
param(
    [string]$LauncherExe = ".\target\release\horizon-offline-launcher.exe",
    [string]$PayloadSource = "..\horizon-offline-launcher-releases\horizon-launcher-windows-x64.exe",
    [string]$OutExe = ".\horizon-offline-launcher.exe"
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not (Test-Path $LauncherExe)) {
    throw "Launcher binary not found: $LauncherExe"
}
if (-not (Test-Path $PayloadSource)) {
    throw "Payload source not found: $PayloadSource"
}

# Read 24-byte footer from source
$srcStream = [IO.File]::OpenRead((Resolve-Path $PayloadSource).Path)
try {
    $srcLen = $srcStream.Length
    if ($srcLen -lt 24) { throw "Payload source too small" }
    $srcStream.Seek(-24, [IO.SeekOrigin]::End) | Out-Null
    $footer = New-Object byte[] 24
    $srcStream.Read($footer, 0, 24) | Out-Null

    $magic = [Text.Encoding]::ASCII.GetString($footer, 8, 16)
    if ($magic -ne "HORIZON_PAYLOAD`0") {
        throw "Invalid HORIZON_PAYLOAD magic in $PayloadSource"
    }
    $payloadLen = [BitConverter]::ToUInt64($footer, 0)
    $payloadOffset = $srcLen - 24 - [int64]$payloadLen
    if ($payloadOffset -lt 0) {
        throw "Corrupt payload length: $payloadLen"
    }

    $tmpOut = "$OutExe.packing"
    $outStream = [IO.File]::Open($tmpOut, [IO.FileMode]::Create, [IO.FileAccess]::Write)
    try {
        # 1. Copy compiled launcher executable
        $exeStream = [IO.File]::OpenRead((Resolve-Path $LauncherExe).Path)
        try {
            $exeStream.CopyTo($outStream, 4 * 1024 * 1024)
        } finally {
            $exeStream.Close()
        }

        # 2. Copy payload + 24-byte footer
        $srcStream.Seek($payloadOffset, [IO.SeekOrigin]::Begin) | Out-Null
        $srcStream.CopyTo($outStream, 4 * 1024 * 1024)
    } finally {
        $outStream.Flush()
        $outStream.Close()
    }
} finally {
    $srcStream.Close()
}

if (Test-Path $OutExe) {
    Remove-Item -Path $OutExe -Force
}
Move-Item -Path "$OutExe.packing" -Destination $OutExe -Force
$final = Get-Item $OutExe
Write-Host ("[OK] Packed standalone launcher: {0} ({1:N2} MB, payload {2:N2} MB)" -f $final.FullName, ($final.Length / 1MB), ($payloadLen / 1MB)) -ForegroundColor Green
