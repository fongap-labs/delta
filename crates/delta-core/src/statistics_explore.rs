//! Explore statistics that run without a model turn (ADR-0078, amended by ADR-0080).
//!
//! The four tools run the Explore Pack worker (`packages/explore_pack/worker.py`) in one fixed,
//! configured Python runtime. They are registered only when `DELTA_EXPLORE_PYTHON` and
//! `DELTA_EXPLORE_ROOT` both point at existing files. Otherwise nothing is registered and the
//! statistics page stays empty: the runtime fails closed and never falls back to another engine.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::capability::{CapabilityGrants, CapabilityRegistration, WorkerProcessRunner};
use crate::runtime_pack::{self, sha256_hex, MANIFEST_NAME};
use crate::skills::SideEffectClass;

pub const PYTHON_ENV: &str = "DELTA_EXPLORE_PYTHON";
pub const ROOT_ENV: &str = "DELTA_EXPLORE_ROOT";

/// The worker's path under the pack root. The worker adds the root to `sys.path` itself.
const WORKER_PARTS: [&str; 3] = ["packages", "explore_pack", "worker.py"];

/// The format a packaged Explore runtime's manifest declares (ADR-0080, `runtime_pack.py`).
const FORMAT: &str = "delta.explore-runtime";

/// The problems that stop a packaged Explore runtime from being used. A root without a manifest
/// is a development checkout and is not checked. A root with a manifest must match it exactly.
fn pack_problems(root: &Path) -> Vec<String> {
    if !root.join(MANIFEST_NAME).exists() {
        return Vec::new();
    }
    runtime_pack::pack_problems(root, FORMAT)
}

/// The fixed Explore runtime: an interpreter and the worker script it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExploreRuntime {
    python: PathBuf,
    worker: PathBuf,
}

impl ExploreRuntime {
    /// Reads the runtime from the process environment.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> Option<Self> {
        let python = PathBuf::from(lookup(PYTHON_ENV)?);
        let root = PathBuf::from(lookup(ROOT_ENV)?);
        if !pack_problems(&root).is_empty() {
            return None;
        }
        let worker = WORKER_PARTS.iter().fold(root, |path, part| path.join(part));
        (python.is_file() && worker.is_file()).then_some(Self { python, worker })
    }
}

/// One Explore method exposed as a statistics tool.
struct Method {
    name: &'static str,
    description: &'static str,
    properties: Value,
    required: &'static [&'static str],
}

fn methods() -> [Method; 4] {
    let path = json!({"type": "string", "description": "The CSV file, relative to the workspace."});
    let column = json!({"type": "string", "description": "A column name in the CSV header."});
    [
        Method {
            name: "describe",
            description:
                "Count, mean, sample standard deviation, minimum, median and maximum of one \
                column. Exploratory only: the result cannot enter a formal report.",
            properties: json!({"path": path, "column": column}),
            required: &["path", "column"],
        },
        Method {
            name: "compare",
            description: "Welch two-sample t-test between two groups of one column, two-sided. \
                Exploratory only: the result cannot enter a formal report.",
            properties: json!({
                "path": path,
                "group_column": column,
                "value_column": column,
                "group_a": {"type": "string"},
                "group_b": {"type": "string"}
            }),
            required: &["path", "group_column", "value_column", "group_a", "group_b"],
        },
        Method {
            name: "correlation",
            description:
                "Pearson correlation of two columns with a two-sided test of zero correlation. \
                Exploratory only: the result cannot enter a formal report.",
            properties: json!({"path": path, "x_column": column, "y_column": column}),
            required: &["path", "x_column", "y_column"],
        },
        Method {
            name: "regression",
            description:
                "Simple least-squares regression of y on x with a slope test. Exploratory only: \
                the result cannot enter a formal report.",
            properties: json!({"path": path, "x_column": column, "y_column": column}),
            required: &["path", "x_column", "y_column"],
        },
    ]
}

