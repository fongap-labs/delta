#!/usr/bin/env bash
# Central execution contract: invoked by fongap-labs/action-worker.
#
# Usage: central-ci.sh <target-root> [shard]
#
# Without a shard, or with "all", one process runs every phase. With a shard, only that shard's
# phases run, so the shards declared under matrix.shard in .github/execution-manifest.json can run
# as parallel jobs. Every phase belongs to exactly one shard.
#
#   checks       repository rules and the path selector regression test
#   python       pytest on three Python versions, ruff, pyright and pip-audit
#   desktop      desktop typecheck, unit tests, npm audit and end-to-end tests
#   rust-app-lint   rustfmt and clippy for the desktop shell and the portable launcher
#   rust-app-test   cargo test for the desktop shell and the portable launcher
#   rust-crates     rustfmt, clippy and cargo test for the delta crates
#   deny            cargo-deny license and advisory checks
set -Eeuo pipefail

TARGET_ROOT="${1:-}"
SHARD="${2:-all}"
if [ -z "$TARGET_ROOT" ] || [ ! -d "$TARGET_ROOT" ]; then
  echo "central-ci: target root is required" >&2
  exit 64
fi
if [ -z "${CENTRAL_CI_AW_ROOT:-}" ] || [ ! -f "$CENTRAL_CI_AW_ROOT/tests/run-pack.mjs" ]; then
  echo "central-ci: CENTRAL_CI_AW_ROOT must point to the action-worker checkout" >&2
  exit 64
fi
case "$SHARD" in
  all | checks | python | desktop | rust-app-lint | rust-app-test | rust-crates | deny) ;;
  *)
    echo "central-ci: unknown shard: $SHARD" >&2
    exit 64
    ;;
esac

cd "$TARGET_ROOT"

run_pool() {
  local limit="$1"
  local worker="$2"
  shift 2

  local -a pids=()
  local -a labels=()
  local failed=0

  for item in "$@"; do
    "$worker" "$item" &
    pids+=("$!")
    labels+=("$item")

    if [ "${#pids[@]}" -ge "$limit" ]; then
      if ! wait "${pids[0]}"; then
        echo "central-ci: $worker failed for ${labels[0]}" >&2
        failed=1
      fi
      pids=("${pids[@]:1}")
      labels=("${labels[@]:1}")
    fi
  done

  local index
  for index in "${!pids[@]}"; do
    if ! wait "${pids[$index]}"; then
      echo "central-ci: $worker failed for ${labels[$index]}" >&2
      failed=1
    fi
  done

  return "$failed"
}

run_shard() {
  [ "$SHARD" = "all" ] || [ "$SHARD" = "$1" ]
}

# A failed phase ends the script before phase_end; the log is printed on failure anyway.
phase_begin() {
  PHASE_LABEL="$1"
  PHASE_STARTED=$SECONDS
}

phase_end() {
  echo "central-ci: $SHARD/$PHASE_LABEL finished in $((SECONDS - PHASE_STARTED))s"
}

run_python_version() {
  local version="$1"
  local env_dir="$TARGET_ROOT/.venv-ci-${version//./}"
  (
    set -Eeuo pipefail
    trap 'rm -rf "$env_dir"' EXIT
    echo "central-ci: pytest Python $version"
    UV_PROJECT_ENVIRONMENT="$env_dir" UV_PYTHON="$version" uv sync --locked --extra dev
    UV_PYTHON="$version" uv pip check --python "$env_dir/bin/python"
    UV_PROJECT_ENVIRONMENT="$env_dir" \
      UV_PYTHON="$version" \
      node "$CENTRAL_CI_AW_ROOT/tests/run-pack.mjs" delta "$TARGET_ROOT"
  )
}

# cargo clippy --all-targets compiles every target that cargo check would, so a separate
# cargo check pass only repeated work.
run_rust_lint_workspace() {
  local workspace="$1"
  (
    set -Eeuo pipefail
    echo "central-ci: Rust lint $workspace"
    cd "$workspace"
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings
  )
}

run_rust_test_workspace() {
  local workspace="$1"
  (
    set -Eeuo pipefail
    echo "central-ci: Rust test $workspace"
    cd "$workspace"
    cargo test --locked
  )
}

