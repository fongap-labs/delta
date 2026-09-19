#!/usr/bin/env bash
# scripts/detect-ci-path.sh
#
# Given a list of file paths (one per line on stdin), outputs the CI
# area flags: python, desktop, rust, python_advisories,
# rust_advisories, full.
#
# This script is extracted from .github/workflows/ci.yml so the path
# detection logic can be unit-tested independently.
#
# Usage:
#   echo "crates/delta-core/src/lib.rs" | bash scripts/detect-ci-path.sh
#   printf "core/ledger.py\napps/tui/manager.py\n" | bash scripts/detect-ci-path.sh

set -euo pipefail

python=false
desktop=false
rust=false
python_advisories=false
rust_advisories=false
full=false

while IFS= read -r path; do
  case "$path" in
    # CI workflow changes are validated against the complete suite.
    .github/workflows/*)
      full=true
      ;;

    # Rust workspace crates (delta-core, delta-stt, delta-sdk, delta-connect, delta-sync).
    crates/*)
      rust=true
      ;;

    # Tauri Rust workspace.
    # MUST precede apps/desktop/* to avoid being shadowed.
    apps/desktop/src-tauri/*)
      rust=true
      ;;

    # Portable launcher Rust workspace.
    packaging/portable/launcher/*)
      rust=true
      ;;

    # Python runtime and repository-level Python tests.
    core/*|providers/*|integrations/*|packages/*|apps/tui/*|tests/*)
      python=true
      ;;

    # Python dependency graph changes require runtime and advisory checks.
    pyproject.toml|uv.lock)
      python=true
      python_advisories=true
      ;;

    # Release-version gate implementation is covered by Python tests.
    scripts/check_release_versions.py)
      python=true
      ;;

    # Desktop frontend.
    apps/desktop/*)
      desktop=true
      ;;

    # cargo-deny policy only affects Rust advisory validation.
    deny.toml)
      rust_advisories=true
      ;;

    # Other packaging changes are release-critical and receive full CI.
    packaging/*)
      full=true
      ;;
  esac
done

# Full validation overrides selective detection.
if [ "$full" = true ]; then
  python=true
  desktop=true
  rust=true
  python_advisories=true
  rust_advisories=true
fi

echo "python=$python"
echo "desktop=$desktop"
echo "rust=$rust"
echo "python_advisories=$python_advisories"
echo "rust_advisories=$rust_advisories"
echo "full=$full"
