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
& pnpm install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { throw 'Harness dependency installation failed' }
& pnpm --filter '@deepseek-ai/dsh-desktop' run 'package:win:x64:dir' -- --unsigned
if ($LASTEXITCODE -ne 0) { throw 'Harness Electron directory packaging failed' }
$artifacts = Join-Path $root 'apps\desktop\.desktop-build\targets\win-x64\unsigned-artifacts'
$matches = @(Get-ChildItem -LiteralPath $artifacts -Recurse -Filter Delta.exe -File |
  Where-Object { Test-Path -LiteralPath (Join-Path $_.DirectoryName 'resources\app.asar') })
if ($matches.Count -ne 1) { throw "Expected one Delta Electron unpacked directory, got $($matches.Count)" }
& (Join-Path $PSScriptRoot 'build-portable.ps1') -ElectronAppDir $matches[0].DirectoryName -Version $Version -OutputDir $OutputDir
if ($LASTEXITCODE -ne 0) { throw 'Delta Portable ZIP build failed' }
