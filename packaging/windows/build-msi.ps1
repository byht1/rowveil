# Builds the Rowveil MSI installer.
#   1. cargo build --release
#   2. dotnet tool install --global wix --version 5.0.2
#   3. pwsh packaging/windows/build-msi.ps1
# Output: target/release/Rowveil-<version>-x64.msi
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$out = "$root\target\release\Rowveil-$version-x64.msi"
# MSI versions are numbers only: 0.5.0-alpha.1 installs as 0.5.0.
$msiVersion = $version -replace '-.*$', ''

foreach ($exe in 'rowveil.exe', 'rowveil-server.exe') {
    if (-not (Test-Path "$root\target\release\$exe")) { throw "target\release\$exe not found: run 'cargo build --release' first" }
}

# WiX extensions come from nuget.org (the local nuget.config names the source).
Push-Location $PSScriptRoot
try {
    $have = wix extension list -g
    foreach ($ext in 'WixToolset.UI.wixext') {
        if (-not ($have -match $ext)) { wix extension add -g "$ext/5.0.2" }
    }
    wix build "$PSScriptRoot\rowveil.wxs" -arch x64 -ext WixToolset.UI.wixext `
        -d "Version=$msiVersion" -d "Root=$root" -o $out
    if ($LASTEXITCODE -ne 0) { throw "wix build failed ($LASTEXITCODE)" }
} finally { Pop-Location }

Write-Host "MSI: $out"
