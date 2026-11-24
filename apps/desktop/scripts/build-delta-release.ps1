param(
  [Parameter(Mandatory = $true)][string]$Target,
  [Parameter(Mandatory = $true)][string]$Version,
  [Parameter(Mandatory = $true)][string]$OutputDir
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if ($Target -ne 'x86_64-pc-windows-msvc' -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -or -not $IsWindows) {
  throw 'Delta release requires Windows x64.'
}
if ($Version -notmatch '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-beta\.[1-9]\d*)?$') {
  throw 'Invalid Delta release version.'
}
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$output = [IO.Path]::GetFullPath($OutputDir)
$stage = Join-Path ([IO.Path]::GetFullPath((Join-Path $root '../../outputs'))) ("Delta-release-" + [guid]::NewGuid().ToString('N'))
$sevenZip = Join-Path $env:ProgramFiles '7-Zip/7z.exe'
if (-not (Test-Path -LiteralPath $sevenZip -PathType Leaf)) { throw '7-Zip is required on the Windows build runner.' }
New-Item -ItemType Directory -Path $output -Force | Out-Null
$archive = Join-Path $output 'Delta-win-x64.zip'
if (Test-Path -LiteralPath $archive) { throw 'Release output already exists.' }

Push-Location $root
try {
  $env:DELTA_OUTPUT_DIR = $stage
  $env:DELTA_RELEASE_VERSION = $Version
  foreach ($name in @('DELTA_SKIP_BUILD', 'DELTA_REUSE_PREPARED_RUNTIME', 'DELTA_REUSE_ELECTRON_OUTPUT', 'DELTA_REPLACE_OUTPUT')) {
    Remove-Item "Env:$name" -ErrorAction SilentlyContinue
  }
  & npm.cmd install --global pnpm@11.28.5
  & pnpm.cmd install --frozen-lockfile
  & pnpm.cmd run package:desktop:portable
  Push-Location $stage
  try {
    & $sevenZip a -tzip -mx=5 $archive App Data Other
  } finally { Pop-Location }
} finally {
  Pop-Location
  if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
}
