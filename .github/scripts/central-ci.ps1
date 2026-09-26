param(
  [Parameter(Mandatory = $true)]
  [string]$TargetRoot
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $TargetRoot -PathType Container)) {
  throw "central-ci: target root is required"
}

Set-Location $TargetRoot

if ($env:CENTRAL_CI_PR_NUMBER -ne "0") {
  $selectedText = git diff --name-only "$env:CENTRAL_CI_BASE_SHA...$env:CENTRAL_CI_HEAD_SHA" | bash scripts/detect-ci-path.sh
  if ($LASTEXITCODE -ne 0) {
    throw "central-ci: failed to detect changed areas"
  }

  $selected = @{}
  foreach ($line in $selectedText) {
    if ($line -match '^([a-z_]+)=(true|false)$') {
      $selected[$matches[1]] = $matches[2]
    }
  }

  if ($selected["python"] -ne "true") {
    Write-Host "central-ci: Windows Python checks not required for this PR."
    exit 0
  }
}

$pythonVersion = (Get-Content .python-version -Raw).Trim()
python -m pip install --disable-pip-version-check uv==0.12.3
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

uv python install $pythonVersion
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$env:UV_PYTHON = $pythonVersion
uv sync --locked --extra dev
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

uv pip check --python .venv\Scripts\python.exe
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

uv run --locked pytest tests -q
exit $LASTEXITCODE