# Lint and test back to back, for a shard that keeps both on one runner. Both calls are plain
# statements so that set -e stops at the first failure.
run_rust_workspace() {
  local workspace="$1"
  (
    set -Eeuo pipefail
    run_rust_lint_workspace "$workspace"
    run_rust_test_workspace "$workspace"
  )
}

run_license_workspace() {
  local workspace="$1"
  (
    set -Eeuo pipefail
    cd "$workspace"
    cargo deny --config "$TARGET_ROOT/deny.toml" check licenses
  )
}

run_advisory_workspace() {
  local workspace="$1"
  (
    set -Eeuo pipefail
    cd "$workspace"
    cargo deny --config "$TARGET_ROOT/deny.toml" check advisories
  )
}

install_rust_toolchain() {
  local channel
  channel="$(grep '^channel' rust-toolchain.toml | tr -d '\r' | sed 's/.*"\(.*\)"/\1/')"
  rustup toolchain install "$channel" --profile minimal --component rustfmt --component clippy
}

# The desktop shell links GTK, WebKit and the tray libraries.
install_rust_app_system_packages() {
  sudo apt-get update -qq
  sudo apt-get install -y     libwebkit2gtk-4.1-dev     libgtk-3-dev     libayatana-appindicator3-dev     librsvg2-dev     patchelf     libasound2-dev     pkg-config
}

# The delta crates need only the audio library of delta-stt. No crate depends on GTK or WebKit,
# and rusqlite builds its own SQLite.
install_rust_crates_system_packages() {
  sudo apt-get update -qq
  sudo apt-get install -y libasound2-dev pkg-config
}

# Building cargo-deny from source on every run took minutes. On Linux x86_64 the pinned upstream
# release binary is installed instead, and it is used only if its SHA-256 matches. Any other
# platform keeps building the same version from source.
install_cargo_deny() {
  local version="0.20.2"
  local asset="cargo-deny-${version}-x86_64-unknown-linux-musl"
  local sha256="9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f"
  if [ "$(uname -s)-$(uname -m)" != "Linux-x86_64" ]; then
    cargo install cargo-deny --version "$version" --locked
    return
  fi

  local work binary bin_dir
  work="$(mktemp -d)"
  curl -fsSL --retry 3 --retry-delay 2 \
    --output "$work/$asset.tar.gz" \
    "https://github.com/EmbarkStudios/cargo-deny/releases/download/${version}/${asset}.tar.gz"
  echo "$sha256  $work/$asset.tar.gz" | sha256sum --check --strict -
  tar -xzf "$work/$asset.tar.gz" -C "$work"
  binary="$(find "$work" -type f -name cargo-deny | head -n 1)"
  if [ -z "$binary" ]; then
    echo "central-ci: cargo-deny binary not found in the release archive" >&2
    exit 1
  fi
  bin_dir="${CARGO_HOME:-$HOME/.cargo}/bin"
  mkdir -p "$bin_dir"
  install -m 0755 "$binary" "$bin_dir/cargo-deny"
  rm -rf "$work"
  cargo deny --version
}

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

if run_shard checks; then
  phase_begin checks
  bash scripts/test-ci-path.sh
  python3 scripts/check_naming.py

  for d in surfaces coworker assets src stt; do
    if [ -d "$d" ]; then
      echo "central-ci: forbidden top-level directory '$d' present" >&2
      exit 1
    fi
  done

  if grep -rn     "openwork-theme\|openwork:theme-pref"     apps core integrations packages services scripts     --include='*.ts' --include='*.tsx' --include='*.html' --include='*.rs'     --include='*.py' --include='*.js' --include='*.json' --include='*.css'     2>/dev/null   | grep -v 'apps/desktop/src/theme.ts'   | grep -v 'apps/desktop/index.html'   | grep -v 'apps/desktop/src-tauri/src/lib.rs'
  then
    echo "central-ci: retired theme runtime key found in source" >&2
    exit 1
  fi

  python3 scripts/check_retired_paths.py
  python3 scripts/check_retired_branding.py
  python3 scripts/check_architecture_boundary.py
  python3 scripts/check_license_policy.py
  phase_end
fi

# The license check always needs cargo; the Rust shards need it only when Rust must be built.
needs_rust_toolchain=false
if run_shard deny; then
  needs_rust_toolchain=true
fi
if [ "$rust_required" = "true" ] && { run_shard rust-app-lint || run_shard rust-app-test || run_shard rust-crates; }; then
  needs_rust_toolchain=true
