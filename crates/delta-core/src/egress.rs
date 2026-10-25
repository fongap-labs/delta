//! Workspace egress levels: where a workspace's content may be sent (ADR-0057, ADR-0061).
//!
//! The user chooses one level per workspace and a default for the rest; Delta never chooses one
//! for them. Until a default is chosen nothing is sent (`ModelDecision::NotChosen`). Only the
//! settings commands of the desktop app change a level; a change that lets content go somewhere it
//! could not go before must be confirmed, and every change is written to the audit log.
//!
//! The authority is one owner-only file, `egress.json`, in the state directory. A missing file is
//! the valid first-run state; a file that cannot be read or does not match the expected shape fails
//! closed and is never overwritten.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::{Host, Url};

use crate::approval::{ApprovalRecordInput, ApprovalWriter};
use crate::durability::atomic_write_private;
use crate::policy::ToolMetadata;
use crate::web::guard::{blocked_reason, AddressPolicy};
use crate::ShadowReadError;

const AUTHORITY_FILE: &str = "egress.json";
const AUTHORITY_VERSION: u32 = 1;

/// Where a workspace's content may be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressLevel {
    /// Any configured endpoint.
    Open,
    /// Endpoints the user approved, and endpoints on this machine or the private network.
    ApprovedEndpoints,
    /// Only endpoints on this machine or the private network, or ones the user marked private.
    PrivateOnly,
    /// Any configured endpoint, after the user confirms it for the run.
    AskEachTime,
}

impl EgressLevel {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "open" => Ok(Self::Open),
            "approved_endpoints" => Ok(Self::ApprovedEndpoints),
            "private_only" => Ok(Self::PrivateOnly),
            "ask_each_time" => Ok(Self::AskEachTime),
            other => Err(format!("unknown egress level: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::ApprovedEndpoints => "approved_endpoints",
            Self::PrivateOnly => "private_only",
            Self::AskEachTime => "ask_each_time",
        }
    }
}

/// The user's statement about one endpoint host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointMark {
    /// Sanctioned for *Approved endpoints* workspaces, for example a company AI gateway.
    Approved,
    /// Treated as private although its name or address is public, for example a self-hosted model.
    Private,
}

impl EndpointMark {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "approved" => Ok(Self::Approved),
            "private" => Ok(Self::Private),
            other => Err(format!("unknown endpoint mark: {other}")),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    version: u32,
    #[serde(default)]
    default_level: Option<EgressLevel>,
    /// Canonical workspace path -> level. A workspace not listed follows `default_level`.
    #[serde(default)]
    workspaces: BTreeMap<String, EgressLevel>,
    /// Lower-case host -> mark.
    #[serde(default)]
    endpoint_marks: BTreeMap<String, EndpointMark>,
    /// Local MCP servers the user has stated do not reach the network (ADR-0061).
    #[serde(default)]
    local_only_servers: BTreeSet<String>,
    /// Workspace (canonical path, or "" for every workspace) -> path -> rule (ADR-0061).
    #[serde(default)]
    path_overrides: BTreeMap<String, BTreeMap<String, PathRule>>,
}

fn authority_path(state_dir: &Path) -> PathBuf {
    state_dir.join(AUTHORITY_FILE)
}

fn load(state_dir: &Path) -> Result<Authority, ShadowReadError> {
    let path = authority_path(state_dir);
    if !path.exists() {
        return Ok(Authority {
            version: AUTHORITY_VERSION,
            ..Authority::default()
        });
    }
    let text = std::fs::read_to_string(&path).map_err(|error| {
        ShadowReadError::Parse(format!(
            "failed to read egress authority {}: {error}",
            path.display()
        ))
    })?;
    let authority: Authority = serde_json::from_str(&text).map_err(|error| {
        ShadowReadError::Parse(format!(
            "invalid egress authority {}: {error}",
            path.display()
        ))
    })?;
    if authority.version != AUTHORITY_VERSION {
        return Err(ShadowReadError::Parse(format!(
            "egress authority {} has version {}, expected {AUTHORITY_VERSION}",
            path.display(),
            authority.version
        )));
    }
    Ok(authority)
}

fn save(state_dir: &Path, authority: &Authority) -> Result<(), ShadowReadError> {
    let bytes = serde_json::to_vec_pretty(authority)?;
    atomic_write_private(authority_path(state_dir), &bytes)
}

fn canonical_workspace(path: &str) -> Result<String, ShadowReadError> {
    PathBuf::from(path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|error| ShadowReadError::Parse(format!("workspace is unavailable: {error}")))
}

fn normalized_host(host: &str) -> Result<String, ShadowReadError> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.contains(['/', ':', '@', ' ']) && !host.starts_with('[') {
        return Err(ShadowReadError::Parse(format!(
            "endpoint host is not valid: {host:?}"
        )));
    }
    Ok(host)
}

/// Whether moving from `from` to `to` can let content go somewhere it could not go before
/// without asking. Choosing a level for the first time (`from` is `None`) is not a loosening,
/// nor is any move to *Private only* or away from *Open*.
pub fn is_loosening(from: Option<EgressLevel>, to: Option<EgressLevel>) -> bool {
    match (from, to) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(a), Some(b)) if a == b => false,
        (Some(EgressLevel::Open), Some(_)) => false,
        (Some(_), Some(EgressLevel::PrivateOnly)) => false,
        _ => true,
    }
}

fn record_change(state_dir: &Path, workspace: &str, arguments: Value) {
    let path = state_dir.join("audit_events.db");
    let result = ApprovalWriter::open(path.to_string_lossy().as_ref()).and_then(|mut writer| {
        writer.record(
            ApprovalRecordInput {
                session_id: String::new(),
                agent: None,
                workspace: Some(workspace.to_owned()),
                connector: None,
                tool: "egress.settings".to_owned(),
                stage: "changed".to_owned(),
                status: Some("ok".to_owned()),
                approval: Some("user".to_owned()),
                arguments: Some(arguments),
                result_preview: None,
                reason: None,
                resource: None,
                level: None,
                isolation: None,
                ts: None,
            },
            0.0,
            workspace,
        )
    });
    if let Err(error) = result {
        eprintln!("egress: could not record the settings change in the audit log: {error}");
    }
}

