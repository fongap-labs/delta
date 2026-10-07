"""Delta credential vault -- local file-backed credential storage.

Security model (Delta-native):

* Credentials never enter model context, prompts, or traces.
* The vault holds profiles keyed by ``connector[:account]``; values may be
  literals or ``${ENV_VAR}`` references resolved at read time from the process
  environment and the local ``.env`` file.
* The backing file is protected to the current user only (POSIX 0600 / Windows
  ACL with inheritance stripped).  Write-path verifies the restriction took
  effect and records a marker when it could not.
* A corrupt file degrades to an empty vault (never crashes the process) and is
  preserved as a ``.corrupt-<ts>`` sibling so the user can recover.

The interface is what callers depend on; a Keychain or age-encrypted backend
can swap in later without touching them.
"""

from __future__ import annotations

import json
import logging
import os
import re
import stat
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any

from packages.jsonstate import load_json_state

logger = logging.getLogger(__name__)

_ENV_REF = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")
_ON_WINDOWS = sys.platform == "win32"


def state_dir() -> Path:
    """Where Delta keeps its state -- the one cross-platform source of truth.

    Resolution order:
    1. ``$DELTA_STATE_DIR`` -- explicit override on any OS (used by tests/sidecars).
    2. Windows: ``%APPDATA%\\delta`` (native per-user app-data location).
    3. macOS / Linux: ``~/.config/delta`` (XDG-style).
    """
    override = os.environ.get("DELTA_STATE_DIR")
    if override:
        return Path(override).expanduser()
    if _ON_WINDOWS:
        appdata = os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "delta"
    return Path.home() / ".config" / "delta"


def _parse_env_file(path: Path) -> dict[str, str]:
    """Read a ``.env`` file into a dict (``KEY=value`` lines, comments and blanks skipped)."""
    entries: dict[str, str] = {}
    if not path.is_file():
        return entries
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        entries[key.strip()] = value.strip().strip('"').strip("'")
    return entries


def _system_tool(name: str) -> str:
    """Full path of a Windows system tool; PATH may start with Git or MSYS tools of the same name."""
    root = os.environ.get("SystemRoot", r"C:\Windows")
    return str(Path(root) / "System32" / f"{name}.exe")


