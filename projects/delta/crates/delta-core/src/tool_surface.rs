//! Which optional tools a session offers the model at all.
//!
//! A tool definition costs tokens on every request, so a tool that cannot do anything in a session
//! is left out of it. The test is a fact about the session that holds for the whole conversation
//! (a study record sits in the workspace, a folder is in the library, an earlier conversation
//! exists), never a guess about what the user will ask next: the same facts give the same tools on
//! every turn, so the provider can keep reusing its cache of the request head. Tools that are
//! generally useful, and tools the user switched on or off, are not decided here; the one switch read
//! here is memory, because its tool has nothing to do while memory is off.

use std::path::Path;

/// What is true of a session when it starts a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionFacts {
    /// A study record is in the workspace, or the statistics runtime is installed.
    pub has_analysis_work: bool,
    /// At least one folder is in the local library.
    pub has_library_folder: bool,
    /// The workspace holds another conversation that has messages.
    pub has_other_conversation: bool,
    /// The user has switched memory on (a settings file that cannot be read counts as off).
    pub memory_is_on: bool,
}

/// Whether `tool_name` can do anything in a session with these facts. A tool this module does not
/// name is always useful.
pub fn is_useful(tool_name: &str, facts: SessionFacts) -> bool {
    match tool_name {
        // The study and report tools work on a study record or on an analysis engine's results.
        "study_evaluate" | "report_render" | "report_check" => facts.has_analysis_work,
        // The library tools search the folders the user put in the library: with none, they find nothing.
        "library_search" | "library_index" => facts.has_library_folder,
        // Searching earlier conversations has nothing to search in the first one.
        "conversation_search" => facts.has_other_conversation,
        // Suggesting a memory is for a user who has switched memory on.
        "memory_propose" => facts.memory_is_on,
        _ => true,
    }
}

/// Most folders looked into when searching a workspace for a study record.
const STUDY_SCAN_LIMIT: usize = 200;

/// Whether `study.json` is in the workspace folder or directly inside one of its folders.
pub fn has_study_record(workspace: &Path) -> bool {
    if workspace.join("study.json").is_file() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return false;
    };
    entries
        .flatten()
        .take(STUDY_SCAN_LIMIT)
        .any(|entry| entry.path().is_dir() && entry.path().join("study.json").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 7] = [
        "study_evaluate",
        "report_render",
        "report_check",
        "library_search",
        "library_index",
        "conversation_search",
        "memory_propose",
    ];

    #[test]
    fn a_session_with_nothing_special_offers_none_of_the_optional_tools() {
        for tool in ALL {
            assert!(!is_useful(tool, SessionFacts::default()), "{tool}");
        }
    }

    #[test]
    fn each_fact_brings_back_its_own_tools_and_no_others() {
        let analysis = SessionFacts {
            has_analysis_work: true,
            ..SessionFacts::default()
        };
        let library = SessionFacts {
            has_library_folder: true,
            ..SessionFacts::default()
        };
        let other = SessionFacts {
            has_other_conversation: true,
            ..SessionFacts::default()
        };
        let memory = SessionFacts {
            memory_is_on: true,
            ..SessionFacts::default()
        };
        let offered = |facts| {
            ALL.into_iter()
                .filter(|tool| is_useful(tool, facts))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            offered(analysis),
            ["study_evaluate", "report_render", "report_check"]
        );
        assert_eq!(offered(library), ["library_search", "library_index"]);
        assert_eq!(offered(other), ["conversation_search"]);
        assert_eq!(offered(memory), ["memory_propose"]);
    }

    #[test]
    fn a_tool_that_is_not_named_here_is_always_offered() {
        for tool in [
            "read_file",
            "write_file",
            "run_shell",
            "web_search",
            "ask_user",
            "mcp_server_tool",
        ] {
            assert!(is_useful(tool, SessionFacts::default()), "{tool}");
        }
    }

    #[test]
    fn a_study_record_is_found_in_the_workspace_or_one_folder_down() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!has_study_record(tmp.path()));
        std::fs::create_dir_all(tmp.path().join("round-1/deeper")).unwrap();
        std::fs::write(tmp.path().join("round-1/deeper/study.json"), "{}").unwrap();
        assert!(
            !has_study_record(tmp.path()),
            "two folders down is not looked at"
        );
        std::fs::write(tmp.path().join("round-1/study.json"), "{}").unwrap();
        assert!(has_study_record(tmp.path()));
        let direct = tempfile::tempdir().unwrap();
        std::fs::write(direct.path().join("study.json"), "{}").unwrap();
        assert!(has_study_record(direct.path()));
        assert!(!has_study_record(&direct.path().join("missing")));
    }
}