fn needs_confirmation(what: &str) -> Value {
    json!({
        "ok": false,
        "needs_confirmation": true,
        "error": format!("{what} lets content go where it could not go before; confirm the change"),
    })
}

/// The settings as the desktop app shows them.
pub fn settings(state_dir: &Path) -> Result<Value, ShadowReadError> {
    let authority = load(state_dir)?;
    let workspaces: Vec<Value> = authority
        .workspaces
        .iter()
        .map(|(path, level)| json!({"workspace": path, "level": level.as_str()}))
        .collect();
    let marks: Vec<Value> = authority
        .endpoint_marks
        .iter()
        .map(|(host, mark)| json!({"host": host, "mark": mark}))
        .collect();
    let path_overrides: Vec<Value> = authority
        .path_overrides
        .iter()
        .flat_map(|(workspace, paths)| {
            paths.iter().map(move |(path, rule)| {
                json!({"workspace": workspace, "path": path, "rule": rule.as_str()})
            })
        })
        .collect();
    Ok(json!({
        "ok": true,
        "path_overrides": path_overrides,
        "default_level": authority.default_level.map(EgressLevel::as_str),
        "is_chosen": authority.default_level.is_some(),
        "workspaces": workspaces,
        "endpoint_marks": marks,
        "local_only_servers": authority.local_only_servers,
    }))
}

/// The level in force for `workspace` (an empty path means "no workspace"), or `None` while the
/// user has not chosen a default.
pub fn level_for(
    state_dir: &Path,
    workspace: &str,
) -> Result<Option<EgressLevel>, ShadowReadError> {
    let authority = load(state_dir)?;
    if !workspace.trim().is_empty() {
        if let Ok(canonical) = canonical_workspace(workspace) {
            if let Some(level) = authority.workspaces.get(&canonical) {
                return Ok(Some(*level));
            }
        }
    }
    Ok(authority.default_level)
}

/// Set the default level for workspaces without their own. The first choice needs no confirmation.
pub fn set_default_level(
    state_dir: &Path,
    level: EgressLevel,
    is_confirmed: bool,
) -> Result<Value, ShadowReadError> {
    let mut authority = load(state_dir)?;
    let from = authority.default_level;
    if is_loosening(from, Some(level)) && !is_confirmed {
        return Ok(needs_confirmation("the new default level"));
    }
    authority.default_level = Some(level);
    save(state_dir, &authority)?;
    record_change(
        state_dir,
        "",
        json!({"scope": "default", "from": from.map(EgressLevel::as_str), "to": level.as_str(), "confirmed": is_confirmed}),
    );
    settings(state_dir)
}

/// Give `workspace` its own level, or make it follow the default again (`level` is `None`).
pub fn set_workspace_level(
    state_dir: &Path,
    workspace: &str,
    level: Option<EgressLevel>,
    is_confirmed: bool,
) -> Result<Value, ShadowReadError> {
    let canonical = canonical_workspace(workspace)?;
    let mut authority = load(state_dir)?;
    let from = authority
        .workspaces
        .get(&canonical)
        .copied()
        .or(authority.default_level);
    let to = level.or(authority.default_level);
    if is_loosening(from, to) && !is_confirmed {
        return Ok(needs_confirmation("the workspace's new level"));
    }
    match level {
        Some(level) => {
            authority.workspaces.insert(canonical.clone(), level);
        }
        None => {
            authority.workspaces.remove(&canonical);
        }
    }
    save(state_dir, &authority)?;
    record_change(
        state_dir,
        &canonical,
        json!({
            "scope": "workspace",
            "from": from.map(EgressLevel::as_str),
            "to": to.map(EgressLevel::as_str),
            "follows_default": level.is_none(),
            "confirmed": is_confirmed,
        }),
    );
    settings(state_dir)
}

/// Mark an endpoint host as approved or private, or remove its mark (`mark` is `None`). Adding a
/// mark lets content reach the host in more workspaces, so it must be confirmed.
pub fn set_endpoint_mark(
    state_dir: &Path,
    host: &str,
    mark: Option<EndpointMark>,
    is_confirmed: bool,
) -> Result<Value, ShadowReadError> {
    let host = normalized_host(host)?;
    let mut authority = load(state_dir)?;
    let from = authority.endpoint_marks.get(&host).copied();
    if mark.is_some() && mark != from && !is_confirmed {
        return Ok(needs_confirmation("marking this endpoint"));
    }
    match mark {
        Some(mark) => {
            authority.endpoint_marks.insert(host.clone(), mark);
        }
        None => {
            authority.endpoint_marks.remove(&host);
        }
    }
    save(state_dir, &authority)?;
    record_change(
        state_dir,
        "",
        json!({"scope": "endpoint", "host": host, "from": from, "to": mark, "confirmed": is_confirmed}),
    );
    settings(state_dir)
}

/// Whether the user stated that the local MCP server `server` does not reach the network.
pub fn is_local_only(state_dir: &Path, server: &str) -> Result<bool, ShadowReadError> {
    Ok(load(state_dir)?.local_only_servers.contains(server))
}

/// Mark a local MCP server as local-only, or remove the mark. The mark is the user's statement
/// (ADR-0061); Delta does not verify it. Marking lets the server run in *Approved endpoints* and
/// *Private only* workspaces, so it must be confirmed; removing the mark takes effect at once.
pub fn set_local_only(
    state_dir: &Path,
    server: &str,
    is_local_only: bool,
    is_confirmed: bool,
) -> Result<Value, ShadowReadError> {
    let server = server.trim();
    if server.is_empty() {
        return Err(ShadowReadError::Parse(
            "the local server name is empty".to_owned(),
        ));
    }
    let mut authority = load(state_dir)?;
    let was_local_only = authority.local_only_servers.contains(server);
    if is_local_only && !was_local_only && !is_confirmed {
        return Ok(needs_confirmation("marking this server as local-only"));
    }
    if is_local_only {
        authority.local_only_servers.insert(server.to_owned());
    } else {
        authority.local_only_servers.remove(server);
    }
    save(state_dir, &authority)?;
    record_change(
        state_dir,
        "",
        json!({"scope": "local_only", "server": server, "from": was_local_only, "to": is_local_only, "confirmed": is_confirmed}),
    );
    settings(state_dir)
}

