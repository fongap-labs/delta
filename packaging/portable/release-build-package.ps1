#requires -Version 5.1
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Target,
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $true)][string]$OutputDir
)
$ErrorActionPreference = 'Stop'
if ($Target -ne 'x86_64-pc-windows-msvc') { throw "Unsupported target: $Target" }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $root
$env:DSH_DESKTOP_APP_ID = 'com.delta.desktop'
$env:DSH_DESKTOP_UNSIGNED = '1'
# Never inherit signing identities for the explicitly unsigned Delta beta portable build.
$env:WINDOWS_SIGNING_CERT = $null
$env:WINDOWS_SIGNING_PASSWORD = $null
$env:WINDOWS_SIGNING_TIMESTAMP_URL = $null
$env:DELTA_PORTABLE_OFFLINE = '1'
& npm install --global pnpm@11.28.5
if ($LASTEXITCODE -ne 0) { throw 'Pinned pnpm installation failed' }
& pnpm install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { throw 'Harness dependency installation failed' }
& pnpm --filter '@deepseek-ai/dsh-desktop' run 'package:win:x64:dir' -- --unsigned
if ($LASTEXITCODE -ne 0) { throw 'Harness Electron directory packaging failed' }
$artifacts = Join-Path $root 'apps\desktop\.desktop-build\targets\win-x64\unsigned-artifacts'
$matches = @(Get-ChildItem -LiteralPath $artifacts -Recurse -Filter Delta.exe -File |
  Where-Object { Test-Path -LiteralPath (Join-Path $_.DirectoryName 'resources\app.asar') })
if ($matches.Count -ne 1) { throw "Expected one Delta Electron unpacked directory, got $($matches.Count)" }
& (Join-Path $PSScriptRoot 'build-portable-package.ps1') -ElectronAppDir $matches[0].DirectoryName -Version $Version -OutputDir $OutputDir
if ($LASTEXITCODE -ne 0) { throw 'Delta Portable ZIP build failed' }
