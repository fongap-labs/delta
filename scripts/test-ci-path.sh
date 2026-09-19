#!/usr/bin/env bash
# scripts/test-ci-path.sh
#
# Regression test for scripts/detect-ci-path.sh. Validates that
# specific paths resolve to the expected CI area flags, especially
# that crates/delta-core/* is detected as rust=true (not shadowed
# by the broad core/* Python rule).
#
# Usage: bash scripts/test-ci-path.sh
# Exit: 0 on success, 1 on any failed assertion.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELECTOR="$SCRIPT_DIR/detect-ci-path.sh"

if [ ! -x "$SELECTOR" ] && [ ! -f "$SELECTOR" ]; then
  echo "FAIL: $SELECTOR not found" >&2
  exit 2
fi

FAIL=0

# assert <expected> <flag> <path>
assert() {
  local expected="$1"
  local flag="$2"
  local path="$3"
  local actual
  actual="$(printf '%s\n' "$path" | bash "$SELECTOR" | grep "^$flag=" | cut -d'=' -f2)"
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $path → $flag=$actual (expected $expected)" >&2
    FAIL=$((FAIL + 1))
  else
    echo "PASS: $path → $flag=$actual"
  fi
}

echo "--- regression tests ---"

# 1. crates/delta-core/* must be rust, not python
assert "true" "rust" "crates/delta-core/src/taskstore.rs"
assert "true" "rust" "crates/delta-core/Cargo.toml"
assert "true" "rust" "crates/delta-core/src/bin/write_ledger.rs"

# 2. core/automation/* and other core Python modules stay python
assert "true" "python" "core/automation/store.py"
assert "true" "python" "core/ledger.py"
assert "true" "python" "core/idemlog.py"
assert "true" "python" "core/analyzer.py"

# 3. apps/desktop/* Rust subdirectory
assert "true" "rust" "apps/desktop/src-tauri/src/main.rs"
assert "true" "rust" "apps/desktop/src-tauri/Cargo.toml"

# 4. apps/desktop/* frontend stays desktop (not rust)
assert "true" "desktop" "apps/desktop/src/App.tsx"
assert "true" "desktop" "apps/desktop/package.json"

# 5. crates/delta-stt/* Rust
assert "true" "rust" "crates/delta-stt/src/main.rs"
assert "true" "rust" "crates/delta-stt/Cargo.toml"

# 5b. crates/delta-sdk, delta-connect, delta-sync (R8.5)
assert "true" "rust" "crates/delta-sdk/src/lib.rs"
assert "true" "rust" "crates/delta-sdk/Cargo.toml"
assert "true" "rust" "crates/delta-connect/src/lib.rs"
assert "true" "rust" "crates/delta-connect/Cargo.toml"
assert "true" "rust" "crates/delta-sync/src/lib.rs"
assert "true" "rust" "crates/delta-sync/Cargo.toml"

# 6. packaging/portable/launcher/* Rust
assert "true" "rust" "packaging/portable/launcher/src/main.rs"
assert "true" "rust" "packaging/portable/launcher/Cargo.toml"

# 7. providers, integrations, packages, apps/tui
assert "true" "python" "providers/example.py"
assert "true" "python" "integrations/example.py"
assert "true" "python" "packages/example.py"
assert "true" "python" "apps/tui/main.py"

# 8. test files
assert "true" "python" "tests/test_ledger.py"
assert "true" "python" "tests/test_rust_idemlog_writer.py"

# 9. .github/workflows forces full
assert "true" "full" ".github/workflows/ci.yml"
# And full triggers rust
result="$(printf '%s\n' ".github/workflows/ci.yml" | bash "$SELECTOR")"
echo "$result" | grep -q "^rust=true" || { echo "FAIL: full did not force rust=true" >&2; FAIL=$((FAIL + 1)); }

# 10. deny.toml forces rust_advisories
assert "true" "rust_advisories" "deny.toml"

# 11. pyproject.toml + uv.lock trigger python_advisories
assert "true" "python_advisories" "pyproject.toml"
assert "true" "python_advisories" "uv.lock"

# 12. packaging/* (non-Rust) forces full
assert "true" "full" "packaging/scripts/release.sh"

# 13. CRITICAL: crates/delta-core/* must NEVER be python-only.
# A non-empty path under delta-core must produce rust=true.
echo "--- critical anti-shadow assertion ---"
for path in \
  "crates/delta-core/src/ledger.rs" \
  "crates/delta-core/src/idemlog.rs" \
  "crates/delta-core/src/taskstore.rs" \
  "crates/delta-core/Cargo.toml" \
  "crates/delta-core/Cargo.lock" \
  "crates/delta-core/src/bin/verify_ledger.rs" \
  "crates/delta-core/src/bin/dump_idemlog.rs" \
  "crates/delta-core/src/bin/inspect_tasks.rs" \
  "crates/delta-core/src/bin/write_idemlog.rs" \
  "crates/delta-core/src/bin/write_ledger.rs" \
  "crates/delta-core/src/bin/write_tasks.rs" \
  "crates/delta-core/src/lib.rs"; do
  result="$(printf '%s\n' "$path" | bash "$SELECTOR")"
  rust_val="$(echo "$result" | grep '^rust=' | cut -d'=' -f2)"
  if [ "$rust_val" != "true" ]; then
    echo "FAIL: $path did NOT set rust=true (got rust=$rust_val)" >&2
    FAIL=$((FAIL + 1))
  else
    echo "PASS: $path → rust=true (not shadowed)"
  fi
done

echo "--- multi-path accumulation ---"
# 14. Mixed paths accumulate flags (any rust path triggers rust=true)
result="$(printf '%s\n' "core/ledger.py" "crates/delta-core/src/lib.rs" | bash "$SELECTOR")"
echo "$result" | grep -q "^rust=true" || { echo "FAIL: mixed paths should set rust=true" >&2; FAIL=$((FAIL + 1)); }
echo "$result" | grep -q "^python=true" || { echo "FAIL: mixed paths should set python=true" >&2; FAIL=$((FAIL + 1)); }
echo "PASS: mixed paths accumulate correctly"

if [ "$FAIL" -gt 0 ]; then
  echo "--- $FAIL assertion(s) FAILED ---" >&2
  exit 1
fi

echo "--- all assertions passed ---"
exit 0