/// What Delta knows about a model endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointClass {
    /// Every address the host resolves to is on this machine or a private network.
    Private,
    /// The user marked the host private.
    MarkedPrivate,
    /// The user marked the host approved.
    Approved,
    /// Anything else, including a host with both public and private addresses.
    Public,
}

/// A classified endpoint and, when its class came from its addresses, the addresses that were
/// checked. A connection that relies on the class must use exactly these addresses.
#[derive(Debug, Clone)]
pub struct EndpointInfo {
    pub host: String,
    pub class: EndpointClass,
    pub addrs: Vec<SocketAddr>,
}

impl EndpointClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::MarkedPrivate => "marked_private",
            Self::Approved => "approved",
            Self::Public => "public",
        }
    }
}

/// Classify the endpoint at `base_url`. Names are resolved here; a name that cannot be resolved is
/// an error, never "private".
pub fn classify_endpoint(state_dir: &Path, base_url: &str) -> Result<EndpointInfo, String> {
    let url = Url::parse(base_url.trim()).map_err(|_| "endpoint url is not valid".to_owned())?;
    let host = url
        .host_str()
        .ok_or_else(|| "endpoint url has no host".to_owned())?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let authority = load(state_dir).map_err(|error| error.to_string())?;
    let marked = match authority.endpoint_marks.get(&host) {
        Some(EndpointMark::Private) => Some(EndpointClass::MarkedPrivate),
        Some(EndpointMark::Approved) => Some(EndpointClass::Approved),
        None => None,
    };
    if let Some(class) = marked {
        return Ok(EndpointInfo {
            host,
            class,
            addrs: Vec::new(),
        });
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "endpoint url has no port".to_owned())?;
    let addrs: Vec<SocketAddr> = match url.host() {
        Some(Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        Some(Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        Some(Host::Domain(name)) => (name, port)
            .to_socket_addrs()
            .map_err(|error| format!("could not resolve {name}: {error}"))?
            .collect(),
        None => return Err("endpoint url has no host".to_owned()),
    };
    let is_private = !addrs.is_empty()
        && addrs
            .iter()
            .all(|addr| blocked_reason(addr.ip(), AddressPolicy::STRICT).is_some());
    let class = if is_private {
        EndpointClass::Private
    } else {
        EndpointClass::Public
    };
    Ok(EndpointInfo { host, class, addrs })
}

/// What may happen with a model request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDecision {
    Allow,
    /// Allowed once the user confirms the endpoint for the run.
    Ask,
    /// Refused; the reason names the level and the endpoint class.
    Deny(String),
    /// The user has not chosen a default level yet; nothing may be sent.
    NotChosen,
}

/// Decide a model request to an endpoint of `class` under `level`.
pub fn decide_model_request(level: Option<EgressLevel>, class: &EndpointClass) -> ModelDecision {
    let Some(level) = level else {
        return ModelDecision::NotChosen;
    };
    let is_private = matches!(class, EndpointClass::Private | EndpointClass::MarkedPrivate);
    match level {
        EgressLevel::Open => ModelDecision::Allow,
        EgressLevel::AskEachTime => ModelDecision::Ask,
        EgressLevel::ApprovedEndpoints => {
            if is_private || *class == EndpointClass::Approved {
                ModelDecision::Allow
            } else {
                ModelDecision::Deny(
                    "this workspace may only use approved, local or private endpoints, and this endpoint is none of them".to_owned(),
                )
            }
        }
        EgressLevel::PrivateOnly => {
            if is_private {
                ModelDecision::Allow
            } else {
                ModelDecision::Deny(
                    "this workspace may only use endpoints on this computer or the private network"
                        .to_owned(),
                )
            }
        }
    }
}

/// Where an outbound tool sends content (ADR-0061).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationClass {
    /// A fixed public service, identified by its host.
    FixedPublic,
    /// Any public host: the model chooses it (web fetch, browser automation).
    AnyPublic,
    /// A host on this machine or the private network, or one the user marked private.
    Private,
    /// A local process; its own network traffic is outside Delta's view.
    LocalProcess,
}

impl DestinationClass {
    /// The class a tool declares in its metadata, or `None` for an unknown name.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "fixed_public" => Some(Self::FixedPublic),
            "any_public" => Some(Self::AnyPublic),
            "private" => Some(Self::Private),
            "local_process" => Some(Self::LocalProcess),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FixedPublic => "fixed_public",
            Self::AnyPublic => "any_public",
            Self::Private => "private",
            Self::LocalProcess => "local_process",
        }
    }
}

/// Where one outbound tool call goes, as far as Delta knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundTarget {
    pub class: DestinationClass,
    /// The host, or empty when the call names none (a local process, or the page a browser shows).
    pub host: String,
    /// The user's mark for the host, if there is one.
    pub mark: Option<EndpointMark>,
    /// For a local process: the user stated it does not reach the network.
    pub local_only: bool,
    /// The path a per-path rule can name (see [`path_key`]).
    pub path: String,
}

/// What may happen with an outbound tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundDecision {
    Allow,
    /// Allowed once the user approves this call. Standing grants and automatic approval do not apply.
    Ask,
    /// Refused; the reason names the level and the destination class. Nothing is sent.
    Deny(String),
    /// The user has not chosen a default level yet; nothing may be sent.
    NotChosen,
}

/// The host of `url`, lowercase and without a trailing dot; empty when it has none.
pub fn url_host(url: &str) -> String {
    Url::parse(url.trim())
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
        })
        .unwrap_or_default()
}

/// The user's mark for `host`, if any.
pub fn endpoint_mark(
    state_dir: &Path,
    host: &str,
) -> Result<Option<EndpointMark>, ShadowReadError> {
    Ok(load(state_dir)?.endpoint_marks.get(host).copied())
}

/// Governed categories: tools in them reach outside the machine unless they declare otherwise.
const GOVERNED_CATEGORIES: &[&str] = &["web", "mcp", "connector"];

/// The class the rules of every level see: a host the user marked private is private (ADR-0057).
pub fn effective_class(target: &OutboundTarget) -> DestinationClass {
    match (target.class, target.mark) {
        (
            DestinationClass::FixedPublic | DestinationClass::AnyPublic,
            Some(EndpointMark::Private),
        ) => DestinationClass::Private,
        (class, _) => class,
    }
}

