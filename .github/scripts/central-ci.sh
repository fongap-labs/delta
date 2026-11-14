#!/usr/bin/env bash
# Bootstrap CI only. Replace this gate with project-specific build and test
# checks in the same PR that introduces the first product source files.
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
    "CHANGELOG.md",
    ".github/execution-manifest.json",
    ".github/scripts/central-ci.sh",
    ".github/workflows/notify-action-worker.yml",
}
tracked = set(
    item.decode("utf-8")
    for item in subprocess.check_output(["git", "-C", str(root), "ls-files", "-z"]).split(b"\0")
    if item
)
missing = sorted(required - tracked)
unexpected = sorted(tracked - required)
if missing:
    raise SystemExit(f"central-ci: required bootstrap files missing: {', '.join(missing)}")
if unexpected:
    raise SystemExit(
        "central-ci: source/build files require real project CI in the same PR: "
        + ", ".join(unexpected)
    )

for name in required:
    path = root / name
    if not path.is_file() or not path.read_bytes().strip():
        raise SystemExit(f"central-ci: missing or empty file: {name}")

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
print("central-ci: bootstrap structure validated; no product source is present")
PY
