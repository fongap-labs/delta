//! Delta Core Rust runtime — authoritative product state and trusted execution.
//!
//! This crate owns the production `CoreControlPlane` composition and authority root.
//! Product shells call typed Core methods; they do not construct durable authorities
//! or Runtime hosts directly.
//! Checkpoint authority (ADR-029) persists checkpoints as `checkpoint.registered`
//! events in the run-event ledger (`run_events.db`).
//! Policy authority (ADR-030) evaluates tool call risk levels and applies policy slices.
//! Approval authority (ADR-031) persists approval audit events.
//!
//! See:
//!   - docs/architecture/adr/ADR-009-delta-core-architecture.md
//!   - docs/architecture/adr/ADR-051-rust-core-control-plane-authority.md
//!   - docs/architecture/runtime-public-contract.md

pub mod agent;
pub mod application;
pub mod approval;
pub mod artifact;
pub mod automation;
pub mod capability;
pub mod checkpoint;
pub mod control_plane;
pub mod core_control_plane;
pub mod durability;
pub mod extension_manifest;
pub mod idemlog;
pub mod inbox;
pub mod ledger;
pub mod mcp;
pub mod mcp_runtime;
pub mod memory;
pub mod model_authority;
pub mod persistent_worker;
pub mod policy;
mod product_settings;
pub mod provider;
pub mod provider_support;
pub mod retry;
pub mod runtime;
pub mod skills;
pub mod source_citation;
pub mod taskstore;
pub mod tool_lifecycle;
pub mod validation;
pub mod wire;

pub use agent::{AgentDefinition, WorkspacePolicy, DELTA_AGENT};
pub use application::ApplicationStore;
pub use approval::{
    ApprovalController, ApprovalDecision, ApprovalRecordInput, ApprovalRecordOutput,
    ApprovalWriter, APPROVAL_SCHEMA_VERSION,
};
pub use artifact::{
    ArtifactInput, ArtifactMismatch, ArtifactReader, ArtifactRecord, ArtifactRegistrationResult,
    ArtifactRegistryWriter,
};
pub use automation::AutomationStore;
pub use capability::{
    CapabilityArtifact, CapabilityBoundary, CapabilityControl, CapabilityDiagnostics,
    CapabilityExitState, CapabilityGrants, CapabilityHost, CapabilityInputFile, CapabilityJob,
    CapabilityProgress, CapabilityRegistration, CapabilityRegistry, CapabilityResult,
    CapabilityRunner, GrantValidationError, McpCapabilityRunner, NativeCapabilityRunner,
    WorkerProcessRunner, CAPABILITY_ABI_VERSION,
};
pub use checkpoint::{
    CheckpointReader, CheckpointRegisterInput, CheckpointValidationResult, CheckpointWriter,
    CHECKPOINT_SCHEMA_VERSION,
};
pub use control_plane::{
    delete_session, get_session_messages, list_recent_workspaces, list_sessions, rename_session,
    set_session_flags,
};
pub use core_control_plane::{default_state_dir, CoreControlPlane, RuntimeStartRequest};
pub use extension_manifest::{
    load_worker_manifests, WorkerManifestLoadReport, CAPABILITY_WORKER_MANIFEST_FILENAME,
    CAPABILITY_WORKER_MANIFEST_VERSION, EXTENSIONS_DIRNAME,
};
pub use idemlog::{
    args_sha256, operation_id, IdempotencyReader, IdempotencyWriter, SideEffectEntry,
    SideEffectState,
};
pub use ledger::{LedgerEvent, LedgerReader, LedgerWriter};
pub use mcp::McpStore;
pub use mcp_runtime::McpRuntime;
pub use memory::MemoryStore;
pub use model_authority::ModelAuthority;
pub use persistent_worker::PersistentWorkerProcessRunner;
pub use policy::{
    classify, enforce_level, enforce_plan_mode, enforce_scope, evaluate, restrict_grants, Decision,
    ExecutionMode, PolicyEvaluateInput, PolicyEvaluateOutput, RiskLevel, RootEntry,
    POLICY_SCHEMA_VERSION,
};
pub use provider::ProviderRequest;
pub use retry::{
    classify_error, ErrorClass as RetryErrorClass, RetryClassifyInput, RetryClassifyOutput,
};
pub use skills::SkillStore;
pub use source_citation::{
    validate_all, validate_citation, validate_source_citation, CitationValidationResult,
    CitationValidity, SourceCitationReader, SourceCitationWriter, SourceRecord,
    SourceRegisterInput, ValidatedCitation,
};
pub use taskstore::{ScheduledTaskEntry, TaskRunEntry, TaskStore};
pub use tool_lifecycle::{
    cancel as cancel_tool_lifecycle, plan as plan_tool_lifecycle, CancelAction, PlanAction,
    ToolLifecycleCancelInput, ToolLifecycleCancelOutput, ToolLifecyclePlanInput,
    ToolLifecyclePlanOutput,
};
pub use validation::{
    run_validation, ValidationCheck, ValidationReader, ValidationRecord, ValidationRegisterInput,
    ValidationResult, ValidationWriter,
};

pub use inbox::{InboxItem, InboxStore};
pub use runtime::{
    AssistantTurn, EventSink, NullSink, RecoveryReport, RuntimeAuthorities, RuntimeConfig,
    RuntimeEvent, RuntimeEventEnvelopeV1, RuntimeHandle, RuntimeHost, RuntimeState, StagedArtifact,
    StdoutSink, ToolCall, ToolExecutionContext, ToolExecutor, ToolExitState, ToolResult,
};

pub use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShadowReadError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("json decode error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(String),
}
