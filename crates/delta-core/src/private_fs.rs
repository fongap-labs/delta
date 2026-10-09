//! Owner-only access for the files that hold credentials.
//!
//! On Unix a private file has mode `0600` from the moment it is created (`durability.rs`). Windows
//! has no mode bits, so the file would otherwise carry whatever its folder grants. This module
//! restricts a file to the current user's SID (inheritance removed) and can verify the result. The
//! check compares SIDs read from the saved DACL, never account names, so it does not depend on the
//! language of the system.

use std::path::{Path, PathBuf};

/// SDDL writes the local Administrator (RID 500) as `LA` and the local Guest (RID 501) as `LG`;
/// the built-in administrator account of a CI runner is spelled that way.
fn trustee_is_user(trustee: &str, user_sid: &str) -> bool {
    if trustee.eq_ignore_ascii_case(user_sid) {
        return true;
    }
    let rid = user_sid.rsplit('-').next().unwrap_or("");
    (trustee.eq_ignore_ascii_case("LA") && rid == "500")
        || (trustee.eq_ignore_ascii_case("LG") && rid == "501")
}

/// True when the DACL in `sddl` is protected (does not inherit), has no inherited entry and grants
/// access to `user_sid` only. `sddl` is the text of one security descriptor, for example
/// `D:PAI(A;;FA;;;S-1-5-21-1-2-3-1001)`.
pub fn sddl_grants_only(sddl: &str, user_sid: &str) -> bool {
    let Some(dacl_start) = sddl.find("D:") else {
        return false;
    };
    let dacl = &sddl[dacl_start + 2..];
    let flags_end = dacl.find('(').unwrap_or(dacl.len());
    let flags = &dacl[..flags_end];
    if !flags.contains('P') {
        return false;
    }
    let mut grants = 0;
    let mut rest = &dacl[flags_end..];
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            return false;
        };
        let entry = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let fields: Vec<&str> = entry.split(';').collect();
        if fields.len() < 6 {
            return false;
        }
        let (ace_type, ace_flags, trustee) = (fields[0], fields[1], fields[5]);
        if ace_flags.contains("ID") {
            return false;
        }
        if ace_type.starts_with('A') {
            if !trustee_is_user(trustee, user_sid) {
                return false;
            }
            grants += 1;
        }
    }
    grants > 0
}

/// Restrict `path` to the current user and report whether the restriction is verified in place.
/// Never fails the caller: a `false` result means "applied best effort, could not confirm".
pub fn restrict_to_current_user(path: &Path, is_directory: bool) -> bool {
    imp::restrict(path, is_directory)
}

/// Verify, without changing anything, that `path` is restricted to the current user.
pub fn is_restricted_to_current_user(path: &Path) -> bool {
    imp::is_restricted(path)
}

/// Restrict a SQLite database file and its `-wal`/`-shm` sidecar files to the current user, and
/// on Windows also the containing directory so files SQLite creates later inherit the same
/// restriction (closing the window between a sidecar being created and the next hardening pass).
/// Unix copies the database file's mode to the sidecars it creates, so hardening the main file
/// covers them. Best effort like [`restrict_to_current_user`]: `false` means "applied, could not
/// confirm" (for example on a file system without ACLs), never an error.
pub fn harden_sqlite_files(path: &Path) -> bool {
    let mut verified = restrict_to_current_user(path, false);
    for suffix in ["-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let sidecar = PathBuf::from(name);
        if sidecar.exists() {
            verified &= restrict_to_current_user(&sidecar, false);
        }
    }
    #[cfg(windows)]
    if let Some(dir) = path.parent() {
        verified &= restrict_to_current_user(dir, true);
    }
    if !verified {
        eprintln!(
            "[delta-core] warning: could not confirm owner-only access for {}",
            path.display()
        );
    }
    verified
}