def _current_user_sid() -> str | None:
    """SID of the current Windows user from ``whoami /user`` (same on every system language).

    None when it cannot be determined; callers then fall back to the account name.
    """
    try:
        proc = subprocess.run(
            [_system_tool("whoami"), "/user", "/fo", "csv", "/nh"],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    if proc.returncode != 0:
        return None
    match = re.search(r"S-1-\d+(?:-\d+)+", proc.stdout or "")
    return match.group(0) if match else None


def _dacl_grants_only(sddl: str, user_sid: str) -> bool:
    """True when the DACL is protected, has no inherited entry and grants only ``user_sid``.

    ``sddl`` is one security descriptor, e.g. ``D:PAI(A;;FA;;;S-1-5-21-...)``.  Trustees are
    compared as SIDs, never by account name, so the answer does not depend on the Windows language.
    """
    start = sddl.find("D:")
    if start < 0:
        return False
    dacl = sddl[start + 2 :]
    first = dacl.find("(")
    flags = dacl if first < 0 else dacl[:first]
    if "P" not in flags:
        return False
    grants = 0
    for entry in re.findall(r"\(([^)]*)\)", dacl):
        fields = entry.split(";")
        if len(fields) < 6:
            return False
        ace_type, ace_flags, trustee = fields[0], fields[1], fields[5]
        if "ID" in ace_flags:
            return False
        if ace_type.startswith("A"):
            if trustee.upper() != user_sid.upper():
                return False
            grants += 1
    return grants > 0


def _read_windows_sddl(path: Path) -> str | None:
    """Security descriptor of ``path`` as SDDL text via ``icacls /save`` (None if unreadable)."""
    handle, saved = tempfile.mkstemp(suffix=".acl")
    os.close(handle)
    try:
        proc = subprocess.run(
            [_system_tool("icacls"), str(path), "/save", saved, "/q"],
            capture_output=True,
            check=False,
        )
        if proc.returncode != 0:
            return None
        raw = Path(saved).read_bytes()
    except OSError:
        return None
    finally:
        try:
            os.unlink(saved)
        except OSError:
            pass
    if raw.startswith(b"\xff\xfe") or raw[1:2] == b"\x00":
        text = raw.decode("utf-16", errors="replace")
    else:
        text = raw.decode("utf-8", errors="replace")
    # Line 1 is the path (it can contain "D:"), so only later lines are descriptors.
    lines = text.splitlines()[1:]
    return next((line for line in lines if "D:" in line), None)


def _apply_user_restriction(path: Path, *, is_dir: bool) -> bool:
    """Restrict a path to the current user and verify it took effect.

    POSIX uses mode bits (0700 dir / 0600 file).  Windows has no mode bits --
    ``os.chmod`` only toggles read-only, so an ACL is required: strip inherited
    entries and grant the current user alone, by SID.  Best-effort on Windows so a
    transient ``icacls`` failure never blocks saving a key.

    Returns True when the restriction is verified in place, False when applied
    best-effort but unconfirmed (degraded -- callers should surface that).
    """
    if _ON_WINDOWS:
        sid = _current_user_sid()
        if sid:
            account = f"*{sid}"
        else:
            username = os.environ.get("USERNAME")
            if not username:
                return False
            domain = os.environ.get("USERDOMAIN")
            account = f"{domain}\\{username}" if domain else username
        # Directory grants must be inheritable ((OI)(CI)) so child files inherit.
        grant = f"{account}:(OI)(CI)F" if is_dir else f"{account}:F"
        try:
            subprocess.run(
                [_system_tool("icacls"), str(path), "/inheritance:r", "/grant:r", grant],
                capture_output=True,
                check=False,
            )
        except OSError:
            return False
        return _verify_windows_acl(path)
    os.chmod(path, 0o700 if is_dir else 0o600)
    try:
        return stat.S_IMODE(os.stat(path).st_mode) == (0o700 if is_dir else 0o600)
    except OSError:
        return False


def _verify_windows_acl(path: Path) -> bool:
    """Re-read the ACL and confirm that only the current user's SID is granted access."""
    sid = _current_user_sid()
    if sid is None:
        return False
    sddl = _read_windows_sddl(path)
    return sddl is not None and _dacl_grants_only(sddl, sid)


def _create_private_file(path: Path, content: str) -> None:
    """Create ``path`` fresh, owner-only from the first byte on POSIX (mode 0600, ``O_EXCL``).

    On Windows the new file inherits the folder's access, which the caller has restricted first,
    and is then restricted itself by ``_apply_user_restriction``.
    """
    path.unlink(missing_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(content)


def write_private_text(path: str | Path, content: str) -> Path:
    """Atomically write a user-only text file using the vault's OS protections."""
    target = Path(path).expanduser()
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        _apply_user_restriction(target.parent, is_dir=True)
    except OSError:
        pass
    tmp = target.with_name(target.name + ".tmp")
    _create_private_file(tmp, content)
    _apply_user_restriction(tmp, is_dir=False)
    os.replace(tmp, target)
    return target


def verify_user_restricted(path: str | Path) -> bool:
    """Verify (without mutating) that ``path`` is restricted to the current user."""
    p = Path(path)
    if _ON_WINDOWS:
        return _verify_windows_acl(p)
    try:
        return stat.S_IMODE(os.stat(p).st_mode) == 0o600
    except OSError:
        return False


class CredentialStore:
    """File-backed credential vault.  Reads resolve ``${VAR}`` refs; status never
    leaks values."""

    def __init__(self, path: str | Path | None = None) -> None:
        self.path = Path(path).expanduser() if path else state_dir() / "secrets.json"
        self._env_file = self.path.parent / ".env"
        self._lock = threading.Lock()

    # -- reads ------------------------------------------------------------------
    def get(self, profile: str) -> dict[str, Any] | None:
        """Return a profile with ``${VAR}`` refs resolved, or None if absent."""
        entry = self._load().get(profile)
        if entry is None:
            return None
        return self.resolve(entry)

    def resolve(self, value: Any) -> Any:
        """Resolve ``${VAR}`` refs in a value (recursively) from env + local .env."""
        env_vars = _parse_env_file(self._env_file)

        def _substitute(node: Any) -> Any:
            if isinstance(node, str):
                return _ENV_REF.sub(
                    lambda m: os.environ.get(m.group(1))
                    or env_vars.get(m.group(1))
                    or m.group(0),
                    node,
                )
            if isinstance(node, dict):
                return {k: _substitute(v) for k, v in node.items()}
            if isinstance(node, list):
                return [_substitute(item) for item in node]
            return node

        return _substitute(value)

    def status(self) -> list[dict[str, Any]]:
        """Profile metadata only -- never the secret values themselves."""
        out: list[dict[str, Any]] = []
        for profile, data in self._load().items():
            data = data if isinstance(data, dict) else {}
            expires = data.get("expires")
            expired = isinstance(expires, (int, float)) and expires < time.time()
            out.append(
                {
                    "profile": profile,
                    "type": data.get("type"),
                    "account": data.get("account_id"),
                    "expired": bool(expired),
                    "acl_unprotected": self.acl_unprotected(),
                }
            )
        return out

    # -- writes -----------------------------------------------------------------
    def put(self, profile: str, data: dict[str, Any]) -> None:
        with self._lock:
            vault = self._load()
            vault[profile] = data
            self._persist(vault)

    def delete(self, profile: str) -> bool:
        with self._lock:
            vault = self._load()
            if profile not in vault:
                return False
            del vault[profile]
            self._persist(vault)
            return True

    # -- internals --------------------------------------------------------------
    def _load(self) -> dict[str, Any]:
        # Tolerant load: a corrupt file degrades to empty state but is preserved
        # as a .corrupt-<ts> sibling so the next save cannot overwrite the only
        # copy of the user's credentials.
        return load_json_state(self.path, {})

    def _persist(self, vault: dict[str, Any]) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        # The folder is restricted first so the new file is not briefly visible to others. A folder
        # that cannot be restricted (or keeps extra grants) is logged, not hidden; the file's own
        # restriction decides the reported state, because that is what protects the secrets.
        try:
            if not _apply_user_restriction(self.path.parent, is_dir=True):
                logger.warning("the credential folder %s is not confirmed owner-only", self.path.parent)
        except OSError as error:
            logger.warning("could not restrict the credential folder %s: %s", self.path.parent, error)
        tmp = self.path.with_name(self.path.name + ".tmp")
        _create_private_file(tmp, json.dumps(vault, indent=2))
        is_file_protected = _apply_user_restriction(tmp, is_dir=False)
        os.replace(tmp, self.path)
        self._record_acl_state(is_file_protected)

    def _record_acl_state(self, ok: bool) -> None:
        """Persist whether the vault file is ACL-protected so callers/UI can surface it."""
        marker = self._acl_marker_path()
        try:
            if ok:
                marker.unlink(missing_ok=True)
            else:
                logger.warning(
                    "credentials stored WITHOUT ACL protection (icacls hardening failed): %s",
                    self.path,
                )
                marker.write_text(time.strftime("%Y-%m-%dT%H:%M:%S") + "\n", encoding="utf-8")
        except OSError:
            pass

    def _acl_marker_path(self) -> Path:
        return self.path.with_name(self.path.name + ".acl-unprotected")

    def acl_unprotected(self) -> bool:
        """True when the last write could not verify OS-level protection on this file."""
        return self._acl_marker_path().is_file()
