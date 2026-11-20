//! Capability catalog — stable "id → capability" layer a persona references.
//!
//! A *capability* bundles a group of tool registrations behind a stable
//! id, plus what session context it needs (requires) and the risk classes
//! it can produce (risk). `expand(ids, context)` turns a persona's
//! capability list into concrete registrations, skipping capabilities
//! whose context prerequisites aren't met (e.g. no shell without executor).
//!
//! The catalog is **platform-owned and closed**: third parties get breadth
//! from us adding vetted capabilities here and from MCP, never by adding
//! entries themselves.
//!
//! Builders reuse the module-level registration functions rather than
//! duplicating them, so the model-visible schema and execution runner stay
//! the single source of truth.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};
use crate::runtime::ToolExecutor;
use crate::skills::SideEffectClass;
use serde_json::{json, Value};

/// Context prerequisites a capability may require.
#[derive(Default, Clone)]
pub struct CapabilityContext {
    pub workspace: Option<String>,
    pub executor: Option<Arc<dyn ToolExecutor>>,
    pub todo: Option<Arc<dyn ToolExecutor>>,
    pub state_dir: Option<PathBuf>,
    pub library: Option<std::sync::Arc<crate::retrieval::Library>>,
    pub skill_store: Option<std::sync::Arc<crate::skills::SkillStore>>,
    pub sandbox: Option<()>, // placeholder for future sandbox support
}

/// Risk classes a capability may produce (for install consent screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskClass {
    Read,
    WriteLocal,
    Exec,
    Network,
    Secrets,
}

/// A capability bundles tool registrations behind a stable id.
pub struct Capability {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub build: fn(&CapabilityContext) -> Vec<CapabilityRegistration>,
    pub requires: &'static [&'static str],
    pub risk: &'static [RiskClass],
}

