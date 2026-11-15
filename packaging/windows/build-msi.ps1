# Builds the Rowveil MSI installer.
#   1. cargo build --release --no-default-features --bin rowveil-server
#      cargo build --release --manifest-path desktop/Cargo.toml
#   2. dotnet tool install --global wix --version 5.0.2
#   3. pwsh packaging/windows/build-msi.ps1
# Output: target/release/Rowveil-<version>-x64.msi
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$out = "$root\target\release\Rowveil-$version-x64.msi"
# MSI versions are numbers only: 0.5.0-alpha.1 installs as 0.5.0.
$msiVersion = $version -replace '-.*$', ''

$desktop = "$root\desktop\target\release"
if (-not (Test-Path "$desktop\rowveil.exe")) { throw "desktop\target\release\rowveil.exe not found: build desktop/ first" }
if (-not (Test-Path "$root\target\release\rowveil-server.exe")) { throw "target\release\rowveil-server.exe not found: build the server first" }

# WiX extensions come from nuget.org (the local nuget.config names the source).
Push-Location $PSScriptRoot
try {
    $have = wix extension list -g
    foreach ($ext in 'WixToolset.UI.wixext') {
        if (-not ($have -match $ext)) { wix extension add -g "$ext/5.0.2" }
    }
    wix build "$PSScriptRoot\rowveil.wxs" -arch x64 -ext WixToolset.UI.wixext `
        -d "Version=$msiVersion" -d "Root=$root" -d "Desktop=$desktop" -o $out
    if ($LASTEXITCODE -ne 0) { throw "wix build failed ($LASTEXITCODE)" }
} finally { Pop-Location }

Write-Host "MSI: $out"
