"""Contracts for manual private release builds without publishing."""

import json
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
WORKFLOW = REPO / ".github" / "workflows" / "private-release-build.yml"
MANIFEST = REPO / "projects" / "delta" / "release.manifest.json"
AW_SHA = "1f46ad902cf0e9391ce4bf9f6394b776577405d8"


class PrivateReleaseBuildContractTests(unittest.TestCase):
    def test_manual_private_workflow_pins_preflight(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("workflow_dispatch:", text)
        self.assertIn("permissions:\n  contents: read", text)
        self.assertIn("private-release-preflight.yml@" + AW_SHA, text)
        self.assertIn("action_worker_sha: " + AW_SHA, text)
        self.assertIn("if: needs.preflight.result == 'success'", text)
        self.assertIn('"refs/heads/main"', text)
        self.assertIn('"workflow_dispatch"', text)
        self.assertIn('"true"', text)
        self.assertIn("ref: ${{ github.sha }}", text)
        self.assertIn("persist-credentials: false", text)

    def test_no_public_artifact_or_privileged_deploy(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        for denied in (
            "secrets:", "secrets.", "actions/upload-artifact",
            "actions/attest", "id-token: write", "contents: write",
            "repository_dispatch:", "environment:", "gh release", "gh api",
        ):
            self.assertNotIn(denied, text, denied)
        self.assertIn("Delete transient release outputs", text)
        self.assertIn("if: always()", text)

    def test_builds_are_approved_in_source_manifest(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        text = WORKFLOW.read_text(encoding="utf-8")
        approved = {
            "windows-portable": "scripts/release-build.ps1",
            "python-runtime": "scripts/release-build-runtime.ps1",
        }
        self.assertEqual(
            {build["id"] for build in manifest["builds"]}, set(approved)
        )
        for build in manifest["builds"]:
            self.assertEqual(build["script"], approved[build["id"]])
            self.assertEqual(build["runner_profile"], "windows-build")
            self.assertEqual(build["target"], "x86_64-pc-windows-msvc")
            self.assertTrue((MANIFEST.parent / build["script"]).is_file())
            self.assertIn(build["script"], text)
            self.assertIn(build["id"], text)
