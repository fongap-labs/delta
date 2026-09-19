"""Behavioral tests for the Delta credential vault.

Validates the security model: round-trip persistence, ``${VAR}`` resolution
from environment and ``.env`` files, status metadata that never leaks values,
OS-level file restriction (POSIX 0600 / Windows ACL), degraded-ACL marker,
and corruption-tolerant load.
"""

from __future__ import annotations

import os
import stat
import subprocess
import sys
import time

import packages.credential_store as vault
from packages.credential_store import CredentialStore


def test_round_trip_persistence(tmp_path):
    store = CredentialStore(tmp_path / "secrets.json")
    store.put("slack:default", {"type": "token", "bot_token": "xoxb-123"})
    assert store.get("slack:default") == {"type": "token", "bot_token": "xoxb-123"}
    assert store.get("nonexistent") is None


def test_env_var_substitution(tmp_path, monkeypatch):
    monkeypatch.setenv("MY_TOK", "from-env")
    store = CredentialStore(tmp_path / "secrets.json")
    store.put("slack:default", {"type": "token", "bot_token": "${MY_TOK}"})
    assert store.get("slack:default")["bot_token"] == "from-env"


def test_dotenv_substitution(tmp_path):
    (tmp_path / ".env").write_text('DOCS_TOKEN = "shhh"' + "\n", encoding="utf-8")
    store = CredentialStore(tmp_path / "secrets.json")
    store.put("docs:default", {"headers": {"Authorization": "Bearer ${DOCS_TOKEN}"}})
    assert store.get("docs:default")["headers"]["Authorization"] == "Bearer shhh"


def test_unresolvable_ref_stays_literal(tmp_path):
    store = CredentialStore(tmp_path / "secrets.json")
    store.put("x", {"v": "${NOPE_NOT_SET}"})
    assert store.get("x")["v"] == "${NOPE_NOT_SET}"


def test_status_leaks_no_values(tmp_path):
    store = CredentialStore(tmp_path / "secrets.json")
    store.put(
        "gmail:default",
        {
            "type": "oauth",
            "access": "secret",
            "account_id": "me@x.com",
            "expires": time.time() - 10,
        },
    )
    store.put("slack:default", {"type": "token", "bot_token": "xoxb"})
    rows = {row["profile"]: row for row in store.status()}
    assert rows["gmail:default"]["type"] == "oauth"
    assert rows["gmail:default"]["account"] == "me@x.com"
    assert rows["gmail:default"]["expired"] is True
    assert rows["slack:default"]["expired"] is False
    blob = str(store.status())
    assert "secret" not in blob and "xoxb" not in blob


def test_vault_file_user_restricted(tmp_path):
    """The vault file must be restricted to the current user.
    POSIX: mode 0600; Windows: ACL with inheritance stripped."""
    path = tmp_path / "secrets.json"
    CredentialStore(path).put("x", {"a": 1})
    if sys.platform == "win32":
        out = subprocess.run(
            ["icacls", str(path)], capture_output=True, text=True
        ).stdout
        user = os.environ.get("USERNAME", "")
        assert user and user in out
        assert "NT AUTHORITY\\SYSTEM" not in out
        assert "BUILTIN\\Administrators" not in out
    else:
        assert stat.S_IMODE(os.stat(path).st_mode) == 0o600


def test_delete_returns_existence(tmp_path):
    store = CredentialStore(tmp_path / "secrets.json")
    store.put("x", {"a": 1})
    assert store.delete("x") is True
    assert store.delete("x") is False
    assert store.get("x") is None


def test_degraded_acl_marker_when_verification_fails(tmp_path, monkeypatch):
    """When ACL hardening cannot be verified, saving must still succeed but the
    degraded state must be persisted (marker file) so callers/UI can surface it.

    On a non-Windows runner, icacls does not exist so the apply subprocess.run
    raises OSError and _apply_user_restriction returns False via its except
    branch without reaching the mocked verifier.  Simulate the Windows shell:
    the apply call succeeds (no raise), leaving the mocked _verify_windows_acl
    as the sole verify oracle."""

    def _fake_icacls_apply(args, *a, **kw):
        return subprocess.CompletedProcess(args=args, returncode=0, stdout="", stderr="")

    path = tmp_path / "secrets.json"
    store = CredentialStore(path)
    monkeypatch.setattr(vault, "_ON_WINDOWS", True)
    monkeypatch.setenv("USERNAME", "testuser")
    monkeypatch.setattr(vault.subprocess, "run", _fake_icacls_apply)
    monkeypatch.setattr(vault, "_verify_windows_acl", lambda p: False)
    store.put("x", {"a": 1})
    assert store.get("x") == {"a": 1}
    assert store.acl_unprotected() is True
    # A later verified write clears the degraded flag.
    monkeypatch.setattr(vault, "_verify_windows_acl", lambda p: True)
    store.put("y", {"b": 2})
    assert store.acl_unprotected() is False


def test_corrupt_file_preserved_not_overwritten(tmp_path):
    """A corrupt vault file must be preserved as a .corrupt-<ts> sibling before
    a later save can overwrite it; loading degrades to empty state."""
    path = tmp_path / "secrets.json"
    path.write_text('{"slack": {"bot_token": "xoxb', encoding="utf-8")
    store = CredentialStore(path)
    assert store._load() == {}
    backups = list(tmp_path.glob("secrets.json.corrupt-*"))
    assert len(backups) == 1
    assert backups[0].read_text(encoding="utf-8").startswith('{"slack"')
    store.put("x", {"a": 1})
    assert store.get("x") == {"a": 1}
    assert len(list(tmp_path.glob("secrets.json.corrupt-*"))) == 1


def test_healthy_store_reports_acl_protected(tmp_path):
    """Happy path: a normal write verifies protection and sets no degraded flag."""
    path = tmp_path / "secrets.json"
    store = CredentialStore(path)
    store.put("x", {"a": 1})
    assert store.acl_unprotected() is False