#[cfg(unix)]
mod imp {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    pub fn restrict(path: &Path, is_directory: bool) -> bool {
        let mode = if is_directory { 0o700 } else { 0o600 };
        if std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).is_err() {
            return false;
        }
        mode_is(path, mode)
    }

    pub fn is_restricted(path: &Path) -> bool {
        let Ok(metadata) = std::fs::metadata(path) else {
            return false;
        };
        let mode = if metadata.is_dir() { 0o700 } else { 0o600 };
        mode_is(path, mode)
    }

    fn mode_is(path: &Path, expected: u32) -> bool {
        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o777 == expected)
            .unwrap_or(false)
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::OnceLock;

    use super::sddl_grants_only;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The tools are taken from System32 and not from PATH: a PATH that starts with Git or MSYS
    /// tools would otherwise run their `whoami`, which does not understand these options.
    fn system_tool(name: &str) -> std::path::PathBuf {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        std::path::PathBuf::from(root)
            .join("System32")
            .join(format!("{name}.exe"))
    }

    fn quiet(program: &str) -> Command {
        let mut command = Command::new(system_tool(program));
        command
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null());
        command
    }

    /// SID of the current user, read from `whoami /user` (the same on every system language).
    fn current_user_sid() -> Option<&'static str> {
        static SID: OnceLock<Option<String>> = OnceLock::new();
        SID.get_or_init(|| {
            let output = quiet("whoami")
                .args(["/user", "/fo", "csv", "/nh"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            let start = text.find("S-1-")?;
            let sid: String = text[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == 'S' || *c == '-')
                .collect();
            (sid.matches('-').count() >= 3).then_some(sid)
        })
        .as_deref()
    }

    pub fn restrict(path: &Path, is_directory: bool) -> bool {
        let Some(sid) = current_user_sid() else {
            return false;
        };
        let grant = if is_directory {
            format!("*{sid}:(OI)(CI)F")
        } else {
            format!("*{sid}:F")
        };
        let applied = quiet("icacls")
            .arg(path)
            .args(["/inheritance:r", "/grant:r", &grant, "/q"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        applied && is_restricted(path)
    }

    pub fn is_restricted(path: &Path) -> bool {
        let Some(sid) = current_user_sid() else {
            return false;
        };
        let saved = std::env::temp_dir().join(format!("delta-acl-{}.txt", uuid::Uuid::new_v4()));
        let result = quiet("icacls")
            .arg(path)
            .arg("/save")
            .arg(&saved)
            .arg("/q")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
            .then(|| std::fs::read(&saved).ok())
            .flatten();
        let _ = std::fs::remove_file(&saved);
        let Some(bytes) = result else {
            return false;
        };
        let text = decode(&bytes);
        // Line 1 is the path (which can contain "D:"), so only later lines are descriptors.
        text.lines()
            .skip(1)
            .any(|line| line.contains("D:") && sddl_grants_only(line, sid))
    }

    /// `icacls /save` writes UTF-16 little endian, with or without a byte order mark.
    fn decode(bytes: &[u8]) -> String {
        let body = bytes.strip_prefix(&[0xFF, 0xFE]);
        let is_utf16 = body.is_some() || bytes.get(1) == Some(&0);
        if is_utf16 {
            let units: Vec<u16> = body
                .unwrap_or(bytes)
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(bytes).to_string()
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    use std::path::Path;

    pub fn restrict(_path: &Path, _is_directory: bool) -> bool {
        false
    }

    pub fn is_restricted(_path: &Path) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "S-1-5-21-1111-2222-3333-1001";

    #[test]
    fn a_protected_dacl_for_the_user_only_is_accepted() {
        let sddl = format!("D:P(A;;FA;;;{USER})");
        assert!(sddl_grants_only(&sddl, USER));
        assert!(
            sddl_grants_only(&sddl, &USER.to_lowercase()),
            "SIDs compare without case"
        );
    }

    #[test]
    fn well_known_groups_inherited_entries_and_open_dacls_are_rejected() {
        assert!(!sddl_grants_only(
            &format!("D:P(A;;FA;;;{USER})(A;;FA;;;SY)"),
            USER
        ));
        assert!(!sddl_grants_only(
            &format!("D:P(A;;FA;;;{USER})(A;;FA;;;BA)"),
            USER
        ));
        assert!(!sddl_grants_only(&format!("D:PAI(A;ID;FA;;;{USER})"), USER));
        assert!(
            !sddl_grants_only(&format!("D:(A;;FA;;;{USER})"), USER),
            "not protected"
        );
        assert!(!sddl_grants_only("D:P(A;;FA;;;S-1-5-21-9-9-9-500)", USER));
        assert!(!sddl_grants_only("D:P", USER), "no entries at all");
        assert!(!sddl_grants_only("garbage", USER));
        assert!(!sddl_grants_only("D:P(A;;FA;;;", USER), "truncated");
    }

    #[test]
    fn the_local_administrator_alias_counts_only_for_rid_500() {
        let admin = "S-1-5-21-1111-2222-3333-500";
        assert!(sddl_grants_only("D:PAI(A;;FA;;;LA)", admin));
        assert!(!sddl_grants_only("D:PAI(A;;FA;;;LA)", USER));
        assert!(!sddl_grants_only("D:PAI(A;;FA;;;LA)(A;;FA;;;SY)", admin));
    }

    #[test]
    fn deny_entries_do_not_count_as_grants() {
        assert!(sddl_grants_only(
            &format!("D:P(D;;FA;;;WD)(A;;FA;;;{USER})"),
            USER
        ));
    }

    #[test]
    fn a_restricted_file_verifies_and_a_loose_one_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("secret.json");
        std::fs::write(&file, b"{}").unwrap();
        assert!(restrict_to_current_user(&file, false));
        assert!(is_restricted_to_current_user(&file));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(!is_restricted_to_current_user(&file));
        }
    }

    #[test]
    fn sqlite_hardening_covers_the_database_and_existing_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("run_events.db");
        std::fs::write(&db, b"").unwrap();
        std::fs::write(dir.path().join("run_events.db-wal"), b"").unwrap();
        assert!(harden_sqlite_files(&db));
        assert!(is_restricted_to_current_user(&db));
        assert!(is_restricted_to_current_user(
            &dir.path().join("run_events.db-wal")
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&db).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(dir.path().join("run_events.db-wal"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn sqlite_hardening_skips_missing_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("side-effects.db");
        std::fs::write(&db, b"").unwrap();
        assert!(harden_sqlite_files(&db));
        assert!(is_restricted_to_current_user(&db));
        assert!(!dir.path().join("side-effects.db-wal").exists());
    }
}