impl Capability {
    /// Whether this capability's context prerequisites are satisfied.
    pub fn available(&self, context: &CapabilityContext) -> bool {
        for req in self.requires {
            match *req {
                "workspace" => {
                    if context.workspace.is_none() {
                        return false;
                    }
                }
                "executor" => {
                    if context.executor.is_none() {
                        return false;
                    }
                }
                "todo" => {
                    if context.todo.is_none() {
                        return false;
                    }
                }
                "state_dir" => {
                    if context.state_dir.is_none() {
                        return false;
                    }
                }
                "library" => {
                    if context.library.is_none() {
                        return false;
                    }
                }
                "skills" => {
                    if context.skill_store.is_none() {
                        return false;
                    }
                }
                "sandbox" => {
                    if context.sandbox.is_none() {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

/// Build file read/write registrations.
fn build_files(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![
        super::host::native_read_registration(),
        super::host::native_write_registration(),
    ]
}

/// Shell registration — the registration is stateless; the executor is needed at
/// execution time, not at registration time.
fn build_shell(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![crate::capability::shell::shell_registration()]
}

/// Todo registration — stateless at registration time.
fn build_todo(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![crate::todo::write_registration()]
}

/// Web search/fetch registrations.
fn build_web(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![
        crate::web::fetch_registration(),
        crate::web::search_registration(),
    ]
}

/// Scholar registrations.
fn build_scholar(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    crate::web::scholar::registrations()
}

/// Document registrations.
fn build_documents(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![
        crate::document::read_registration(),
        crate::document::create_registration(),
        crate::document::edit_registration(),
    ]
}

/// Document table registrations.
fn build_document_tables(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    crate::document::table::registrations()
}

/// Study/evaluation registration.
fn build_study(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![crate::study::verify::evaluate_registration()]
}

/// Numeric trace registrations.
fn build_numeric_trace(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    crate::numeric_trace::tools::registrations()
}

/// Interaction registrations.
fn build_interactions(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![
        interaction_registration(
            "request_directory",
            "Request access to an additional directory.",
            json!({
                "type": "object", "required": ["reason"],
                "properties": {"reason": {"type": "string"}, "path": {"type": "string"}, "writable": {"type": "boolean"}}
            }),
        ),
        interaction_registration(
            "ask_user",
            "Ask the user a focused question before continuing.",
            json!({
                "type": "object", "required": ["question"],
                "properties": {"question": {"type": "string"}, "options": {"type": "array"}, "allow_text": {"type": "boolean"}}
            }),
        ),
        interaction_registration(
            "propose_plan",
            "Present a plan and wait for user approval.",
            json!({
                "type": "object", "required": ["plan"], "properties": {"plan": {"type": "string"}}
            }),
        ),
    ]
}

/// MCP placeholder — MCP tools registered via extension manifest.
fn build_mcp(_context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    vec![]
}

/// Conversation search — needs state_dir.
fn build_conversation_search(context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    let state_dir = context
        .state_dir
        .clone()
        .expect("state_dir required for conversation_search");
    vec![crate::conversation_search::tool_registration(state_dir)]
}

/// Library search/index — needs library.
fn build_library(context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    let library = context
        .library
        .clone()
        .expect("library required for library capability");
    crate::retrieval::tools::registrations(library)
}

/// Memory propose — needs state_dir.
fn build_memory(context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    let state_dir = context
        .state_dir
        .clone()
        .expect("state_dir required for memory");
    vec![crate::memory::propose_registration(state_dir)]
}

/// Skill find/read — needs skill_store.
fn build_skills(context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    let store = context
        .skill_store
        .clone()
        .expect("skill_store required for skills");
    let mut registrations = vec![crate::skills::save_registration(store.clone())];
    registrations.extend(crate::skills::lookup_registrations(store));
    registrations
}

/// All capabilities in the catalog.
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        id: "files",
        name: "Files",
        description: "Read & edit files in the workspace (read_file, write_file).",
        build: build_files,
        requires: &["workspace"],
        risk: &[RiskClass::Read, RiskClass::WriteLocal],
    },
    Capability {
        id: "shell",
        name: "Shell",
        description: "Run shell commands in a persistent session (run_shell).",
        build: build_shell,
        requires: &[],
        risk: &[RiskClass::Exec],
    },
    Capability {
        id: "todo",
        name: "Task list",
        description: "Maintain a visible task/progress list (todo_write).",
        build: build_todo,
        requires: &[],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "web",
        name: "Web",
        description: "Fetch pages and search the web (web_fetch, web_search).",
        build: build_web,
        requires: &["workspace"],
        risk: &[RiskClass::Read, RiskClass::Network],
    },
    Capability {
        id: "scholar",
        name: "Scholar",
        description:
            "Search and cite academic literature (scholar_search, scholar_paper, scholar_cite).",
        build: build_scholar,
        requires: &["workspace"],
        risk: &[RiskClass::Read, RiskClass::Network],
    },
    Capability {
        id: "documents",
        name: "Documents",
        description: "Read & write PDF, Word, Excel, PowerPoint.",
        build: build_documents,
        requires: &["workspace"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "document_tables",
        name: "Document Tables",
        description: "Reconcile and check spreadsheet tables.",
        build: build_document_tables,
        requires: &["workspace"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "study",
        name: "Study",
        description: "Multi-round study evaluation (study_evaluate).",
        build: build_study,
        requires: &["workspace"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "numeric_trace",
        name: "Numeric Trace",
        description: "Statistical report render/check.",
        build: build_numeric_trace,
        requires: &["workspace"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "interactions",
        name: "Interactions",
        description: "User interaction tools (request_directory, ask_user, propose_plan).",
        build: build_interactions,
        requires: &["workspace"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "mcp",
        name: "MCP",
        description: "Model Context Protocol extension tools (registered via manifest).",
        build: build_mcp,
        requires: &[],
        risk: &[
            RiskClass::Read,
            RiskClass::WriteLocal,
            RiskClass::Exec,
            RiskClass::Network,
        ],
    },
    Capability {
        id: "conversation_search",
        name: "Conversation Search",
        description: "Search local chat history (conversation_search).",
        build: build_conversation_search,
        requires: &["workspace", "state_dir"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "library",
        name: "Library",
        description: "Search and index user's file library (library_search, library_index).",
        build: build_library,
        requires: &["workspace", "library"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "memory",
        name: "Memory",
        description: "Propose user memories (memory_propose).",
        build: build_memory,
        requires: &["workspace", "state_dir"],
        risk: &[RiskClass::Read],
    },
    Capability {
        id: "skills",
        name: "Skills",
        description: "Find and read saved skills (find_skill, read_skill).",
        build: build_skills,
        requires: &["workspace", "skills"],
        risk: &[RiskClass::Read],
    },
];

/// Lookup a capability by id.
pub fn capability(id: &str) -> Option<&'static Capability> {
    CAPABILITIES.iter().find(|c| c.id == id)
}

/// Expand a persona's capability ids into concrete registrations.
pub fn expand(ids: &[&str], context: &CapabilityContext) -> Vec<CapabilityRegistration> {
    let mut tools = Vec::new();
    for cap_id in ids {
        if let Some(cap) = capability(cap_id) {
            if cap.available(context) {
                tools.extend((cap.build)(context));
            }
        }
    }
    tools
}

/// Union of risk classes a capability list can produce.
pub fn risk_summary(ids: &[&str]) -> HashSet<RiskClass> {
    let mut out = HashSet::new();
    for cap_id in ids {
        if let Some(cap) = capability(cap_id) {
            out.extend(cap.risk.iter().copied());
        }
    }
    out
}

/// Minimal interaction registration used by the catalog.
fn interaction_registration(
    tool_name: &str,
    description: &str,
    parameters: Value,
) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: format!("interaction.{tool_name}"),
        tool_name: tool_name.to_string(),
        side_effect: SideEffectClass::Read,
        description: description.to_string(),
        parameters,
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "interaction", "capabilities": ["user.interaction"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, _| {
            CapabilityResult::failed(
                &job.job_id,
                "interaction capabilities must be executed by RuntimeHost",
                Some("runtime_boundary"),
            )
        })),
    }
}