/// Decide an outbound tool call to `target` under `level` (ADR-0061, table in point 2).
pub fn decide_outbound(level: Option<EgressLevel>, target: &OutboundTarget) -> OutboundDecision {
    let Some(level) = level else {
        return OutboundDecision::NotChosen;
    };
    let class = effective_class(target);
    let approved = target.mark == Some(EndpointMark::Approved);
    match level {
        EgressLevel::Open => OutboundDecision::Allow,
        EgressLevel::AskEachTime => OutboundDecision::Ask,
        EgressLevel::ApprovedEndpoints => match class {
            DestinationClass::Private => OutboundDecision::Allow,
            DestinationClass::FixedPublic if approved => OutboundDecision::Allow,
            DestinationClass::LocalProcess if target.local_only => OutboundDecision::Allow,
            DestinationClass::AnyPublic => OutboundDecision::Ask,
            _ => OutboundDecision::Deny(outbound_refusal(level, class)),
        },
        EgressLevel::PrivateOnly => match class {
            DestinationClass::Private => OutboundDecision::Allow,
            DestinationClass::LocalProcess if target.local_only => OutboundDecision::Allow,
            _ => OutboundDecision::Deny(outbound_refusal(level, class)),
        },
    }
}

/// The outbound target of a tool call, or `None` for a tool that sends nothing off the machine
/// (ADR-0061). A tool in a governed category without a declared destination is `any_public`.
/// `arguments` is `None` when the call is not known yet, as when the model is offered the tool.
pub fn outbound_target(
    state_dir: &Path,
    metadata: &ToolMetadata,
    tool_name: &str,
    arguments: Option<&Value>,
) -> Result<Option<OutboundTarget>, String> {
    let category = metadata
        .category
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let declared = metadata.destination.as_deref();
    if declared.is_none() && !GOVERNED_CATEGORIES.contains(&category.as_str()) {
        return Ok(None);
    }
    let mut class = declared
        .and_then(DestinationClass::parse)
        .unwrap_or(DestinationClass::AnyPublic);
    // A fixed URL (a remote MCP server) is judged by where it resolves, as a model endpoint is.
    let host = match &metadata.destination_url {
        Some(url) => {
            if class == DestinationClass::FixedPublic {
                if let Ok(info) = classify_endpoint(state_dir, url) {
                    if matches!(
                        info.class,
                        EndpointClass::Private | EndpointClass::MarkedPrivate
                    ) {
                        class = DestinationClass::Private;
                    }
                }
            }
            url_host(url)
        }
        None => arguments
            .and_then(|arguments| arguments.get("url"))
            .and_then(Value::as_str)
            .map(url_host)
            .unwrap_or_default(),
    };
    let local_only = match (&metadata.server, class) {
        (Some(server), DestinationClass::LocalProcess) => is_local_only(state_dir, server)
            .map_err(|error| {
                format!("The local server marks could not be read, so nothing was sent: {error}")
            })?,
        _ => false,
    };
    let mark = if host.is_empty() {
        None
    } else {
        endpoint_mark(state_dir, &host).map_err(|error| {
            format!("The endpoint marks could not be read, so nothing was sent: {error}")
        })?
    };
    Ok(Some(OutboundTarget {
        class,
        host,
        mark,
        local_only,
        path: path_key(tool_name, metadata),
    }))
}

/// Whether the model is offered `metadata`'s tool in `workspace`: a tool the level refuses for
/// every call is not offered at all. Errors hide the tool.
pub fn offered_to_model(
    state_dir: &Path,
    workspace: &str,
    tool_name: &str,
    metadata: &ToolMetadata,
) -> bool {
    let Ok(level) = level_for(state_dir, workspace) else {
        return false;
    };
    match outbound_target(state_dir, metadata, tool_name, None) {
        Ok(None) => true,
        Ok(Some(target)) => {
            let Ok(rule) = path_rule(state_dir, workspace, &target.path) else {
                return false;
            };
            !matches!(
                decide_path(level, &target, rule),
                OutboundDecision::Deny(_) | OutboundDecision::NotChosen
            )
        }
        Err(_) => false,
    }
}

/// A rule the user sets for one path in one workspace (ADR-0061 point 3). It replaces the level's
/// decision for that path: `allow` lets it go without the level's refusal, `ask` asks for each call,
/// `deny` refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathRule {
    Allow,
    Ask,
    Deny,
}

