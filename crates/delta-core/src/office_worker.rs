//! Office tools that build finished DOCX, PPTX and XLSX files (ADR-0085).
//!
//! The three tools run the Office Worker (`packages/office_worker/worker.py`) in one fixed,
//! configured Python runtime. They are registered only when `DELTA_OFFICE_PYTHON` and
//! `DELTA_OFFICE_ROOT` both point at existing files. Otherwise nothing is registered and the model
//! is offered no office tool it cannot run. The worker writes a candidate into the staging
//! directory the host allocates; the host checks the path, hashes the file and registers it.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use crate::capability::{CapabilityGrants, CapabilityRegistration, WorkerProcessRunner};
use crate::runtime_pack;
use crate::skills::SideEffectClass;

pub const PYTHON_ENV: &str = "DELTA_OFFICE_PYTHON";
pub const ROOT_ENV: &str = "DELTA_OFFICE_ROOT";
/// Set to `1` to run an unpacked checkout that has no manifest. Never set in a packaged install.
pub const DEVELOPMENT_ENV: &str = "DELTA_OFFICE_DEVELOPMENT";

/// The format the packaged runtime's manifest declares (ADR-0085, `packages/office_worker`).
const FORMAT: &str = "delta.office-runtime";

/// The worker's path under the package root. The worker adds the root to `sys.path` itself.
const WORKER_PARTS: [&str; 3] = ["packages", "office_worker", "worker.py"];

const CONTENT_DESCRIPTION: &str = "The document as structured content, not layout. \
    docx: {title, subtitle?, header?, footer?, blocks: [{type: heading (level 1-3, text) | \
    paragraph (text) | bullets (items) | numbered (items) | table (caption?, columns, rows) | \
    page_break}]}. \
    pptx: {title, slides: [{layout: title (title, subtitle?) | bullets (title, items) | \
    table (title, columns, rows), notes?}]}. \
    xlsx: {title, sheets: [{name, columns, rows, number_formats? (column name to format)}]}. \
    A table cell is text, a number, true/false or null. Text that begins with = in an xlsx cell is a formula.";

/// The fixed Office runtime: an interpreter and the worker script it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficeRuntime {
    python: PathBuf,
    worker: PathBuf,
}

impl OfficeRuntime {
    /// Reads the runtime from the process environment.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> Option<Self> {
        let python = PathBuf::from(lookup(PYTHON_ENV)?);
        let root = PathBuf::from(lookup(ROOT_ENV)?);
        let is_development = lookup(DEVELOPMENT_ENV).is_some_and(|value| value == "1");
        if !may_run(&root, is_development) {
            return None;
        }
        let worker = WORKER_PARTS.iter().fold(root, |path, part| path.join(part));
        (python.is_file() && worker.is_file()).then_some(Self { python, worker })
    }
}

/// Whether the runtime at `root` may run. A root with a manifest must match it exactly, whatever
/// the development switch says. A root without one is refused, unless the switch admits an
/// unpacked checkout; that admission is logged, so it is never silent.
fn may_run(root: &Path, is_development: bool) -> bool {
    if root.join(runtime_pack::MANIFEST_NAME).exists() {
        let problems = runtime_pack::pack_problems(root, FORMAT);
        if !problems.is_empty() {
            eprintln!("delta: office runtime refused: {}", problems.join("; "));
            return false;
        }
        return true;
    }
    if is_development {
        eprintln!("delta: office runtime runs an unpacked checkout without a manifest ({DEVELOPMENT_ENV}=1)");
        return true;
    }
    eprintln!("delta: office runtime refused: manifest.json is missing; set {DEVELOPMENT_ENV}=1 for an unpacked checkout");
    false
}

/// One Office format the worker can build.
struct Format {
    extension: &'static str,
    capability_id: &'static str,
    tool_name: &'static str,
    description: &'static str,
}

const FORMATS: [Format; 3] = [
    Format {
        extension: "docx",
        capability_id: "office.docx.create",
        tool_name: "office_create_docx",
        description: "Create a finished Word document (.docx) with a house style: headings, \
            paragraphs, lists, tables with a repeating header row, page breaks, a header and a footer \
            with page numbers. The file is reopened and checked before it is registered.",
    },
    Format {
        extension: "pptx",
        capability_id: "office.pptx.create",
        tool_name: "office_create_pptx",
        description: "Create a finished PowerPoint deck (.pptx), 16:9, with title, bullet and table \
            slides and optional speaker notes. The file is reopened and checked before it is registered.",
    },
    Format {
        extension: "xlsx",
        capability_id: "office.xlsx.create",
        tool_name: "office_create_xlsx",
        description: "Create a finished Excel workbook (.xlsx) with styled, frozen header rows, \
            autofilters, fitted columns, numeric cells and formulas. Formulas that reach outside the \
            workbook are refused. The file is reopened and checked before it is registered.",
    },
];

