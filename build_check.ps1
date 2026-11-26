Set-Location $PSScriptRoot
if (Test-Path "$env:USERPROFILE\tools\w64devkit\bin") {
    $env:PATH = "$env:USERPROFILE\tools\w64devkit\bin;$env:PATH"
}
if (Test-Path "$env:USERPROFILE\.cargo\bin") {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}
cargo check