impl PathRule {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "allow" => Ok(Self::Allow),
            "ask" => Ok(Self::Ask),
            "deny" => Ok(Self::Deny),
            other => Err(format!("unknown path rule: {other}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }
}

/// The path a tool's decision is kept under: a remote or local MCP server by name, the web tools
/// and browser automation by family, anything else by its tool name.
pub fn path_key(tool_name: &str, metadata: &ToolMetadata) -> String {
    let category = metadata
        .category
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    if category == "mcp" {
        return format!("mcp:{}", metadata.server.as_deref().unwrap_or(""));
    }
    if category == "connector" {
        return format!("connector:{}", metadata.connector.as_deref().unwrap_or(""));
    }
    if tool_name.starts_with("browser_") {
        return "browser".to_owned();
    }
    tool_name.to_owned()
}

/// Paths a rule may name: the web tools, browser automation, opening an address, and MCP servers.
fn is_path_key(path: &str) -> bool {
    matches!(path, "web_search" | "web_fetch" | "browser" | "open_url")
        || path
            .strip_prefix("mcp:")
            .is_some_and(|name| !name.trim().is_empty())
        || path
            .strip_prefix("connector:")
            .is_some_and(|name| !name.trim().is_empty())
}

/// The workspace key rules are stored under: its canonical path, or "" for every workspace.
fn workspace_key(workspace: &str) -> String {
    if workspace.trim().is_empty() {
        return String::new();
    }
    canonical_workspace(workspace).unwrap_or_else(|_| workspace.to_owned())
}

/// The rule in force for `path` in `workspace`: the workspace's own rule, else the rule for every
/// workspace, else none.
pub fn path_rule(
    state_dir: &Path,
    workspace: &str,
    path: &str,
) -> Result<Option<PathRule>, ShadowReadError> {
    let authority = load(state_dir)?;
    if !workspace.trim().is_empty() {
        let own = authority
            .path_overrides
            .get(&workspace_key(workspace))
            .and_then(|paths| paths.get(path));
        if let Some(rule) = own {
            return Ok(Some(*rule));
        }
    }
    Ok(authority
        .path_overrides
        .get("")
        .and_then(|paths| paths.get(path))
        .copied())
}

/// The decision for a call on `target` under `level`, with the path's rule applied when there is
/// one. Without a chosen level nothing is allowed, whatever the rule says.
pub fn decide_path(
    level: Option<EgressLevel>,
    target: &OutboundTarget,
    rule: Option<PathRule>,
) -> OutboundDecision {
    if level.is_none() {
        return OutboundDecision::NotChosen;
    }
    match rule {
        None => decide_outbound(level, target),
        Some(PathRule::Allow) => OutboundDecision::Allow,
        Some(PathRule::Ask) => OutboundDecision::Ask,
        Some(PathRule::Deny) => OutboundDecision::Deny(
            "Blocked for this path by a rule in Settings ▸ Privacy. Nothing was sent.".to_owned(),
        ),
    }
}

/// How much a decision lets through: a larger number lets more content go.
fn decision_rank(decision: &OutboundDecision) -> u8 {
    match decision {
        OutboundDecision::Allow => 2,
        OutboundDecision::Ask => 1,
        OutboundDecision::Deny(_) | OutboundDecision::NotChosen => 0,
    }
}

/// Set (`rule` is `Some`) or remove (`None`) the rule for `path` in `workspace` (`""` for every
/// workspace). `class` is the destination class the path has: `any_public` for the web tools and
/// browser automation, `fixed_public` or `local_process` for an MCP server. A rule that lets more
/// content go than the level does now asks for confirmation; removing a rule takes effect at once.
pub fn set_path_rule(
    state_dir: &Path,
    workspace: &str,
    path: &str,
    class: &str,
    rule: Option<PathRule>,
    is_confirmed: bool,
) -> Result<Value, ShadowReadError> {
    let invalid = |why: &str| ShadowReadError::Parse(why.to_owned());
    if !is_path_key(path) {
        return Err(invalid("unknown path for a rule"));
    }
    let Some(level) = level_for(state_dir, workspace)? else {
        return Err(invalid(
            "choose a default egress level before setting a per-path rule",
        ));
    };
    let class =
        DestinationClass::parse(class).ok_or_else(|| invalid("unknown destination class"))?;
    let class_fits = if path.starts_with("mcp:") {
        matches!(
            class,
            DestinationClass::FixedPublic | DestinationClass::LocalProcess
        )
    } else if path.starts_with("connector:") {
        matches!(
            class,
            DestinationClass::FixedPublic | DestinationClass::AnyPublic
        )
    } else {
        class == DestinationClass::AnyPublic
    };
    if !class_fits {
        return Err(invalid("the destination class does not fit this path"));
    }
    let local_only = match path.strip_prefix("mcp:") {
        Some(server) if class == DestinationClass::LocalProcess => {
            is_local_only(state_dir, server)?
        }
        _ => false,
    };
    let target = OutboundTarget {
        class,
        host: String::new(),
        mark: None,
        local_only,
        path: path.to_owned(),
    };

    let key = workspace_key(workspace);
    let mut authority = load(state_dir)?;
    let inherited = if key.is_empty() {
        None
    } else {
        authority
            .path_overrides
            .get("")
            .and_then(|paths| paths.get(path))
            .copied()
    };
    let before = path_rule(state_dir, workspace, path)?;
    let from = decide_path(Some(level), &target, before);
    let after = rule.or(inherited);
    let to = decide_path(Some(level), &target, after);
    if decision_rank(&to) > decision_rank(&from) && !is_confirmed {
        return Ok(needs_confirmation("allowing this path"));
    }

    let emptied = {
        let paths = authority.path_overrides.entry(key.clone()).or_default();
        match rule {
            Some(rule) => {
                paths.insert(path.to_owned(), rule);
            }
            None => {
                paths.remove(path);
            }
        }
        paths.is_empty()
    };
    if emptied {
        authority.path_overrides.remove(&key);
    }
    save(state_dir, &authority)?;
    record_change(
        state_dir,
        &key,
        json!({
            "scope": "path",
            "path": path,
            "from": before.map(PathRule::as_str),
            "to": rule.map(PathRule::as_str),
            "confirmed": is_confirmed,
        }),
    );
    settings(state_dir)
}

fn outbound_refusal(level: EgressLevel, class: DestinationClass) -> String {
    let level = match level {
        EgressLevel::ApprovedEndpoints => "Approved endpoints",
        EgressLevel::PrivateOnly => "Private only",
        EgressLevel::Open => "Open",
        EgressLevel::AskEachTime => "Ask each time",
    };
    let destination = match class {
        DestinationClass::FixedPublic => "a public service that is not approved",
        DestinationClass::AnyPublic => "a public site chosen by the model",
        DestinationClass::Private => "a private network",
        DestinationClass::LocalProcess => "a local program whose traffic Delta cannot see",
    };
    format!(
        "This workspace's egress level is {level}, and this tool would send content to {destination}. Nothing was sent. Change the level in Settings ▸ Privacy."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(class: DestinationClass, mark: Option<EndpointMark>) -> OutboundTarget {
        OutboundTarget {
            class,
            host: "example.test".to_owned(),
            mark,
            local_only: false,
            path: "web_search".to_owned(),
        }
    }

    fn metadata(category: &str, destination: Option<&str>) -> ToolMetadata {
        ToolMetadata {
            risk_level: Some("medium".to_owned()),
            requires_approval: Some(true),
            category: Some(category.to_owned()),
            capabilities: Some(Vec::new()),
            destination: destination.map(str::to_owned),
            destination_url: None,
            server: Some("notes".to_owned()),
            connector: None,
            side_effect: None,
        }
    }

    #[test]
    fn a_local_server_marked_local_only_runs_in_the_restrictive_levels() {
        let local = |local_only| OutboundTarget {
            local_only,
            ..target(DestinationClass::LocalProcess, None)
        };
        for level in [EgressLevel::ApprovedEndpoints, EgressLevel::PrivateOnly] {
            assert!(matches!(
                decide_outbound(Some(level), &local(false)),
                OutboundDecision::Deny(_)
            ));
            assert_eq!(
                decide_outbound(Some(level), &local(true)),
                OutboundDecision::Allow
            );
        }
    }

    #[test]
    fn marking_a_local_server_needs_confirmation_and_removing_the_mark_does_not() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        let asked = set_local_only(state, "notes", true, false).unwrap();
        assert_eq!(asked["needs_confirmation"], true);
        assert!(!is_local_only(state, "notes").unwrap());

        set_local_only(state, "notes", true, true).unwrap();
        assert!(is_local_only(state, "notes").unwrap());
        assert_eq!(settings(state).unwrap()["local_only_servers"][0], "notes");

        // Removing the mark tightens at once.
        set_local_only(state, "notes", false, false).unwrap();
        assert!(!is_local_only(state, "notes").unwrap());
        assert!(set_local_only(state, "  ", true, true).is_err());
    }

    #[test]
    fn outbound_target_reads_the_local_only_mark_for_the_server() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        let stdio = metadata("mcp", Some("local_process"));
        let target = outbound_target(state, &stdio, "notes", None)
            .unwrap()
            .unwrap();
        assert!(!target.local_only);
        set_local_only(state, "notes", true, true).unwrap();
        let target = outbound_target(state, &stdio, "notes", None)
            .unwrap()
            .unwrap();
        assert!(target.local_only);
        assert_eq!(target.class, DestinationClass::LocalProcess);
        // A tool outside the outbound categories has no target.
        assert!(
            outbound_target(state, &metadata("read", None), "read_report", None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_tool_the_level_refuses_for_every_call_is_not_offered_to_the_model() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        let workspace = "";
        let web = metadata("web", Some("any_public"));
        let stdio = metadata("mcp", Some("local_process"));
        let local_read = metadata("read", None);

        // No chosen level: outbound tools are not offered; nothing outside them is hidden.
        assert!(!offered_to_model(state, workspace, "web_fetch", &web));
        assert!(offered_to_model(
            state,
            workspace,
            "read_report",
            &local_read
        ));

        set_default_level(state, EgressLevel::PrivateOnly, false).unwrap();
        assert!(!offered_to_model(state, workspace, "web_fetch", &web));
        assert!(!offered_to_model(state, workspace, "notes", &stdio));
        assert!(offered_to_model(
            state,
            workspace,
            "read_report",
            &local_read
        ));

        set_local_only(state, "notes", true, true).unwrap();
        assert!(offered_to_model(state, workspace, "notes", &stdio));

        // Ask each time offers every outbound tool: each call is asked, not refused.
        set_default_level(state, EgressLevel::AskEachTime, true).unwrap();
        assert!(offered_to_model(state, workspace, "web_fetch", &web));
    }

    #[test]
    fn outbound_decisions_follow_the_level_table() {
        use DestinationClass::*;
        use EgressLevel::*;
        let allowed = |decision: OutboundDecision| decision == OutboundDecision::Allow;
        let denied = |decision: &OutboundDecision| matches!(decision, OutboundDecision::Deny(_));

        assert_eq!(
            decide_outbound(None, &target(AnyPublic, None)),
            OutboundDecision::NotChosen
        );
        for class in [FixedPublic, AnyPublic, Private, LocalProcess] {
            assert!(allowed(decide_outbound(Some(Open), &target(class, None))));
            assert_eq!(
                decide_outbound(Some(AskEachTime), &target(class, None)),
                OutboundDecision::Ask
            );
        }
        // Approved endpoints: fixed services only when approved, pages asked per call, private allowed.
        assert!(denied(&decide_outbound(
            Some(ApprovedEndpoints),
            &target(FixedPublic, None)
        )));
        assert!(allowed(decide_outbound(
            Some(ApprovedEndpoints),
            &target(FixedPublic, Some(EndpointMark::Approved))
        )));
        assert_eq!(
            decide_outbound(Some(ApprovedEndpoints), &target(AnyPublic, None)),
            OutboundDecision::Ask
        );
        assert!(allowed(decide_outbound(
            Some(ApprovedEndpoints),
            &target(Private, None)
        )));
        assert!(denied(&decide_outbound(
            Some(ApprovedEndpoints),
            &target(LocalProcess, None)
        )));
        // Private only: nothing public, a private host or a local process is refused.
        assert!(denied(&decide_outbound(
            Some(PrivateOnly),
            &target(FixedPublic, None)
        )));
        assert!(denied(&decide_outbound(
            Some(PrivateOnly),
            &target(AnyPublic, None)
        )));
        assert!(denied(&decide_outbound(
            Some(PrivateOnly),
            &target(LocalProcess, None)
        )));
        assert!(allowed(decide_outbound(
            Some(PrivateOnly),
            &target(Private, None)
        )));
    }

    #[test]
    fn a_host_marked_private_counts_as_private_under_every_level() {
        let marked = target(DestinationClass::AnyPublic, Some(EndpointMark::Private));
        assert!(matches!(
            decide_outbound(Some(EgressLevel::PrivateOnly), &marked),
            OutboundDecision::Allow
        ));
        assert!(matches!(
            decide_outbound(Some(EgressLevel::ApprovedEndpoints), &marked),
            OutboundDecision::Allow
        ));
    }

    #[test]
    fn url_host_is_lowercase_without_trailing_dot() {
        assert_eq!(
            url_host("https://Docs.Example.TEST./a?b=1"),
            "docs.example.test"
        );
        assert_eq!(url_host("not a url"), "");
    }

    #[test]
    fn a_path_rule_replaces_the_levels_decision_and_loosening_is_confirmed() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        set_default_level(state, EgressLevel::PrivateOnly, false).unwrap();
        let search = target(DestinationClass::AnyPublic, None);
        assert!(matches!(
            decide_path(Some(EgressLevel::PrivateOnly), &search, None),
            OutboundDecision::Deny(_)
        ));

        let asked = set_path_rule(
            state,
            "",
            "web_search",
            "any_public",
            Some(PathRule::Allow),
            false,
        )
        .unwrap();
        assert_eq!(asked["needs_confirmation"], true);
        assert_eq!(path_rule(state, "", "web_search").unwrap(), None);

        set_path_rule(
            state,
            "",
            "web_search",
            "any_public",
            Some(PathRule::Allow),
            true,
        )
        .unwrap();
        assert_eq!(
            path_rule(state, "", "web_search").unwrap(),
            Some(PathRule::Allow)
        );
        assert_eq!(
            decide_path(
                Some(EgressLevel::PrivateOnly),
                &search,
                path_rule(state, "", "web_search").unwrap()
            ),
            OutboundDecision::Allow
        );
        assert_eq!(
            settings(state).unwrap()["path_overrides"][0]["rule"],
            "allow"
        );

        // Removing the rule tightens at once, without confirmation.
        set_path_rule(state, "", "web_search", "any_public", None, false).unwrap();
        assert_eq!(path_rule(state, "", "web_search").unwrap(), None);
        assert!(settings(state).unwrap()["path_overrides"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_deny_rule_hides_the_tool_even_when_the_level_allows_it() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        set_default_level(state, EgressLevel::Open, false).unwrap();
        let web = metadata("web", Some("any_public"));
        assert!(offered_to_model(state, "", "web_fetch", &web));
        set_path_rule(
            state,
            "",
            "web_fetch",
            "any_public",
            Some(PathRule::Deny),
            false,
        )
        .unwrap();
        assert!(!offered_to_model(state, "", "web_fetch", &web));
        // Another path is untouched.
        assert!(offered_to_model(
            state,
            "",
            "web_search",
            &metadata("web", Some("any_public"))
        ));
    }

    #[test]
    fn a_workspace_rule_takes_precedence_over_the_rule_for_every_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        let workspace = tempfile::tempdir().unwrap();
        let workspace_path = workspace.path().to_string_lossy().to_string();
        set_default_level(state, EgressLevel::PrivateOnly, false).unwrap();
        set_path_rule(
            state,
            "",
            "web_fetch",
            "any_public",
            Some(PathRule::Deny),
            false,
        )
        .unwrap();
        set_path_rule(
            state,
            &workspace_path,
            "web_fetch",
            "any_public",
            Some(PathRule::Allow),
            true,
        )
        .unwrap();
        assert_eq!(
            path_rule(state, &workspace_path, "web_fetch").unwrap(),
            Some(PathRule::Allow)
        );
        assert_eq!(
            path_rule(state, "", "web_fetch").unwrap(),
            Some(PathRule::Deny)
        );
        // A workspace without its own rule follows the rule for every workspace.
        let other = tempfile::tempdir().unwrap();
        assert_eq!(
            path_rule(state, &other.path().to_string_lossy(), "web_fetch").unwrap(),
            Some(PathRule::Deny)
        );
    }

    #[test]
    fn path_rules_name_only_known_paths_with_a_class_that_fits() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        set_default_level(state, EgressLevel::PrivateOnly, false).unwrap();
        let rule = Some(PathRule::Allow);
        assert!(set_path_rule(state, "", "shell", "any_public", rule, true).is_err());
        assert!(set_path_rule(state, "", "web_search", "fixed_public", rule, true).is_err());
        assert!(set_path_rule(state, "", "mcp:", "local_process", rule, true).is_err());
        assert!(set_path_rule(state, "", "mcp:notes", "any_public", rule, true).is_err());
        assert!(set_path_rule(state, "", "mcp:notes", "local_process", rule, true).is_ok());
        // Without a chosen level no rule can be set.
        let fresh = tempfile::tempdir().unwrap();
        assert!(set_path_rule(fresh.path(), "", "web_search", "any_public", rule, true).is_err());
    }

    #[test]
    fn a_connector_without_a_declared_destination_is_any_public_and_keyed_by_name() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        let mut connector = metadata("connector", None);
        connector.connector = Some("mail".to_owned());
        assert_eq!(path_key("mail_send", &connector), "connector:mail");
        let target = outbound_target(state, &connector, "mail_send", None)
            .unwrap()
            .unwrap();
        assert_eq!(target.class, DestinationClass::AnyPublic);

        set_default_level(state, EgressLevel::PrivateOnly, false).unwrap();
        assert!(!offered_to_model(state, "", "mail_send", &connector));
        set_default_level(state, EgressLevel::Open, true).unwrap();
        assert!(offered_to_model(state, "", "mail_send", &connector));
    }

    #[test]
    fn a_connector_rule_accepts_only_a_fixed_or_public_destination() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path();
        set_default_level(state, EgressLevel::PrivateOnly, false).unwrap();
        let rule = Some(PathRule::Allow);
        assert!(set_path_rule(state, "", "connector:mail", "fixed_public", rule, true).is_ok());
        assert!(set_path_rule(state, "", "connector:mail", "any_public", rule, true).is_ok());
        assert!(set_path_rule(state, "", "connector:mail", "local_process", rule, true).is_err());
        assert!(set_path_rule(state, "", "connector:", "fixed_public", rule, true).is_err());
    }

