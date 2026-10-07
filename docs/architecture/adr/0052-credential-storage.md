# ADR-0052: Credential storage

Status:

**Step 1 accepted and implemented** (owner, 2026-10-07; delta PR "fix: create credential files owner-only and judge Windows ACLs by SID"). **Step 2 is proposed and waits for the owner's decision.** (Audit finding DL-002.)

## Context

Where credentials live today (checked in the code, not assumed):

1. **Rust runtime (the product path).** `crates/delta-core/src/model_authority.rs` keeps provider API keys in the `secrets` map of `<state>/model-authority.json`; `crates/delta-core/src/application.rs` keeps connector credentials in `<state>/application.json`. Both are written by `durability::atomic_write_private`, which on Unix creates the temp file with mode `0600` (`create_new`) and renames it. **On Windows `open_temp_file` ignores the "private" flag: no ACL is set.** Protection there is whatever the parent directory inherits. The Tauri command `update_model_key` (`apps/desktop/src-tauri/src/runtime_ipc.rs`) goes through this Rust path, so the Rust side is the main store, not a leftover.
2. **Python vault (`packages/credential_store.py`).** Used by the connector code (`integrations/connectors/email_tools.py`, `delta_extension_api/credentials.py`) for the mail app password and OAuth tokens in `<state>/secrets.json`. The file is written first (`write_text`) and only afterwards restricted: POSIX `chmod 0600`, Windows `icacls /inheritance:r /grant:r <user>:F`. The parent directory is restricted first, so the exposure window is small, but exceptions while restricting the directory are swallowed. The Windows check `_verify_windows_acl` looks for the English strings `NT AUTHORITY\SYSTEM` and `BUILTIN\Administrators` in `icacls` output; on a localised Windows the group names differ, so the check can report "protected" when it is not (or the reverse). A failed check is recorded in a `*.acl-unprotected` marker file, but nothing in the UI reads it.
3. **State directory.** `DELTA_STATE_DIR`, else `%APPDATA%\delta` (Windows) or `~/.config/delta`. In the **portable** build the launcher sets `DELTA_STATE_DIR` to `<folder>\Data`, so the data moves with the folder (a USB stick, a synced or shared directory, another user's PC).

The module comment says a Keychain/age back end is planned. All values are stored as plain JSON text.

## Decision drivers

- Windows is the only released target today (`x86_64-pc-windows-msvc`), macOS/Linux builds exist in CI.
- The portable edition is a product promise: the folder moves, the app does not touch the registry. Any OS-bound encryption (DPAPI, Keychain) cannot decrypt on another machine or user.
- One authority: the Rust runtime owns credentials (`core_control_plane.rs`: "Rust ApplicationStore remains the sole product credential authority"). The Python vault should not grow its own scheme.
- Headless and CI runs have no desktop keyring.

## Options

**A. OS credential store** (Windows Credential Manager/DPAPI, macOS Keychain, Linux Secret Service), for example through the Rust `keyring` crate; the JSON keeps only a reference.
- Plus: standard protection; secrets not in the folder; no file ACL to maintain.
- Minus: not portable (secrets stay on the machine, so a moved portable folder loses its keys and the user re-enters them); Linux needs a running Secret Service (not available headless/CI); a new dependency in the signed binary and its audit surface; the Python side must reach the Rust store (IPC or CLI) instead of reading a file.

**B. Master key wrapped by the platform (DPAPI on Windows, Keychain on macOS), secrets in an encrypted file.**
- Plus: the data file can be copied without exposing secrets; one key to protect instead of many entries.
- Minus: the same portability loss as A (the wrapped key does not open elsewhere); more code (format, versioning, key rotation, recovery); a corrupt wrapped key makes every secret unreadable.

**C. Harden the files only (today's model, fixed properly).**
- Plus: portable-friendly; no new dependency; smallest change.
- Minus: secrets are readable by anything running as the same user and by anyone who gets the folder (backups, sync, a lost USB stick). It is "protection from other local users", not encryption.

**D. Optional passphrase-based encryption for the portable edition (age/scrypt style), with A for installed builds.**
- Plus: honours the portable promise (the passphrase travels in the user's head); installed builds get OS protection.
- Minus: two modes to build and test; a passphrase prompt in the UI; losing the passphrase loses the keys (same as losing them today if the folder is lost).

## Recommendation

Do it in two steps so the urgent gap closes first and the design question stays open:

1. **Step 1 (no format change; I recommend approving this now).** Make option C correct on every path:
   - Rust `atomic_write_private`: on Windows create the temp file already restricted (a `SECURITY_ATTRIBUTES` descriptor granting only the current user's SID, or create it inside a directory whose ACL was restricted first), so a file is never visible with inherited permissions. POSIX keeps `create_new` + `0600`.
   - Python `write_private_text`/`_persist`: create with `os.open(path, O_WRONLY|O_CREAT|O_EXCL, 0o600)` on POSIX; on Windows restrict the (new) parent first and then write. Replace the English-name substring check by comparing **SIDs** (the user's SID and `S-1-5-18`, `S-1-5-32-544` must not appear as grants other than the user's). Do not swallow errors: expose `acl_unprotected` in the UI status.
   - One-time migration: nothing moves; existing files are re-applied the restriction on next write.
   - Tests: a fresh file has no inherited ACE (Windows) / mode `0600` (POSIX); a file that cannot be restricted sets the flag; the unprotected flag reaches the UI status.
2. **Step 2 (design decision, needs you).** Choose between A, D or "C is enough" after considering that installed users and portable users have different needs. My lean: **D for the portable edition and A for installed builds**, behind one Rust trait (`CredentialBackend`) with the file as the fallback, so headless runs and CI keep working. Migration then imports the old JSON, verifies it can read it back, and overwrites the plaintext with a restricted file containing references only. I would not start this before the owner decides on the portable trade-off, because it changes what "portable" means.

## Consequences

Step 1: no data format change; no breaking change for portable. Moves nothing out of the folder, so a stolen folder still exposes keys; the README/docs should say so honestly until step 2.
Step 2 (if chosen): portable semantics change (keys no longer travel unless protected by passphrase), new dependency, a migration that must be idempotent and must not delete the plaintext until the new copy is read back successfully; failure path when no keyring is available must be a visible warning, not a silent fallback.

## Questions for the owner

1. Approve Step 1 now?
2. For Step 2: is "keys do not travel with a portable folder unless protected by a passphrase" acceptable? If not, only C/D remain.
3. Is Linux/macOS support of interest enough to include the Secret Service/Keychain paths in the first version?

## Not verified

Behaviour of `icacls` on a localised Windows was reasoned from the code, not reproduced. Windows ACL creation through `SECURITY_ATTRIBUTES` has not been prototyped here.
