param(
  [Parameter(Mandatory = $true)]
  [string]$Target,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+(-beta\.[1-9][0-9]*)?$')]
  [string]$Version,

  [Parameter(Mandatory = $true)]
  [string]$OutputDir
)

$ErrorActionPreference = "Stop"

# The runtime pack keeps its own version (PACK_VERSION in packages/python_runtime); the Delta release
# version is accepted here so the release calls every build the same way, and is not written into
# the pack.
if ($Target -ne "x86_64-pc-windows-msvc") {
  throw "Unsupported runtime target: $Target"
}

# The pinned CPython 3.12 install_only archive: CPython 3.12.16 from python-build-standalone release
# 20261026. The digest matches that release's SHA256SUMS, checked by the runtime-interpreter-pin workflow.
# The build refuses to run while either value is empty.
$InterpreterUrl = "https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16%2B20261026-x86_64-pc-windows-msvc-install_only.tar.gz"
$InterpreterSha256 = "590dfcbe3a34c07301e6332e13e2188b98655cd072b08afa45a7998dc4c22bc5"
if ([string]::IsNullOrEmpty($InterpreterUrl) -or [string]::IsNullOrEmpty($InterpreterSha256)) {
  throw "The CPython interpreter is not pinned yet. Run the Runtime interpreter pin workflow and record its digest."
}

# The release job sets RELEASE_PRODUCT_ROOT to the product root; a local run uses this folder's parent.
$repoRoot = if ($env:RELEASE_PRODUCT_ROOT) { $env:RELEASE_PRODUCT_ROOT } else { Resolve-Path (Join-Path $PSScriptRoot "..") }
Set-Location $repoRoot

$work = Join-Path $env:RUNNER_TEMP "python-runtime"
New-Item -ItemType Directory -Force -Path $work | Out-Null
$archive = Join-Path $work "cpython.tar.gz"
Invoke-WebRequest -Uri $InterpreterUrl -OutFile $archive -UseBasicParsing
$digest = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
if ($digest -ne $InterpreterSha256) {
  throw "the interpreter archive digest is $digest, expected $InterpreterSha256"
}

# The builder runs `uv export` and `uv pip install`; the repository pins uv 0.12.3.
python -m pip install "uv==0.12.3"
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

$packOut = Join-Path $work "out"
$packVersion = (python -c "from packages.python_runtime.runtime_pack import PACK_VERSION; print(PACK_VERSION)").Trim()
python scripts/build_python_runtime.py `
  --version $packVersion `
  --platform x86_64-pc-windows-msvc `
  --interpreter-archive $archive `
  --interpreter-sha256 $InterpreterSha256 `
  --out-dir $packOut
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
Copy-Item -LiteralPath (Join-Path $packOut "delta-python-runtime.tar.gz") -Destination $OutputDir
Copy-Item -LiteralPath (Join-Path $packOut "manifest.json") -Destination (Join-Path $OutputDir "delta-python-runtime-manifest.json")

$names = @(Get-ChildItem -LiteralPath $OutputDir -File | ForEach-Object { $_.Name } | Sort-Object)
$expected = @("delta-python-runtime-manifest.json", "delta-python-runtime.tar.gz")
if (($names -join ",") -ne ($expected -join ",")) {
  throw "output must contain exactly delta-python-runtime.tar.gz and delta-python-runtime-manifest.json; found: $($names -join ', ')"
}
