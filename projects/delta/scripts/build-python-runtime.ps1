param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$')]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$InterpreterArchive,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-f]{64}$')]
  [string]$InterpreterSha256,

  [Parameter(Mandatory = $true)]
  [string]$OutputDir
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# The interpreter archive is fetched by the release job from its pinned address; the build checks
# its digest and refuses anything else. The version must match the pack's PACK_VERSION.
python3 scripts/build_python_runtime.py `
  --version $Version `
  --platform x86_64-pc-windows-msvc `
  --interpreter-archive $InterpreterArchive `
  --interpreter-sha256 $InterpreterSha256 `
  --out-dir $OutputDir
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

$names = @(Get-ChildItem -LiteralPath $OutputDir -File | ForEach-Object { $_.Name } | Sort-Object)
$expected = @("delta-python-runtime.tar.gz", "manifest.json")
if (($names -join ",") -ne ($expected -join ",")) {
  throw "output must contain exactly delta-python-runtime.tar.gz and manifest.json; found: $($names -join ', ')"
}
