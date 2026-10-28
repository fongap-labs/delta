//! The folder trigger of an automation: what is in a watched folder, which files are new, and how
//! the run that follows is told about them (ADR-0090).
//!
//! The watched folder always lies inside the automation's workspace, so an unattended run reads the
//! files it was started for under the same grant as any other file of that workspace. Only files
//! directly in the folder count; nothing here follows folders or links out of the workspace.

use std::path::{Path, PathBuf};

/// A file as it was seen in the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub name: String,
    pub size: u64,
    /// Modification time, seconds since the epoch.
    pub mtime: i64,
}

/// The most files named in the text a run starts from; the run is told how many more there are.
const MAX_NAMED: usize = 20;

/// Resolve `folder` (relative to the workspace, or an absolute path inside it; empty means the
/// workspace itself) to an existing directory inside `workspace`, outside `.delta`. Returns the
/// directory and its path relative to the workspace (`.` for the workspace itself).
pub fn resolve_folder(workspace: &str, folder: &str) -> Result<(PathBuf, String), String> {
    if workspace.trim().is_empty() {
        return Err("a folder trigger needs the automation to have a workspace".to_owned());
    }
    let root = Path::new(workspace)
        .canonicalize()
        .map_err(|error| format!("the workspace is unavailable: {error}"))?;
    let requested = folder.trim();
    let joined = if requested.is_empty() || requested == "." {
        root.clone()
    } else if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        root.join(requested)
    };
    let resolved = joined
        .canonicalize()
        .map_err(|_| format!("the folder {requested:?} does not exist"))?;
    if !resolved.is_dir() {
        return Err(format!("{requested:?} is not a folder"));
    }
    if !resolved.starts_with(&root) {
        return Err("the watched folder must be inside the automation's workspace".to_owned());
    }
    if resolved.starts_with(root.join(".delta")) {
        return Err("the watched folder may not be inside .delta".to_owned());
    }
    let relative = resolved
        .strip_prefix(&root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    Ok((
        resolved,
        if relative.is_empty() {
            ".".to_owned()
        } else {
            relative
        },
    ))
}

/// Whether `name` matches one of the comma-separated patterns, ignoring case. `*` is any run of
/// characters and `?` is one character. No pattern, or an empty one, matches every name.
pub fn matches_pattern(pattern: &str, name: &str) -> bool {
    let patterns: Vec<&str> = pattern
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if patterns.is_empty() {
        return true;
    }
    let name: Vec<char> = name.to_lowercase().chars().collect();
    patterns.iter().any(|part| {
        let part: Vec<char> = part.to_lowercase().chars().collect();
        glob(&part, &name)
    })
}

fn glob(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|skip| glob(rest, &name[skip..])),
        Some((&c, rest)) => match name.split_first() {
            Some((&n, tail)) => (c == '?' || c == n) && glob(rest, tail),
            None => false,
        },
    }
}

/// A name a file has while it is still being written or is a program's own scratch file.
fn is_transient(name: &str) -> bool {
    let lower = name.to_lowercase();
    name.starts_with('.')
        || name.starts_with("~$")
        || [
            ".tmp",
            ".temp",
            ".part",
            ".partial",
            ".crdownload",
            ".download",
            ".swp",
        ]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

/// The files directly in `folder` that a trigger may fire for: plain files, not hidden or
/// scratch files, matching `pattern`. Sorted by name.
pub fn scan(folder: &Path, pattern: Option<&str>) -> Vec<Observation> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut seen: Vec<Observation> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            // `symlink_metadata`: a link is not followed, so one cannot lead out of the workspace.
            let metadata = std::fs::symlink_metadata(entry.path()).ok()?;
            if !metadata.is_file() || is_transient(&name) {
                return None;
            }
            if !pattern.is_none_or(|pattern| matches_pattern(pattern, &name)) {
                return None;
            }
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_secs() as i64);
            Some(Observation {
                name,
                size: metadata.len(),
                mtime,
            })
        })
        .collect();
    seen.sort_by(|a, b| a.name.cmp(&b.name));
    seen
}