fn registration(runtime: &ExploreRuntime, method: &Method) -> CapabilityRegistration {
    let capability_id = format!("statistics.explore.{}", method.name);
    let environment = BTreeMap::from([("PYTHONIOENCODING".to_string(), "utf-8".to_string())]);
    CapabilityRegistration {
        tool_name: format!("statistics_explore_{}", method.name),
        description: method.description.to_string(),
        parameters: json!({
            "type": "object",
            "required": method.required,
            "properties": method.properties,
        }),
        metadata: json!({
            "risk_level": "low",
            "requires_approval": false,
            "category": "statistics",
            "engine": "python",
            "assurance": "exploratory",
            "capabilities": [capability_id],
        }),
        side_effect: SideEffectClass::Read,
        // The pinned runtime needs exec; nothing else is granted. The workspace is read-only and
        // the host adds it as the read root for every job.
        grants: CapabilityGrants {
            exec: true,
            ..CapabilityGrants::default()
        },
        workspace_write: false,
        runner: Arc::new(
            WorkerProcessRunner::new(
                runtime.python.clone(),
                vec![
                    "-I".to_string(),
                    runtime.worker.to_string_lossy().to_string(),
                ],
            )
            .with_environment(environment),
        ),
        capability_id,
    }
}

/// The Explore statistics registrations. Empty unless a runtime is configured.
pub fn registrations(runtime: Option<&ExploreRuntime>) -> Vec<CapabilityRegistration> {
    let Some(runtime) = runtime else {
        return Vec::new();
    };
    methods()
        .iter()
        .map(|method| registration(runtime, method))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityHost;
    use crate::runtime::{ToolCall, ToolExecutionContext, ToolExecutor};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    fn lookup<'a>(pairs: &'a [(&'a str, OsString)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
        }
    }

    fn fake_runtime(root: &std::path::Path) -> (PathBuf, PathBuf) {
        let python = root.join("python");
        std::fs::write(&python, b"").unwrap();
        let worker = WORKER_PARTS
            .iter()
            .fold(root.to_path_buf(), |path, part| path.join(part));
        std::fs::create_dir_all(worker.parent().unwrap()).unwrap();
        std::fs::write(&worker, b"").unwrap();
        (python, root.to_path_buf())
    }

    #[test]
    fn the_runtime_needs_both_variables_and_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = fake_runtime(temp.path());
        let both = [
            (PYTHON_ENV, python.clone().into_os_string()),
            (ROOT_ENV, root.clone().into_os_string()),
        ];
        assert!(ExploreRuntime::from_lookup(lookup(&both)).is_some());

        let missing_root = [(PYTHON_ENV, python.clone().into_os_string())];
        assert!(ExploreRuntime::from_lookup(lookup(&missing_root)).is_none());

        let missing_file = [
            (PYTHON_ENV, temp.path().join("absent").into_os_string()),
            (ROOT_ENV, root.into_os_string()),
        ];
        assert!(ExploreRuntime::from_lookup(lookup(&missing_file)).is_none());
    }

    #[test]
    fn nothing_is_registered_without_a_runtime() {
        assert!(registrations(None).is_empty());
    }

    #[test]
    fn each_tool_is_local_statistics_and_reads_only_the_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = fake_runtime(temp.path());
        let runtime = ExploreRuntime {
            python,
            worker: WORKER_PARTS.iter().fold(root, |path, part| path.join(part)),
        };
        let found = registrations(Some(&runtime));
        assert_eq!(found.len(), 4);
        for registration in &found {
            assert!(registration
                .capability_id
                .starts_with("statistics.explore."));
            assert!(
                registration.is_local_statistics(),
                "{}",
                registration.tool_name
            );
            assert!(registration.grants.write_roots.is_empty());
            assert!(registration.grants.network.is_empty());
            assert!(registration.grants.secrets.is_empty());
            assert!(!registration.workspace_write);
            assert!(matches!(registration.side_effect, SideEffectClass::Read));
            assert_eq!(registration.parameters["required"][0], "path");
        }
    }

    #[test]
    fn exec_is_refused_for_any_other_statistics_tool() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = fake_runtime(temp.path());
        let runtime = ExploreRuntime {
            python,
            worker: WORKER_PARTS.iter().fold(root, |path, part| path.join(part)),
        };
        let mut other = registrations(Some(&runtime)).remove(0);
        other.capability_id = "statistics.shell.run".to_string();
        assert!(other.grants.exec);
        assert!(!other.is_local_statistics());
    }

    fn write_manifest(root: &Path, files: &[(&str, &[u8])]) {
        let entries: Vec<Value> = files
            .iter()
            .map(|(path, bytes)| {
                json!({"path": path, "sha256": sha256_hex(bytes), "size": bytes.len()})
            })
            .collect();
        let manifest = json!({"format": "delta.explore-runtime", "version": 1, "files": entries});
        std::fs::write(root.join(MANIFEST_NAME), manifest.to_string()).unwrap();
    }

    #[test]
    fn a_root_without_a_manifest_is_a_development_checkout() {
        let temp = tempfile::tempdir().unwrap();
        assert!(pack_problems(temp.path()).is_empty());
    }

    #[test]
    fn a_pack_that_matches_its_manifest_is_accepted() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("packages")).unwrap();
        std::fs::write(temp.path().join("packages/a.py"), b"x = 1\n").unwrap();
        write_manifest(temp.path(), &[("packages/a.py", b"x = 1\n")]);
        assert!(pack_problems(temp.path()).is_empty());
    }

    #[test]
    fn a_changed_file_fails_the_manifest_check() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("packages")).unwrap();
        std::fs::write(temp.path().join("packages/a.py"), b"x = 2\n").unwrap();
        write_manifest(temp.path(), &[("packages/a.py", b"x = 1\n")]);
        assert_eq!(pack_problems(temp.path()), ["hash mismatch: packages/a.py"]);
    }

    #[test]
    fn a_missing_file_fails_the_manifest_check() {
        let temp = tempfile::tempdir().unwrap();
        write_manifest(temp.path(), &[("packages/a.py", b"x = 1\n")]);
        assert_eq!(pack_problems(temp.path()), ["missing file: packages/a.py"]);
    }

    #[test]
    fn a_path_that_escapes_the_pack_fails_the_manifest_check() {
        let temp = tempfile::tempdir().unwrap();
        write_manifest(temp.path(), &[("../outside.py", b"x = 1\n")]);
        assert_eq!(
            pack_problems(temp.path()),
            ["path escapes the pack: ../outside.py"]
        );
    }

    #[test]
    fn a_tampered_pack_is_not_registered() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = fake_runtime(temp.path());
        let worker = "packages/explore_pack/worker.py";
        write_manifest(&root, &[(worker, &b""[..])]);
        let pairs = [
            (PYTHON_ENV, python.clone().into_os_string()),
            (ROOT_ENV, root.clone().into_os_string()),
        ];
        assert!(ExploreRuntime::from_lookup(lookup(&pairs)).is_some());

        std::fs::write(root.join(worker), b"tampered").unwrap();

        assert!(ExploreRuntime::from_lookup(lookup(&pairs)).is_none());
    }

    fn python3() -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("python3"))
            .find(|candidate| candidate.is_file())
    }

    #[test]
    fn a_describe_call_runs_the_worker_through_the_host() {
        let Some(python) = python3() else {
            eprintln!("skipped: python3 is not on PATH, so the worker cannot run here");
            return;
        };
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("packages").join("explore_pack").is_dir())
            .expect("repository root")
            .to_path_buf();
        let runtime = ExploreRuntime {
            python,
            worker: WORKER_PARTS
                .iter()
                .fold(repository, |path, part| path.join(part)),
        };
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("data.csv"),
            "x,y\n1,2.1\n2,3.9\n3,6.2\n4,8.1\n",
        )
        .unwrap();

        let host = CapabilityHost::new();
        for registration in registrations(Some(&runtime)) {
            host.register(registration).unwrap();
        }
        let context = ToolExecutionContext {
            session_id: "session-stats".to_string(),
            run_id: "run-stats".to_string(),
            workspace: Some(workspace.path().to_string_lossy().to_string()),
            timeout: Duration::from_secs(30),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::new(),
            proxy: None,
        };
        let call = ToolCall {
            id: "call-describe".to_string(),
            name: "statistics_explore_describe".to_string(),
            arguments: json!({"path": "data.csv", "column": "y"}),
        };

        let result = host.execute(&call, &context);

        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.output["assurance"], "exploratory");
        assert_eq!(result.output["values"]["n"]["value"], 4.0);
    }
}