    fn state() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn nothing_is_chosen_on_first_run() {
        let dir = state();
        assert_eq!(level_for(dir.path(), "").unwrap(), None);
        assert_eq!(
            decide_model_request(None, &EndpointClass::Private),
            ModelDecision::NotChosen
        );
        assert_eq!(settings(dir.path()).unwrap()["is_chosen"], false);
    }

    #[test]
    fn the_first_choice_needs_no_confirmation_and_existing_workspaces_follow_it() {
        let dir = state();
        let workspace = state();
        let out = set_default_level(dir.path(), EgressLevel::AskEachTime, false).unwrap();
        assert_eq!(out["default_level"], "ask_each_time");
        let ws = workspace.path().to_str().unwrap();
        assert_eq!(
            level_for(dir.path(), ws).unwrap(),
            Some(EgressLevel::AskEachTime)
        );
    }

    #[test]
    fn loosening_without_confirmation_changes_nothing() {
        let dir = state();
        set_default_level(dir.path(), EgressLevel::PrivateOnly, false).unwrap();
        let refused = set_default_level(dir.path(), EgressLevel::Open, false).unwrap();
        assert_eq!(refused["needs_confirmation"], true);
        assert_eq!(
            level_for(dir.path(), "").unwrap(),
            Some(EgressLevel::PrivateOnly)
        );
        set_default_level(dir.path(), EgressLevel::Open, true).unwrap();
        assert_eq!(level_for(dir.path(), "").unwrap(), Some(EgressLevel::Open));
    }

