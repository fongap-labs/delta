#!/usr/bin/env bash
set -Eeuo pipefail
TARGET_ROOT="${1:-}"
if [[ -z "$TARGET_ROOT" || ! -d "$TARGET_ROOT/.git" ]]; then
  echo "central-ci: checked-out target root required" >&2
  exit 64
fi
cd "$TARGET_ROOT"
node scripts/delta-verify.mjs
# This is the initial product import: exercise real Harness compilation as
# well as the Delta integration/branding contract (not only metadata checks).
node -e 'const v = Number(process.versions.node.split(".")[0]); if (v < 24) process.exit(2)'
if command -v pnpm >/dev/null 2>&1; then
  pnpm install --frozen-lockfile
  pnpm run build:desktop
else
  echo "central-ci: pnpm 11.28.5 is required on the Action Worker runner" >&2
  exit 68
fi
