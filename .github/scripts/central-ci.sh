#!/usr/bin/env bash
# Governance-only bootstrap CI. When product sources arrive, replace this with real build/tests in the same PR.
set -Eeuo pipefail

TARGET_ROOT="${1:-}"
if [[ -z "$TARGET_ROOT" || ! -d "$TARGET_ROOT/.git" ]]; then
  echo "central-ci: checked-out target root is required" >&2
  exit 64
fi

python3 - "$TARGET_ROOT" <<'PY'
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1]).resolve()
required = {
    "README.md",
    "LICENSE",
    "AGENTS.md",
    "CLAUDE.md",
    "CONTRIBUTING.md",
    "docs/GOVERNANCE.md",
    "docs/ARCHITECTURE.md",
    "docs/MIGRATION_PLAN.md",
    "docs/BRAND.md",
    ".github/execution-manifest.json",
    ".github/scripts/central-ci.sh",
}
tracked = {
    item.decode("utf-8")
    for item in subprocess.check_output(["git", "-C", str(root), "ls-files", "-z"]).split(b"\0")
    if item
}
missing = sorted(required - tracked)
unexpected = sorted(tracked - required)
if missing:
    raise SystemExit("central-ci: required governance file missing: " + ", ".join(missing))
if unexpected:
    raise SystemExit(
        "central-ci: source/build files require real project CI in the same PR: "
        + ", ".join(unexpected)
    )

for name in required:
    path = root / name
    if not path.is_file() or not path.read_bytes().strip():
        raise SystemExit("central-ci: missing or empty file: " + name)

manifest = json.loads((root / ".github/execution-manifest.json").read_text(encoding="utf-8"))
jobs = manifest.get("operations", {}).get("ci", {}).get("jobs")
if manifest.get("schema_version") != "1" or not isinstance(jobs, list) or len(jobs) != 1:
    raise SystemExit("central-ci: invalid bootstrap execution manifest")
job = jobs[0]
if (
    job.get("id") != "linux"
    or job.get("runner_profile") != "linux-standard"
    or job.get("command") != ["bash", "{control_root}/.github/scripts/central-ci.sh", "{target_root}"]
    or job.get("capability_requests") != ["source.read"]
):
    raise SystemExit("central-ci: bootstrap CI contract mismatch")

readme = (root / "README.md").read_text(encoding="utf-8")
brand = (root / "docs/BRAND.md").read_text(encoding="utf-8")
governance = (root / "docs/GOVERNANCE.md").read_text(encoding="utf-8")
if "Delta Runtime" not in readme or "Harness Runtime" not in readme:
    raise SystemExit("central-ci: missing two-runtime architecture contract")
if "#286F78" not in brand or "#60B4BF" not in brand:
    raise SystemExit("central-ci: missing original Delta brand colors")
if "fongap-labs/action-worker" not in governance:
    raise SystemExit("central-ci: organization governance reference is required")
print("central-ci: governance-only baseline validated; product code requires real CI")
PY