    #[test]
    fn tightening_takes_effect_at_once() {
        let dir = state();
        let workspace = state();
        let ws = workspace.path().to_str().unwrap();
        set_default_level(dir.path(), EgressLevel::Open, false).unwrap();
        set_workspace_level(dir.path(), ws, Some(EgressLevel::PrivateOnly), false).unwrap();
        assert_eq!(
            level_for(dir.path(), ws).unwrap(),
            Some(EgressLevel::PrivateOnly)
        );
        // Following an Open default again is a loosening.
        let refused = set_workspace_level(dir.path(), ws, None, false).unwrap();
        assert_eq!(refused["needs_confirmation"], true);
        assert_eq!(
            level_for(dir.path(), ws).unwrap(),
            Some(EgressLevel::PrivateOnly)
        );
    }

    #[test]
    fn loosening_rules() {
        use EgressLevel::*;
        assert!(!is_loosening(None, Some(Open)));
        assert!(!is_loosening(Some(Open), Some(PrivateOnly)));
        assert!(!is_loosening(Some(AskEachTime), Some(PrivateOnly)));
        assert!(!is_loosening(Some(Open), Some(AskEachTime)));
        assert!(is_loosening(Some(PrivateOnly), Some(ApprovedEndpoints)));
        assert!(is_loosening(Some(AskEachTime), Some(ApprovedEndpoints)));
        assert!(is_loosening(Some(ApprovedEndpoints), Some(AskEachTime)));
        assert!(is_loosening(Some(PrivateOnly), None));
    }

