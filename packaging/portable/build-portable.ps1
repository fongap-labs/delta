#requires -Version 5.1
<#
.SYNOPSIS
  Delta Windows portable ZIP following the legacy/v0.0.1 public layout and naming.
.DESCRIPTION
  Keeps the Harness Electron/runtime architecture unchanged. A legacy Rust
  root launcher is used only as the portable executable dispatcher.
  The legacy embedded-Rust --runtime-self-test does NOT apply to Harness.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$ElectronAppDir,
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $true)][string]$OutputDir
)
$ErrorActionPreference = "Stop"
if ($env:OS -ne "Windows_NT") { throw "Windows x64 is required" }
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$electron = (Resolve-Path -LiteralPath $ElectronAppDir).Path
$innerExe = Join-Path $electron "Delta.exe"
if (-not (Test-Path -LiteralPath $innerExe -PathType Leaf)) { throw "Delta.exe is missing in unpacked Electron app: $innerExe" }
if (-not (Test-Path -LiteralPath (Join-Path $electron "resources\app.asar") -PathType Leaf)) { throw "Electron app.asar is missing; incomplete runtime" }
if (-not (Test-Path -LiteralPath (Join-Path $electron "resources\runtime") -PathType Container)) { throw "Harness runtime is missing" }

$launcherRoot = Join-Path $PSScriptRoot "launcher"
$launcherExe = Join-Path $launcherRoot "target\release\delta-portable-launcher.exe"
$oldRustFlags = $env:RUSTFLAGS
$env:RUSTFLAGS = (@($oldRustFlags, "-C target-feature=+crt-static") | Where-Object { $_ }) -join " "
try {
  Push-Location $launcherRoot
  try {
    & cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "Legacy launcher build failed" }
  } finally { Pop-Location }
} finally { $env:RUSTFLAGS = $oldRustFlags }
if (-not (Test-Path -LiteralPath $launcherExe -PathType Leaf)) { throw "Legacy launcher executable was not built" }

$stage = Join-Path $PSScriptRoot "build\staging"
$delta = Join-Path $stage "Delta"
$dest = Join-Path $delta "App\Delta"
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Path $dest -Force | Out-Null
foreach ($item in Get-ChildItem -LiteralPath $electron -Force) {
  Copy-Item -LiteralPath $item.FullName -Destination $dest -Recurse -Force
}
Copy-Item -LiteralPath $launcherExe -Destination (Join-Path $delta "Delta.exe") -Force
foreach ($item in @("Data", "Other\AppInfo", "Other\License", "Other\Source", "Other\Help")) {
  New-Item -ItemType Directory -Path (Join-Path $delta $item) -Force | Out-Null
}
Copy-Item (Join-Path $root "LICENSE") (Join-Path $delta "Other\License\LICENSE.txt")
Copy-Item (Join-Path $root "UPSTREAM_LICENSE") (Join-Path $delta "Other\License\UPSTREAM_LICENSE.txt")
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $delta "Other\Source\BUILD.txt") -Value @"
Delta portable from fongap-labs/delta
Baseline: deepseek-ai/deepseek-harness 2e39e70496aa02986ca6a820c5d497e816b69d7e
Packaging: legacy/v0.0.1 directory, launcher, ZIP, SHA-256
Carrier: Electron (Harness); not the former Tauri/Rust runtime
"@
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $delta "Other\Help\PORTABLE.txt") -Value @"
Delta Portable
1. Extract the whole Delta folder to a writable location.
2. Launch Delta.exe from the folder root.
3. Keep Data and App inside the same folder when moving it.
"@
$info = New-Object System.Xml.XmlDocument
$info.LoadXml("<appinfo><name>Delta Portable</name><version></version><launcher>Delta.exe</launcher><layout relocatable='true' runtime='harness-electron' /></appinfo>")
$info.DocumentElement.version = [string]$Version
$info.Save((Join-Path $delta "Other\AppInfo\appinfo.xml"))

& (Join-Path $PSScriptRoot "sign-windows.ps1") -Path @((Join-Path $delta "Delta.exe"), (Join-Path $dest "Delta.exe"))
if ($LASTEXITCODE -ne 0) { throw "Portable code signing failed" }

$zip = Join-Path $root "releases\Delta-Windows-Portable.zip"
New-Item -ItemType Directory -Path (Split-Path -Parent $zip) -Force | Out-Null
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
Push-Location $stage
try {
  & tar -a -c -f $zip "Delta"
  if ($LASTEXITCODE -ne 0) { throw "Portable ZIP creation failed" }
} finally { Pop-Location }
$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -Encoding ASCII -LiteralPath "$zip.sha256" -Value $hash
$entryNames = @(& tar -tf $zip)
if ($LASTEXITCODE -ne 0 -or -not $entryNames.Count) { throw "Portable ZIP verification failed" }
if (@($entryNames | Where-Object { $_ -notlike 'Delta/*' }).Count -gt 0) { throw "ZIP has a non-Delta root" }
$required = @("Delta/Delta.exe", "Delta/App/Delta/Delta.exe", "Delta/Other/AppInfo/appinfo.xml")
$normalizedEntries = @($entryNames | ForEach-Object { $_.Replace('\', '/') })
foreach ($expected in $required) {
  if ($expected -notin $normalizedEntries) { throw "ZIP is missing $expected" }
}
if ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) { throw "SHA256 validation failed" }
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$central = Join-Path $OutputDir "delta-windows-x64-portable.zip"
Copy-Item -LiteralPath $zip -Destination $central -Force
if ((Get-FileHash -LiteralPath $central -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) { throw "Central asset checksum mismatch" }
Write-Host "Delta Portable asset: $central"
Write-Host "Delta Portable SHA256: $hash"