fi
if [ "$needs_rust_toolchain" = "true" ]; then
  phase_begin rust-toolchain
  install_rust_toolchain
  phase_end
fi

if run_shard python && { [ "$python_required" = "true" ] || [ "$python_advisories_required" = "true" ]; }; then
  phase_begin python-tools
  python -m pip install --disable-pip-version-check uv==0.12.3
  phase_end

  if [ "$python_required" = "true" ]; then
    phase_begin python
    uv python install 3.11 3.12 3.13
    run_pool 2 run_python_version 3.11 3.12 3.13

    default_python="$(tr -d '\r\n ' < .python-version)"
    UV_PYTHON="$default_python" uv sync --locked --extra dev
    UV_PYTHON="$default_python" uv run --locked ruff check     core integrations packages --select E9,F63,F7,F82
    UV_PYTHON="$default_python" uv run --locked ruff check     core integrations packages --output-format=json > ruff-report.json || true
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
    phase_end
  fi

  if [ "$python_advisories_required" = "true" ]; then
    phase_begin python-advisories
    default_python="$(tr -d '\r\n ' < .python-version)"
    UV_PYTHON="$default_python" uv sync --locked --extra dev
    UV_PYTHON="$default_python" uv pip check --python .venv/bin/python
    UV_PYTHON="$default_python" uv run --locked pip-audit --skip-editable
    phase_end
  fi
fi

if run_shard desktop && [ "$desktop_required" = "true" ]; then
  phase_begin desktop
  (
    cd apps/desktop
    npm ci
    npm audit --omit=dev --audit-level=high
    npx tsc --noEmit
    # Contract tests run first; every listed file must exist so a rename cannot silently empty this list.
    contract_tests=(src/runtime-contract.test.ts)
    exclude_contract_tests=()
    for contract_test in "${contract_tests[@]}"; do
      [ -f "$contract_test" ] || { echo "central-ci: missing desktop contract test: $contract_test" >&2; exit 1; }
      exclude_contract_tests+=(--exclude "$contract_test")
    done
    npx vitest run "${contract_tests[@]}"
    npm test -- "${exclude_contract_tests[@]}"
    npx playwright install --with-deps chromium
    npm run e2e
  )
  phase_end
fi

if [ "$rust_required" = "true" ]; then
  if run_shard rust-app-lint || run_shard rust-app-test; then
    phase_begin rust-app-system-packages
    install_rust_app_system_packages
    phase_end
  fi
  # A run of all shards already installed the larger set above.
  if [ "$SHARD" = "rust-crates" ]; then
    phase_begin rust-crates-system-packages
    install_rust_crates_system_packages
    phase_end
  fi

  if run_shard rust-app-lint; then
    phase_begin rust-app-lint
    run_pool 2 run_rust_lint_workspace \
      "apps/desktop/src-tauri" \
      "packaging/portable/launcher"
    phase_end
  fi

  if run_shard rust-app-test; then
    phase_begin rust-app-test
    run_pool 2 run_rust_test_workspace \
      "apps/desktop/src-tauri" \
      "packaging/portable/launcher"
    phase_end
  fi

  if run_shard rust-crates; then
    phase_begin rust-crates
    run_pool 2 run_rust_workspace \
      "crates/delta-core" \
      "crates/delta-sdk" \
      "crates/delta-connect" \
      "crates/delta-sync" \
      "crates/delta-stt"
    phase_end
  fi
fi

if run_shard deny; then
  phase_begin cargo-deny-install
  install_cargo_deny
  phase_end

  license_workspaces=(
    "apps/desktop/src-tauri"
    "packaging/portable/launcher"
    "crates/delta-stt"
    "crates/delta-core"
  )
  phase_begin cargo-deny-licenses
  run_pool 2 run_license_workspace "${license_workspaces[@]}"
  phase_end

  if [ "$rust_required" = "true" ] || [ "$rust_advisories_required" = "true" ]; then
    advisory_workspaces=(
      "apps/desktop/src-tauri"
      "packaging/portable/launcher"
      "crates/delta-stt"
      "crates/delta-core"
      "crates/delta-sdk"
      "crates/delta-connect"
      "crates/delta-sync"
    )
    phase_begin cargo-deny-advisories
    run_pool 2 run_advisory_workspace "${advisory_workspaces[@]}"
    phase_end
  fi
fi