/// What is added to an automation's instructions when files arrive, naming each by its path in the
/// workspace.
pub fn describe_arrival(folder_relative: &str, files: &[Observation]) -> String {
    let path_of = |name: &str| {
        if folder_relative == "." {
            name.to_owned()
        } else {
            format!("{folder_relative}/{name}")
        }
    };
    let mut text = format!(
        "This run was started because {} new file{} arrived in {}:\n",
        files.len(),
        if files.len() == 1 { "" } else { "s" },
        if folder_relative == "." {
            "the workspace folder"
        } else {
            folder_relative
        },
    );
    for file in files.iter().take(MAX_NAMED) {
        text.push_str(&format!(
            "- {} ({} bytes)\n",
            path_of(&file.name),
            file.size
        ));
    }
    if files.len() > MAX_NAMED {
        text.push_str(&format!("- and {} more\n", files.len() - MAX_NAMED));
    }
    text.push_str("Work on these files; do not touch other files in that folder unless the instructions say to.");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_must_exist_inside_the_workspace_and_outside_delta() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("ws");
        std::fs::create_dir_all(ws.join("inbox/sub")).unwrap();
        std::fs::create_dir_all(ws.join(".delta/versions")).unwrap();
        std::fs::create_dir_all(tmp.path().join("outside")).unwrap();
        std::fs::write(ws.join("file.txt"), "x").unwrap();
        let root = ws.canonicalize().unwrap();
        let workspace = ws.to_string_lossy().to_string();

        assert_eq!(resolve_folder(&workspace, "inbox").unwrap().1, "inbox");
        assert_eq!(
            resolve_folder(&workspace, "inbox/sub").unwrap().1,
            "inbox/sub"
        );
        assert_eq!(resolve_folder(&workspace, "").unwrap().1, ".");
        assert_eq!(resolve_folder(&workspace, ".").unwrap().0, root);
        // An absolute path inside the workspace is read as the same folder.
        assert_eq!(
            resolve_folder(&workspace, &root.join("inbox").to_string_lossy())
                .unwrap()
                .1,
            "inbox"
        );

        for refused in [
            "../outside",
            "missing",
            "file.txt",
            ".delta",
            ".delta/versions",
        ] {
            assert!(resolve_folder(&workspace, refused).is_err(), "{refused}");
        }
        assert!(resolve_folder(&workspace, &tmp.path().join("outside").to_string_lossy()).is_err());
        assert!(resolve_folder("", "inbox").is_err());
        assert!(resolve_folder(&tmp.path().join("nope").to_string_lossy(), "").is_err());
    }

    #[test]
    fn patterns_match_names_without_regard_to_case() {
        assert!(matches_pattern("", "anything.bin"));
        assert!(matches_pattern("  ,", "anything.bin"));
        assert!(matches_pattern("*.pdf", "Report.PDF"));
        assert!(!matches_pattern("*.pdf", "report.pdf.txt"));
        assert!(matches_pattern("*.pdf, *.xlsx", "q3.xlsx"));
        assert!(matches_pattern("invoice-??.csv", "invoice-07.csv"));
        assert!(!matches_pattern("invoice-??.csv", "invoice-7.csv"));
        assert!(matches_pattern("*发票*", "2026年发票.pdf"));
        assert!(matches_pattern("*", ""));
    }

    #[test]
    fn a_scan_lists_plain_files_and_skips_hidden_scratch_and_folders() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "b.pdf",
            "a.pdf",
            "note.txt",
            ".hidden.pdf",
            "~$open.xlsx",
            "half.pdf.part",
            "dl.crdownload",
        ] {
            std::fs::write(tmp.path().join(name), "12345").unwrap();
        }
        std::fs::create_dir(tmp.path().join("folder.pdf")).unwrap();
        let all = scan(tmp.path(), None);
        assert_eq!(
            all.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["a.pdf", "b.pdf", "note.txt"]
        );
        assert_eq!(all[0].size, 5);
        let pdfs = scan(tmp.path(), Some("*.pdf"));
        assert_eq!(pdfs.len(), 2);
        assert!(scan(&tmp.path().join("missing"), None).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_not_followed_out_of_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("secret.txt");
        std::fs::write(&outside, "secret").unwrap();
        let folder = tmp.path().join("inbox");
        std::fs::create_dir(&folder).unwrap();
        std::os::unix::fs::symlink(&outside, folder.join("link.txt")).unwrap();
        assert!(scan(&folder, None).is_empty());
    }

    #[test]
    fn the_run_is_told_which_files_arrived_by_their_path_in_the_workspace() {
        let one = vec![Observation {
            name: "a.pdf".into(),
            size: 10,
            mtime: 0,
        }];
        let text = describe_arrival("inbox", &one);
        assert!(text.contains("1 new file arrived in inbox:"), "{text}");
        assert!(text.contains("- inbox/a.pdf (10 bytes)"), "{text}");
        let root = describe_arrival(".", &one);
        assert!(
            root.contains("the workspace folder") && root.contains("- a.pdf (10 bytes)"),
            "{root}"
        );
        let many: Vec<Observation> = (0..25)
            .map(|n| Observation {
                name: format!("f{n:02}.txt"),
                size: 1,
                mtime: 0,
            })
            .collect();
        let text = describe_arrival("in", &many);
        assert!(text.contains("25 new files"), "{text}");
        assert!(text.contains("- and 5 more"), "{text}");
        assert!(!text.contains("f20.txt"), "{text}");
    }
}