fn registration(runtime: &OfficeRuntime, format: &Format) -> CapabilityRegistration {
    let environment = BTreeMap::from([("PYTHONIOENCODING".to_string(), "utf-8".to_string())]);
    CapabilityRegistration {
        capability_id: format.capability_id.to_string(),
        tool_name: format.tool_name.to_string(),
        side_effect: SideEffectClass::LocalWrite,
        description: format.description.to_string(),
        parameters: json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": format!("Where to create the file, relative to the workspace, ending in .{}.", format.extension),
                },
                "content": {"type": "object", "description": CONTENT_DESCRIPTION},
            },
        }),
        metadata: json!({
            "risk_level": "medium",
            "requires_approval": true,
            "category": "filesystem",
            "engine": "python",
            "capabilities": [format.capability_id],
        }),
        // The worker stages its candidate; the host promotes it into the workspace, so the worker
        // itself needs no grant beyond running.
        grants: CapabilityGrants::default(),
        workspace_write: true,
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
    }
}

/// The Office registrations. Empty unless a runtime is configured.
pub fn registrations(runtime: Option<&OfficeRuntime>) -> Vec<CapabilityRegistration> {
    let Some(runtime) = runtime else {
        return Vec::new();
    };
    FORMATS
        .iter()
        .map(|format| registration(runtime, format))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKER_RELATIVE: &str = "packages/office_worker/worker.py";

    /// An unpacked checkout: an interpreter path and a root holding the worker script.
    fn checkout(directory: &Path) -> (PathBuf, PathBuf) {
        let python = directory.join("python");
        let root = directory.join("root");
        let worker = root.join(WORKER_RELATIVE);
        std::fs::create_dir_all(worker.parent().unwrap()).unwrap();
        std::fs::write(&python, "").unwrap();
        std::fs::write(&worker, "print('worker')\n").unwrap();
        (python, root)
    }

    /// Writes the manifest a packaged runtime carries, listing the worker with its hash.
    fn write_manifest(root: &Path) {
        let bytes = std::fs::read(root.join(WORKER_RELATIVE)).unwrap();
        let manifest = serde_json::json!({
            "format": FORMAT,
            "version": 1,
            "files": [{
                "path": WORKER_RELATIVE,
                "sha256": runtime_pack::sha256_hex(&bytes),
                "size": bytes.len(),
            }],
        });
        std::fs::write(root.join(runtime_pack::MANIFEST_NAME), manifest.to_string()).unwrap();
    }

    fn lookup_for(
        python: &Path,
        root: &Path,
        is_development: bool,
    ) -> impl Fn(&str) -> Option<OsString> {
        let python = python.to_path_buf().into_os_string();
        let root = root.to_path_buf().into_os_string();
        move |name| match name {
            PYTHON_ENV => Some(python.clone()),
            ROOT_ENV => Some(root.clone()),
            DEVELOPMENT_ENV if is_development => Some(OsString::from("1")),
            _ => None,
        }
    }

    /// A packaged runtime: a checkout with its manifest written beside the worker.
    fn packaged_runtime_in(directory: &Path) -> Option<OfficeRuntime> {
        let (python, root) = checkout(directory);
        write_manifest(&root);
        OfficeRuntime::from_lookup(lookup_for(&python, &root, false))
    }

    #[test]
    fn nothing_is_registered_without_a_configured_runtime() {
        assert!(registrations(None).is_empty());
    }

    #[test]
    fn an_unset_or_missing_runtime_is_not_found() {
        assert!(OfficeRuntime::from_lookup(|_| None).is_none());

        let temp = tempfile::tempdir().unwrap();
        let (python, _) = checkout(temp.path());
        let missing = OfficeRuntime::from_lookup(lookup_for(
            &python,
            &temp.path().join("no-such-root"),
            false,
        ));
        assert!(missing.is_none());
    }

    #[test]
    fn a_packaged_runtime_whose_manifest_matches_is_accepted() {
        let temp = tempfile::tempdir().unwrap();

        assert!(packaged_runtime_in(temp.path()).is_some());
    }

    #[test]
    fn a_root_without_a_manifest_is_refused_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = checkout(temp.path());

        assert!(OfficeRuntime::from_lookup(lookup_for(&python, &root, false)).is_none());
    }

    #[test]
    fn the_development_switch_admits_an_unpacked_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = checkout(temp.path());

        assert!(OfficeRuntime::from_lookup(lookup_for(&python, &root, true)).is_some());
    }

    #[test]
    fn a_tampered_packaged_worker_is_refused_even_with_the_development_switch() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = checkout(temp.path());
        write_manifest(&root);
        std::fs::write(root.join(WORKER_RELATIVE), "import os\n").unwrap();

        assert!(OfficeRuntime::from_lookup(lookup_for(&python, &root, true)).is_none());
        assert!(OfficeRuntime::from_lookup(lookup_for(&python, &root, false)).is_none());
    }

    #[test]
    fn a_manifest_of_another_pack_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let (python, root) = checkout(temp.path());
        write_manifest(&root);
        let other = std::fs::read_to_string(root.join(runtime_pack::MANIFEST_NAME))
            .unwrap()
            .replace(FORMAT, "delta.explore-runtime");
        std::fs::write(root.join(runtime_pack::MANIFEST_NAME), other).unwrap();

        assert!(OfficeRuntime::from_lookup(lookup_for(&python, &root, true)).is_none());
    }

    #[test]
    fn three_formats_are_registered_as_staged_writes_that_need_approval() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = packaged_runtime_in(temp.path()).expect("configured runtime");

        let registered = registrations(Some(&runtime));

        let ids: Vec<&str> = registered
            .iter()
            .map(|r| r.capability_id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "office.docx.create",
                "office.pptx.create",
                "office.xlsx.create"
            ]
        );
        for registration in &registered {
            assert!(registration.workspace_write);
            assert_eq!(
                registration.metadata["requires_approval"],
                serde_json::json!(true)
            );
            assert_eq!(
                registration.metadata["risk_level"],
                serde_json::json!("medium")
            );
            assert_eq!(
                registration.parameters["required"],
                serde_json::json!(["path", "content"])
            );
        }
    }

    #[test]
    fn tool_names_are_distinct_from_the_native_document_tools() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = packaged_runtime_in(temp.path()).expect("configured runtime");

        let names: Vec<String> = registrations(Some(&runtime))
            .into_iter()
            .map(|r| r.tool_name)
            .collect();

        assert_eq!(
            names,
            [
                "office_create_docx",
                "office_create_pptx",
                "office_create_xlsx"
            ]
        );
        assert!(!names.iter().any(|name| name == "create_document"));
    }

    /// The worker run through the host, with a real Python runtime. It needs an interpreter that
    /// has the Office libraries installed, so it runs only when DELTA_OFFICE_PYTHON names one. The
    /// repository checkout has no manifest, so it runs under the development switch.
    #[test]
    fn a_docx_call_runs_the_worker_through_the_host() {
        let Some(python) = std::env::var_os(PYTHON_ENV).map(PathBuf::from) else {
            eprintln!("skipped: {PYTHON_ENV} is not set, so the office worker cannot run here");
            return;
        };
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("packages").join("office_worker").is_dir())
            .expect("repository root")
            .to_path_buf();
        let runtime = OfficeRuntime::from_lookup(|name| match name {
            PYTHON_ENV => Some(python.clone().into_os_string()),
            ROOT_ENV => Some(repository.clone().into_os_string()),
            DEVELOPMENT_ENV => Some(OsString::from("1")),
            _ => None,
        })
        .expect("the unpacked checkout runs under the development switch");
        let workspace = tempfile::tempdir().unwrap();
        let host = crate::capability::CapabilityHost::new();
        for registration in registrations(Some(&runtime)) {
            host.register(registration).unwrap();
        }
        let context = crate::runtime::ToolExecutionContext {
            session_id: "session-office".to_string(),
            run_id: "run-office".to_string(),
            workspace: Some(workspace.path().to_string_lossy().to_string()),
            timeout: std::time::Duration::from_secs(60),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: BTreeMap::new(),
            proxy: None,
        };
        let call = crate::runtime::ToolCall {
            id: "call-office-docx".to_string(),
            name: "office_create_docx".to_string(),
            arguments: serde_json::json!({
                "path": "reports/q3.docx",
                "content": {
                    "title": "Q3 review",
                    "blocks": [
                        {"type": "heading", "level": 1, "text": "Results"},
                        {"type": "table", "columns": ["Region", "Revenue"], "rows": [["North", 1200.5]]}
                    ]
                }
            }),
        };

        let result = crate::runtime::ToolExecutor::execute(&host, &call, &context);

        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.output["format"], "docx");
        assert_eq!(result.output["verification"]["status"], "passed");
        assert_eq!(result.output["counts"]["tables"], 1);
    }
}
