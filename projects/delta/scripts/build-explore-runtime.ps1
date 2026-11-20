param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$')]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$OutputDir
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# The version is the one entered when the release was started; the build refuses it unless it
# matches the pack's own PACK_VERSION.
python3 scripts/build_explore_runtime.py --version $Version --out-dir $OutputDir
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

$names = @(Get-ChildItem -LiteralPath $OutputDir -File | ForEach-Object { $_.Name } | Sort-Object)
$expected = @("explore-runtime.tar.gz", "manifest.json")
if (($names -join ",") -ne ($expected -join ",")) {
  throw "output must contain exactly explore-runtime.tar.gz and manifest.json; found: $($names -join ', ')"
}