    #[test]
    fn marking_an_endpoint_needs_confirmation_and_removing_does_not() {
        let dir = state();
        let refused = set_endpoint_mark(
            dir.path(),
            "Gateway.Example.COM",
            Some(EndpointMark::Approved),
            false,
        )
        .unwrap();
        assert_eq!(refused["needs_confirmation"], true);
        set_endpoint_mark(
            dir.path(),
            "Gateway.Example.COM",
            Some(EndpointMark::Approved),
            true,
        )
        .unwrap();
        let info = classify_endpoint(dir.path(), "https://gateway.example.com/v1").unwrap();
        assert_eq!(info.host, "gateway.example.com");
        assert_eq!(info.class, EndpointClass::Approved);
        let removed = set_endpoint_mark(dir.path(), "gateway.example.com", None, false).unwrap();
        assert_eq!(removed["ok"], true);
    }

    #[test]
    fn loopback_and_private_addresses_are_private_and_unmarked_public_ones_are_not() {
        let dir = state();
        for url in [
            "http://127.0.0.1:11434/v1",
            "http://[::1]:8000",
            "http://10.1.2.3/v1",
            "http://192.168.1.20:8080",
            "http://[fd00::1]/v1",
            "http://localhost:11434",
        ] {
            let info = classify_endpoint(dir.path(), url).unwrap();
            assert_eq!(info.class, EndpointClass::Private, "{url}");
            assert!(!info.addrs.is_empty(), "{url}");
        }
        assert_eq!(
            classify_endpoint(dir.path(), "https://8.8.8.8/v1")
                .unwrap()
                .class,
            EndpointClass::Public
        );
    }

    #[test]
    fn a_marked_private_host_counts_as_private() {
        let dir = state();
        set_endpoint_mark(
            dir.path(),
            "llm.corp.example",
            Some(EndpointMark::Private),
            true,
        )
        .unwrap();
        assert_eq!(
            classify_endpoint(dir.path(), "https://llm.corp.example/v1")
                .unwrap()
                .class,
            EndpointClass::MarkedPrivate
        );
    }

    #[test]
    fn decisions_follow_the_level() {
        use EgressLevel::*;
        let public = EndpointClass::Public;
        assert_eq!(
            decide_model_request(Some(Open), &public),
            ModelDecision::Allow
        );
        assert_eq!(
            decide_model_request(Some(AskEachTime), &public),
            ModelDecision::Ask
        );
        assert!(matches!(
            decide_model_request(Some(PrivateOnly), &public),
            ModelDecision::Deny(_)
        ));
        assert!(matches!(
            decide_model_request(Some(ApprovedEndpoints), &public),
            ModelDecision::Deny(_)
        ));
        assert_eq!(
            decide_model_request(Some(ApprovedEndpoints), &EndpointClass::Approved),
            ModelDecision::Allow
        );
        assert_eq!(
            decide_model_request(Some(PrivateOnly), &EndpointClass::Approved),
            decide_model_request(Some(PrivateOnly), &EndpointClass::Public)
        );
        assert_eq!(
            decide_model_request(Some(PrivateOnly), &EndpointClass::MarkedPrivate),
            ModelDecision::Allow
        );
    }

    #[test]
    fn a_corrupt_authority_fails_closed_and_is_not_overwritten() {
        let dir = state();
        std::fs::write(dir.path().join(AUTHORITY_FILE), b"{not json").unwrap();
        assert!(level_for(dir.path(), "").is_err());
        assert!(set_default_level(dir.path(), EgressLevel::PrivateOnly, true).is_err());
        assert_eq!(
            std::fs::read(dir.path().join(AUTHORITY_FILE)).unwrap(),
            b"{not json"
        );
    }

    #[test]
    fn an_unknown_field_or_version_fails_closed() {
        let dir = state();
        std::fs::write(
            dir.path().join(AUTHORITY_FILE),
            br#"{"version":1,"default_level":"open","extra":true}"#,
        )
        .unwrap();
        assert!(level_for(dir.path(), "").is_err());
        std::fs::write(dir.path().join(AUTHORITY_FILE), br#"{"version":2}"#).unwrap();
        assert!(level_for(dir.path(), "").is_err());
    }

    #[test]
    fn every_change_is_in_the_audit_log() {
        let dir = state();
        set_default_level(dir.path(), EgressLevel::PrivateOnly, false).unwrap();
        set_default_level(dir.path(), EgressLevel::Open, true).unwrap();
        let writer = ApprovalWriter::open(
            dir.path()
                .join("audit_events.db")
                .to_string_lossy()
                .as_ref(),
        )
        .unwrap();
        let events = writer
            .list(10, None, None, Some("egress.settings"))
            .unwrap();
        assert_eq!(events.len(), 2);
    }
}
