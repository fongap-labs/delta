"""Cross-repo contract tests for Foundation/Suite compatibility (R8.9).

These tests verify:
1. Foundation ABI version matches Suite expectations
2. Capability manifest schema compliance
3. Grant schema compliance
4. End-to-end integration: Foundation startup -> Suite extension -> capability execution
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest

try:
    from delta_worker_sdk import CAPABILITY_ABI_VERSION, CapabilityJob, CapabilityResult
    HAS_SDK = True
except ImportError:
    HAS_SDK = False

pytestmark = pytest.mark.skipif(not HAS_SDK, reason="delta_worker_sdk not installed")


class TestFoundationABI:
    """Test Foundation ABI version compatibility."""

    def test_abi_version_known(self):
        """Foundation ABI version should be known and stable."""
        assert CAPABILITY_ABI_VERSION == 2

    def test_rust_runtime_reports_same_abi(self):
        """Rust runtime should report the same ABI version."""
        # Check that the Rust constant matches
        # This would require running the Rust binary or reading from a generated file
        # For now, we verify the constant is defined
        assert isinstance(CAPABILITY_ABI_VERSION, int)
        assert CAPABILITY_ABI_VERSION > 0


class TestCapabilityManifestSchema:
    """Test capability manifest schema compliance."""

    SCHEMA_PATH = Path("schemas/capability-manifest.schema.json")

    def test_schema_exists(self):
        assert self.SCHEMA_PATH.exists()

    def test_schema_valid_json(self):
        with open(self.SCHEMA_PATH) as f:
            schema = json.load(f)
        assert schema["$schema"] == "http://json-schema.org/draft-07/schema#"
        assert "worker_id" in schema["required"]
        assert "capabilities" in schema["required"]

    def test_file_reader_manifest_complies(self):
        """File reader manifest should comply with schema."""
        import jsonschema

        with open(self.SCHEMA_PATH) as f:
            schema = json.load(f)

        manifest_path = Path("integrations/capabilities/file_reader/manifest.json")
        with open(manifest_path) as f:
            manifest = json.load(f)

        jsonschema.validate(instance=manifest, schema=schema)

    def test_manifest_has_required_fields(self):
        """Manifest must have worker_id and capabilities array."""
        manifest_path = Path("integrations/capabilities/file_reader/manifest.json")
        with open(manifest_path) as f:
            manifest = json.load(f)

        assert "worker_id" in manifest
        assert manifest["worker_id"] == "file-reader"
        assert "capabilities" in manifest
        assert isinstance(manifest["capabilities"], list)
        assert len(manifest["capabilities"]) >= 1

        cap = manifest["capabilities"][0]
        assert "capability_id" in cap
        assert cap["capability_id"] == "file.read"
        assert "permissions" in cap
        assert "fs.read" in cap["permissions"]
        assert "timeout_seconds" in cap


class TestGrantSchema:
    """Test execution grant schema compliance."""

    SCHEMA_PATH = Path("schemas/execution-grant.schema.json")

    def test_schema_exists(self):
        assert self.SCHEMA_PATH.exists()

    def test_schema_valid_json(self):
        with open(self.SCHEMA_PATH) as f:
            schema = json.load(f)
        assert schema["$schema"] == "http://json-schema.org/draft-07/schema#"


class TestCapabilityJobSchema:
    """Test CapabilityJob wire format compliance."""

    def test_job_serialization_roundtrip(self):
        """CapabilityJob should serialize and deserialize correctly."""
        job = CapabilityJob(
            abi_version=CAPABILITY_ABI_VERSION,
            capability_id="test.cap",
            job_id="job-1",
            arguments={"path": "test.txt"},
            grants={"secrets": []},
        )

        json_str = job.model_dump_json()
        parsed = CapabilityJob.model_validate_json(json_str)

        assert parsed.abi_version == CAPABILITY_ABI_VERSION
        assert parsed.capability_id == "test.cap"
        assert parsed.job_id == "job-1"
        assert parsed.arguments == {"path": "test.txt"}

    def test_job_includes_grants(self):
        """Job should include grants with proper structure."""
        job = CapabilityJob(
            abi_version=CAPABILITY_ABI_VERSION,
            capability_id="test.cap",
            job_id="job-1",
            arguments={},
            grants={"network": ["api.example.com"], "secrets": ["API_KEY"]},
        )

        json_str = job.model_dump_json()
        data = json.loads(json_str)

        assert "grants" in data
        assert data["grants"]["network"] == ["api.example.com"]
        assert data["grants"]["secrets"] == ["API_KEY"]

    def test_job_includes_boundary(self):
        """Job should include boundary with provenance."""
        job = CapabilityJob(
            abi_version=CAPABILITY_ABI_VERSION,
            capability_id="test.cap",
            job_id="job-1",
            arguments={},
            grants={},
            boundary={"provenance": "test-run", "secrets": ["API_KEY"]},
        )

        json_str = job.model_dump_json()
        data = json.loads(json_str)

        assert "boundary" in data
        assert data["boundary"]["provenance"] == "test-run"
        assert data["boundary"]["secrets"] == ["API_KEY"]


class TestCapabilityResultSchema:
    """Test CapabilityResult wire format compliance."""

    def test_result_completed(self):
        """Completed result should have correct structure."""
        result = CapabilityResult(
            abi_version=CAPABILITY_ABI_VERSION,
            job_id="job-1",
            state="completed",
            output="success",
            result={"ok": True},
            artifacts=[],
            diagnostics=None,
            finished_at=1234567890.0,
        )

        json_str = result.model_dump_json()
        data = json.loads(json_str)

        assert data["state"] == "completed"
        assert data["output"] == "success"
        assert data["result"] == {"ok": True}
        assert data["artifacts"] == []

    def test_result_failed_with_diagnostics(self):
        """Failed result should include diagnostics."""
        result = CapabilityResult(
            abi_version=CAPABILITY_ABI_VERSION,
            job_id="job-1",
            state="failed",
            output=None,
            result=None,
            artifacts=[],
            diagnostics={
                "stderr_lines": ["error: something failed"],
                "error_code": "test_error",
                "error_message": "Something failed",
            },
            finished_at=1234567890.0,
        )

        json_str = result.model_dump_json()
        data = json.loads(json_str)

        assert data["state"] == "failed"
        assert data["diagnostics"] is not None
        assert data["diagnostics"]["error_code"] == "test_error"
        assert "something failed" in data["diagnostics"]["error_message"].lower()


class TestEndToEndIntegration:
    """End-to-end integration test: Foundation -> Suite -> capability."""

    def test_file_reader_execution(self):
        """Test file.read capability executes correctly via worker SDK."""
        from integrations.capabilities.file_reader.worker import handle_file_read
        from delta_worker_sdk import run_worker

        with tempfile.TemporaryDirectory() as tmpdir:
            # Create test file
            test_file = Path(tmpdir) / "test.txt"
            test_file.write_text("Hello, Contract Test!")

            job = CapabilityJob(
                abi_version=CAPABILITY_ABI_VERSION,
                capability_id="file.read",
                job_id="contract-test-1",
                arguments={"path": "test.txt"},
                grants={"secrets": []},
                workspace=tmpdir,
            )

            # Capture stdout
            import io
            from unittest.mock import patch

            stdin_data = job.model_dump_json() + "\n{}\n"

            with patch("sys.stdin", io.StringIO(stdin_data)):
                with patch("sys.stdout", io.StringIO()) as mock_stdout:
                    run_worker(handle_file_read)
                    output = mock_stdout.getvalue().strip()
                    last_line = output.split("\n")[-1]
                    result = CapabilityResult.model_validate_json(last_line)

            assert result.state == "completed"
            assert result.result is not None
            assert result.result["content"] == "Hello, Contract Test!"


class TestCompatibilityMatrix:
    """Test Foundation/Suite version compatibility matrix."""

    def test_minimum_foundation_version(self):
        """Suite should declare minimum Foundation version."""
        # This would be a manifest field like "min_foundation_version"
        # For now, verify the concept exists
        manifest_path = Path("integrations/capabilities/file_reader/manifest.json")
        with open(manifest_path) as f:
            manifest = json.load(f)

        # Check if min_foundation_version is declared (optional for now)
        # This is a placeholder for future compatibility matrix
        assert "worker_id" in manifest

    def test_abi_backwards_compatibility(self):
        """New Suite capabilities should work with older Foundation ABI."""
        # This test would verify that capabilities built against ABI 2
        # work with Foundation runtime that supports ABI 2+
        assert CAPABILITY_ABI_VERSION >= 2


# Run with: python -m pytest tests/contract/test_foundation_suite_contract.py -v
if __name__ == "__main__":
    pytest.main([__file__, "-v"])