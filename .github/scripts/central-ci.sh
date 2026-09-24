#!/usr/bin/env bash
set -Eeuo pipefail

TARGET_ROOT="${1:-}"
if [ -z "$TARGET_ROOT" ] || [ ! -d "$TARGET_ROOT" ]; then
  echo "central-ci: target root is required" >&2
  exit 64
fi

cd "$TARGET_ROOT"

python_required=true
desktop_required=true
rust_required=true
python_advisories_required=true
rust_advisories_required=true

if [ "${CENTRAL_CI_PR_NUMBER:-0}" != "0" ]; then
  changed_files="$(mktemp)"
  selected="$(mktemp)"
  trap 'rm -f "$changed_files" "$selected"' EXIT

  git diff --name-only "${CENTRAL_CI_BASE_SHA}...${CENTRAL_CI_HEAD_SHA}" > "$changed_files"
  bash scripts/detect-ci-path.sh < "$changed_files" > "$selected"
  # shellcheck disable=SC1090
  source "$selected"

  python_required="${python:-false}"
  desktop_required="${desktop:-false}"
  rust_required="${rust:-false}"
  python_advisories_required="${python_advisories:-false}"
  rust_advisories_required="${rust_advisories:-false}"
fi

bash scripts/test-ci-path.sh
python3 scripts/check_naming.py

for d in surfaces coworker assets src stt; do
  if [ -d "$d" ]; then
    echo "central-ci: forbidden top-level directory '$d' present" >&2
    exit 1
  fi
done

if grep -rn     "openwork-theme\|openwork:theme-pref"     apps core integrations packages services tests scripts     --include='*.ts' --include='*.tsx' --include='*.html' --include='*.rs'     --include='*.py' --include='*.js' --include='*.json' --include='*.css'     2>/dev/null   | grep -v 'apps/desktop/src/theme.ts'   | grep -v 'apps/desktop/index.html'   | grep -v 'apps/desktop/src-tauri/src/lib.rs'
then
  echo "central-ci: retired theme runtime key found in source" >&2
  exit 1
fi

python3 scripts/check_retired_paths.py
python3 scripts/check_retired_branding.py
python3 scripts/check_architecture_boundary.py

channel="$(grep '^channel' rust-toolchain.toml | tr -d '\r' | sed 's/.*"\(.*\)"/\1/')"
rustup toolchain install "$channel" --profile minimal --component rustfmt --component clippy
python -m pip install --disable-pip-version-check uv==0.12.3

if [ "$python_required" = "true" ] || [ "$rust_required" = "true" ]; then
  cargo build --release --bin delta_core --manifest-path crates/delta-core/Cargo.toml
fi

if [ "$python_required" = "true" ]; then
  uv python install 3.11 3.12 3.13
  for version in 3.11 3.12 3.13; do
    echo "central-ci: pytest Python $version"
    UV_PYTHON="$version" uv sync --locked --extra dev
    UV_PYTHON="$version" uv pip check --python .venv/bin/python
    DELTA_CORE_BINARY="$TARGET_ROOT/crates/delta-core/target/release/delta_core"       UV_PYTHON="$version" uv run --locked pytest tests -q
  done

  default_python="$(tr -d '\r\n ' < .python-version)"
  UV_PYTHON="$default_python" uv sync --locked --extra dev
  UV_PYTHON="$default_python" uv run --locked ruff check     core integrations packages tests --select E9,F63,F7,F82
  UV_PYTHON="$default_python" uv run --locked ruff check     core integrations packages tests --output-format=json > ruff-report.json || true
  UV_PYTHON="$default_python" uv run --locked python - <<'PY'
import json
with open("ruff-report.json", "r", encoding="utf-8") as fh:
    findings = json.load(fh)
if not isinstance(findings, list):
    raise SystemExit("invalid Ruff JSON report")
if findings:
    for finding in findings:
        print(
            f"{finding.get('filename','?')}:"
            f"{finding.get('location',{}).get('row','?')} "
            f"{finding.get('code','?')} {finding.get('message','?')}"
        )
    raise SystemExit(f"ruff findings {len(findings)} > 0")
PY
  UV_PYTHON="$default_python" uv run --locked pyright core integrations packages
fi

if [ "$python_advisories_required" = "true" ]; then
  default_python="$(tr -d '\r\n ' < .python-version)"
  UV_PYTHON="$default_python" uv sync --locked --extra dev
  UV_PYTHON="$default_python" uv pip check --python .venv/bin/python
  UV_PYTHON="$default_python" uv run --locked pip-audit --skip-editable
fi

if [ "$desktop_required" = "true" ]; then
  (
    cd apps/desktop
    npm ci
    npm audit --omit=dev --audit-level=high
    npx tsc --noEmit
    npx vitest run src/api.contract.test.ts src/runtime-contract.test.ts src/api.auth.test.ts
    npm test --       --exclude src/api.contract.test.ts       --exclude src/runtime-contract.test.ts       --exclude src/api.auth.test.ts
    npx playwright install --with-deps chromium
    npm run e2e
  )
fi

if [ "$rust_required" = "true" ]; then
  sudo apt-get update -qq
  sudo apt-get install -y     libwebkit2gtk-4.1-dev     libgtk-3-dev     libayatana-appindicator3-dev     librsvg2-dev     patchelf     libasound2-dev     pkg-config

  workspaces=(
    "apps/desktop/src-tauri"
    "packaging/portable/launcher"
    "crates/delta-core"
    "crates/delta-sdk"
    "crates/delta-connect"
    "crates/delta-sync"
    "crates/delta-stt"
  )
  for workspace in "${workspaces[@]}"; do
    echo "central-ci: Rust workspace $workspace"
    (
      cd "$workspace"
      cargo fmt --check
      cargo check --locked
      cargo clippy --locked --all-targets -- -D warnings
      cargo test --locked
    )
  done

  python scripts/check_rust_smoke.py --binary crates/delta-core/target/release/delta_core
fi

if [ "$rust_required" = "true" ] || [ "$rust_advisories_required" = "true" ]; then
  cargo install cargo-deny --version 0.20.2 --locked
  for workspace in     apps/desktop/src-tauri     packaging/portable/launcher     crates/delta-stt     crates/delta-core     crates/delta-sdk     crates/delta-connect     crates/delta-sync
  do
    (
      cd "$workspace"
      cargo deny --config "$TARGET_ROOT/deny.toml" check advisories
    )
  done
fi
