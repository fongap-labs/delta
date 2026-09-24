param(
  [Parameter(Mandatory = $true)]
  [string]$Target,

  [Parameter(Mandatory = $true)]
  [string]$OutputDir
)

$ErrorActionPreference = "Stop"

if ($Target -ne "x86_64-pc-windows-msvc") {
  throw "Unsupported Delta release target: $Target"
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $repoRoot

python scripts/check_release_versions.py --quiet --release
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

Push-Location "apps\desktop"
try {
  npm ci
  if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
  }
}
finally {
  Pop-Location
}

& ".\packaging\portable\build-portable.ps1"
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

$files = @(Get-ChildItem -LiteralPath ".\releases" -File)
$zip = @($files | Where-Object Name -EQ "Delta-Windows-Portable.zip")
$sum = @($files | Where-Object Name -EQ "Delta-Windows-Portable.zip.sha256")

if (
  $files.Count -ne 2 -or
  $zip.Count -ne 1 -or
  $sum.Count -ne 1
) {
  throw "releases/ must contain exactly the portable ZIP and its SHA256 file"
}

$expectedHash = (Get-Content -LiteralPath $sum[0].FullName -Raw).Trim().ToLower()
$actualHash = (Get-FileHash -LiteralPath $zip[0].FullName -Algorithm SHA256).Hash.ToLower()
if ($expectedHash -ne $actualHash) {
  throw "portable SHA256 mismatch"
}

$entries = @(& tar -tf $zip[0].FullName)
if ($LASTEXITCODE -ne 0) {
  throw "unable to list portable ZIP"
}
if (
  -not $entries.Count -or
  @($entries | Where-Object { $_ -notlike "Delta/*" }).Count
) {
  throw "portable ZIP must contain exactly one Delta/ top-level directory"
}

$forbidden = @(
  "delta-server",
  "fastapi",
  "uvicorn",
  "server_entry",
  "packaging/server",
  "packaging\server",
  "/sidecar/",
  "\sidecar\"
)
$badEntries = @()
foreach ($entry in $entries) {
  $candidate = $entry.ToLowerInvariant()
  foreach ($pattern in $forbidden) {
    if ($candidate.Contains($pattern)) {
      $badEntries += $entry
      break
    }
  }
}
if ($badEntries.Count) {
  throw "portable ZIP contains retired application-backend artifacts: $($badEntries -join ', ')"
}

$verifyRoot = Join-Path $env:RUNNER_TEMP "delta-portable-verify"
if (Test-Path -LiteralPath $verifyRoot) {
  Remove-Item -LiteralPath $verifyRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $verifyRoot | Out-Null

& tar -xf $zip[0].FullName -C $verifyRoot
if ($LASTEXITCODE -ne 0) {
  throw "unable to extract portable ZIP"
}

$requiredFiles = @(
  "Delta\Delta.exe",
  "Delta\App\Delta\Delta.exe",
  "Delta\Other\AppInfo\appinfo.xml"
)
foreach ($relativePath in $requiredFiles) {
  $absolutePath = Join-Path $verifyRoot $relativePath
  if (-not (Test-Path -LiteralPath $absolutePath -PathType Leaf)) {
    throw "portable artifact missing required file: $relativePath"
  }
}

[xml]$appInfo = Get-Content -LiteralPath (Join-Path $verifyRoot "Delta\Other\AppInfo\appinfo.xml") -Raw
$actualVersion = [string]$appInfo.appinfo.version
$declaredVersion = (
  Select-String -Path "apps\desktop\src-tauri\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"'
).Matches.Groups[1].Value
if ($actualVersion -ne $declaredVersion) {
  throw "portable metadata version mismatch: expected $declaredVersion, got $actualVersion"
}

$appExe = Join-Path $verifyRoot "Delta\App\Delta\Delta.exe"
$smokeState = Join-Path $env:RUNNER_TEMP "delta-portable-smoke-state"
if (Test-Path -LiteralPath $smokeState) {
  Remove-Item -LiteralPath $smokeState -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $smokeState | Out-Null

$oldStateDir = $env:DELTA_STATE_DIR
$env:DELTA_STATE_DIR = $smokeState
try {
  & $appExe --runtime-self-test
  if ($LASTEXITCODE -ne 0) {
    throw "packaged Rust Runtime self-test failed (exit $LASTEXITCODE)"
  }
}
finally {
  $env:DELTA_STATE_DIR = $oldStateDir
}

$unicodeRoot = Join-Path $env:RUNNER_TEMP "test-Delta\Delta move-test\Delta"
if (Test-Path -LiteralPath $unicodeRoot) {
  Remove-Item -LiteralPath $unicodeRoot -Recurse -Force
}
Copy-Item -Recurse -Force (Join-Path $verifyRoot "Delta") $unicodeRoot

$unicodeExe = Join-Path $unicodeRoot "App\Delta\Delta.exe"
$unicodeState = Join-Path $unicodeRoot "Data"
if (-not (Test-Path -LiteralPath $unicodeState -PathType Container)) {
  throw "relocated portable did not preserve Data\ directory layout"
}

$oldStateDir = $env:DELTA_STATE_DIR
$env:DELTA_STATE_DIR = $unicodeState
try {
  & $unicodeExe --runtime-self-test
  if ($LASTEXITCODE -ne 0) {
    throw "packaged Rust Runtime self-test failed under Unicode+whitespace path (exit $LASTEXITCODE)"
  }
}
finally {
  $env:DELTA_STATE_DIR = $oldStateDir
}

if (Test-Path -LiteralPath $OutputDir) {
  Remove-Item -LiteralPath $OutputDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$targetZip = Join-Path $OutputDir "delta-windows-x64-portable.zip"
Copy-Item -LiteralPath $zip[0].FullName -Destination $targetZip -Force

if (-not (Test-Path -LiteralPath $targetZip -PathType Leaf)) {
  throw "central release asset was not staged"
}
if ((Get-Item -LiteralPath $targetZip).Length -le 0) {
  throw "central release asset is empty"
}

$actual = @(Get-ChildItem -LiteralPath $OutputDir -File)
if ($actual.Count -ne 1 -or $actual[0].Name -ne "delta-windows-x64-portable.zip") {
  throw "Delta central release build must produce exactly one ZIP asset"
}
