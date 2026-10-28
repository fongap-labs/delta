//! Workspace approval rules (roadmap A3).
//!
//! The approval ladder has three rungs: allow this call once; allow it for the rest of the session
//! (kept in memory by the session's `ApprovalController`); allow it in this workspace from now on.
//! This module is the third rung. A rule is made only when the user clicks the button for it, is
//! listed in the app, and is revoked by the user with one click; revoking it restores the question.
//!
//! A rule answers a question; it never overrules a refusal, and it never applies to a call that
//! sends content off the machine (those are confirmed every time, ADR-0061). A rule is either for
//! a whole tool, or, for a tool that runs a command, for exactly one command string.
//!
//! The authority is one owner-only file, `workspace_rules.json`, in the state directory. A file
//! that cannot be read or does not match the expected shape fails closed (no rule applies) and is
//! never overwritten.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::approval::{ApprovalRecordInput, ApprovalWriter};
use crate::durability::atomic_write_private;
use crate::ShadowReadError;

const AUTHORITY_FILE: &str = "workspace_rules.json";
const AUTHORITY_VERSION: u32 = 1;
/// The most rules one workspace may hold; a user who needs more is allowing too much to review.
pub const MAX_RULES_PER_WORKSPACE: usize = 200;
const MAX_COMMAND_CHARS: usize = 2000;

/// One rule: a tool, and for a command tool the exact command. Without `command` it covers every
/// call of the tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub tool: String,
    #[serde(default)]
    pub command: Option<String>,
    pub created_at: String,
}

impl Rule {
    pub fn to_json(&self) -> Value {
        json!({"tool": self.tool, "command": self.command, "created_at": self.created_at})
    }

    fn covers(&self, tool: &str, arguments: &Value) -> bool {
        if self.tool != tool {
            return false;
        }
        match &self.command {
            None => true,
            Some(command) => arguments.get("command").and_then(Value::as_str) == Some(command),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Authority {
    #[serde(default)]
    version: u32,
    /// Canonical workspace path -> its rules.
    #[serde(default)]
    workspaces: BTreeMap<String, Vec<Rule>>,
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
            "failed to read the workspace rules {}: {error}",
            path.display()
        ))
    })?;
    let authority: Authority = serde_json::from_str(&text).map_err(|error| {
        ShadowReadError::Parse(format!(
            "invalid workspace rules {}: {error}",
            path.display()
        ))
    })?;
    if authority.version != AUTHORITY_VERSION {
        return Err(ShadowReadError::Parse(format!(
            "workspace rules {} have version {}, expected {AUTHORITY_VERSION}",
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

fn canonical(workspace: &str) -> Result<String, ShadowReadError> {
    if workspace.trim().is_empty() {
        return Err(ShadowReadError::Parse(
            "a rule needs a workspace; there is none".to_owned(),
        ));
    }
    PathBuf::from(workspace)
        .canonicalize()
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|error| ShadowReadError::Parse(format!("workspace is unavailable: {error}")))
}

/// The rule a click on "always allow in this workspace" makes for this call: the exact command when
/// the call runs one, the whole tool otherwise. A command that is empty or very long makes no rule.
pub fn rule_for(tool: &str, arguments: &Value) -> Option<(String, Option<String>)> {
    match arguments.get("command") {
        None | Some(Value::Null) => Some((tool.to_owned(), None)),
        Some(Value::String(command))
            if !command.trim().is_empty() && command.chars().count() <= MAX_COMMAND_CHARS =>
        {
            Some((tool.to_owned(), Some(command.clone())))
        }
        Some(_) => None,
    }
}

/// The rules of `workspace`, oldest first.
pub fn list(state_dir: &Path, workspace: &str) -> Result<Vec<Rule>, ShadowReadError> {
    let key = canonical(workspace)?;
    Ok(load(state_dir)?
        .workspaces
        .get(&key)
        .cloned()
        .unwrap_or_default())
}

/// Whether a rule of `workspace` covers this call. A workspace that is not set, or rules that can
/// not be read, cover nothing: the user is asked.
pub fn covers(state_dir: &Path, workspace: &str, tool: &str, arguments: &Value) -> bool {
    list(state_dir, workspace)
        .map(|rules| rules.iter().any(|rule| rule.covers(tool, arguments)))
        .unwrap_or(false)
}

/// Add the rule for this call. Adding a rule that exists changes nothing.
pub fn add(
    state_dir: &Path,
    workspace: &str,
    tool: &str,
    arguments: &Value,
) -> Result<Rule, ShadowReadError> {
    let key = canonical(workspace)?;
    let (tool, command) = rule_for(tool, arguments).ok_or_else(|| {
        ShadowReadError::Parse("this call can not be made into a rule".to_owned())
    })?;
    let mut authority = load(state_dir)?;
    let rules = authority.workspaces.entry(key.clone()).or_default();
    if let Some(existing) = rules
        .iter()
        .find(|rule| rule.tool == tool && rule.command == command)
    {
        return Ok(existing.clone());
    }
    if rules.len() >= MAX_RULES_PER_WORKSPACE {
        return Err(ShadowReadError::Parse(format!(
            "this workspace already has {MAX_RULES_PER_WORKSPACE} rules; revoke some first"
        )));
    }
    let rule = Rule {
        tool,
        command,
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    };
    rules.push(rule.clone());
    save(state_dir, &authority)?;
    audit(state_dir, &key, "created", &rule);
    Ok(rule)
}

/// Remove a rule. Returns whether one was removed. Afterwards the call asks again.
pub fn revoke(
    state_dir: &Path,
    workspace: &str,
    tool: &str,
    command: Option<&str>,
) -> Result<bool, ShadowReadError> {
    let key = canonical(workspace)?;
    let mut authority = load(state_dir)?;
    let Some(rules) = authority.workspaces.get_mut(&key) else {
        return Ok(false);
    };
    let Some(position) = rules
        .iter()
        .position(|rule| rule.tool == tool && rule.command.as_deref() == command)
    else {
        return Ok(false);
    };
    let rule = rules.remove(position);
    if rules.is_empty() {
        authority.workspaces.remove(&key);
    }
    save(state_dir, &authority)?;
    audit(state_dir, &key, "revoked", &rule);
    Ok(true)
}

/// Write the creation or revocation of a rule to the audit log. A failure to write is reported on
/// standard error and never changes the outcome.
fn audit(state_dir: &Path, workspace: &str, stage: &str, rule: &Rule) {
    let path = state_dir.join("audit_events.db");
    let result = ApprovalWriter::open(path.to_string_lossy().as_ref()).and_then(|mut writer| {
        writer.record(
            ApprovalRecordInput {
                session_id: String::new(),
                agent: None,
                workspace: Some(workspace.to_owned()),
                connector: None,
                tool: "approval.rule".to_owned(),
                stage: stage.to_owned(),
                status: Some("ok".to_owned()),
                approval: Some("user".to_owned()),
                arguments: Some(
                    json!({"tool": rule.tool, "command": rule.command, "scope": "workspace"}),
                ),
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
        eprintln!("workspace rules: could not write the audit log: {error}");
    }
}

#[cfg(test)]
mod tests;
