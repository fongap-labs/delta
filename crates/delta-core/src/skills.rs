//! Rust authority for folder-backed Delta skills.
//!
//! A Skill is a controlled extension unit, not a second agent. Its `SKILL.md`
//! front-matter declares an executable contract that the Runtime / Policy /
//! Capability layer consumes; the Skill itself never runs its own scheduler,
//! approval, retry, or authority — every tool call still flows through the
//! single main Agent via `Task -> Policy -> Capability/Worker -> Validation ->
//! Artifact/Ledger`.

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, ErrorKind, Read};
use std::path::{Component, Path, PathBuf};

use std::sync::Arc;

use base64::Engine;
use serde_json::{json, Value};

use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};

use crate::{durability::atomic_write, ShadowReadError};

/// Declared side-effect class of a Skill's execution. This is metadata the
/// Policy layer uses to bound what a Skill may cause; it is not an execution
/// path (the main Agent still runs every tool call through Policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideEffectClass {
    #[default]
    /// No side effects — pure reasoning / analysis.
    None,
    /// Read-only access (files, search, inspection).
    Read,
    /// Reversible local writes within the workspace.
    LocalWrite,
    /// External effects (messages, connectors, remote writes).
    ExternalWrite,
    /// Irreversible or destructive actions.
    Destructive,
}

impl SideEffectClass {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "read" | "read_only" | "read-only" => Some(Self::Read),
            "local_write" | "local-write" | "write" => Some(Self::LocalWrite),
            "external_write" | "external-write" | "external" => Some(Self::ExternalWrite),
            "destructive" => Some(Self::Destructive),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::LocalWrite => "local_write",
            Self::ExternalWrite => "external_write",
            Self::Destructive => "destructive",
        }
    }
}

/// The skills that ship with Delta (roadmap B3). They are compiled into the binary, so they need
/// no install and no network, and they are read-only: the user can turn one off or save a copy
/// under another name. A skill of the same name in the user's own folders replaces the built-in
/// one. Like every skill they hold instructions only; each tool they suggest still asks.
const BUILTIN_SKILLS: &[(&str, &str)] = &[
    (
        "wechat-article",
        include_str!("../skills/wechat-article/SKILL.md"),
    ),
    (
        "five-image-note",
        include_str!("../skills/five-image-note/SKILL.md"),
    ),
    (
        "short-video-pack",
        include_str!("../skills/short-video-pack/SKILL.md"),
    ),
];

/// What the user is told when they try to change or delete a built-in skill.
const BUILTIN_READ_ONLY: &str =
    "built-in skills can not be edited or deleted; turn it off, or save a copy under another name";

/// The most skills one `find_skill` call returns.
const FIND_SKILL_LIMIT: usize = 10;

/// The executable contract for a Skill, parsed from its `SKILL.md` front-matter.
///
/// Every field is optional in the file and falls back to a safe default; a
/// malformed value fails closed so a broken contract can never silently weaken
/// the Runtime / Policy boundary.
#[derive(Debug, Clone, Default)]
pub struct SkillContract {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub source: String,
    pub version: String,
    pub compatibility: String,
    pub side_effect: SideEffectClass,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub timeout_secs: Option<u64>,
    pub cancellable: bool,
    pub worker: Option<String>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
}

pub struct SkillStore {
    state_dir: PathBuf,
}

impl SkillStore {
    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, ShadowReadError> {
        std::fs::create_dir_all(state_dir.as_ref().join("skills"))?;
        Ok(Self {
            state_dir: state_dir.as_ref().to_path_buf(),
        })
    }

    fn global_dir(&self) -> PathBuf {
        self.state_dir.join("skills")
    }

    fn base(&self, scope: &str, workspace: Option<&str>) -> Result<PathBuf, ShadowReadError> {
        match scope {
            "global" => Ok(self.global_dir()),
            "project" => {
                let workspace = workspace
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| ShadowReadError::Parse("workspace is required".to_string()))?;
                let workspace = PathBuf::from(workspace)
                    .canonicalize()
                    .map_err(|error| ShadowReadError::Parse(error.to_string()))?;
                if !workspace.is_dir() {
                    return Err(ShadowReadError::Parse(
                        "workspace must be a directory".to_string(),
                    ));
                }
                Ok(workspace.join(".delta").join("skills"))
            }
            _ => Err(ShadowReadError::Parse("invalid skill scope".to_string())),
        }
    }

    pub fn list(&self, workspace: Option<&str>) -> Result<Vec<Value>, ShadowReadError> {
        let disabled = self.disabled()?;
        let mut rows = BTreeMap::new();
        let mut scopes = vec![(self.global_dir(), "global")];
        if let Some(workspace) = workspace.filter(|value| !value.is_empty()) {
            scopes.push((self.base("project", Some(workspace))?, "project"));
        }
        for (base, scope) in scopes {
            let Ok(entries) = std::fs::read_dir(&base) else {
                continue;
            };
            for entry in entries.flatten() {
                let folder = entry.path();
                if folder.is_symlink() || !folder.is_dir() {
                    continue;
                }
                let skill_md = folder.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }
                let text = std::fs::read_to_string(&skill_md)?;
                let fallback = folder
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                let contract = parse_skill(&text, &fallback)?.contract;
                let files = walk_files(&folder)?.saturating_sub(1);
                rows.insert(
                    contract.name.clone(),
                    json!({
                        "name": contract.name,
                        "description": contract.description,
                        "instructions": contract.instructions,
                        "scope": scope,
                        "source": contract.source,
                        "enabled": !disabled.contains(&contract.name),
                        "path": folder,
                        "files": files,
                        "version": contract.version,
                        "compatibility": contract.compatibility,
                        "side_effect": contract.side_effect.as_str(),
                        "capabilities": contract.capabilities,
                        "permissions": contract.permissions,
                        "timeout_secs": contract.timeout_secs,
                        "cancellable": contract.cancellable,
                        "worker": contract.worker,
                        "input_schema": contract.input_schema,
                        "output_schema": contract.output_schema,
                    }),
                );
            }
        }
        for (folder, text) in BUILTIN_SKILLS {
            let contract = parse_skill(text, folder)?.contract;
            rows.entry(contract.name.clone()).or_insert_with(|| {
                json!({
                    "name": contract.name,
                    "description": contract.description,
                    "instructions": contract.instructions,
                    "scope": "builtin",
                    "source": "builtin",
                    "enabled": !disabled.contains(&contract.name),
                    "path": Value::Null,
                    "files": 0,
                    "version": contract.version,
                    "compatibility": contract.compatibility,
                    "side_effect": contract.side_effect.as_str(),
                    "capabilities": contract.capabilities,
                    "permissions": contract.permissions,
                    "timeout_secs": contract.timeout_secs,
                    "cancellable": contract.cancellable,
                    "worker": contract.worker,
                    "input_schema": contract.input_schema,
                    "output_schema": contract.output_schema,
                })
            });
        }
        Ok(rows.into_values().collect())
    }

    /// Whether `name` is a built-in skill the user has not replaced with one of their own.
    fn is_builtin_only(&self, name: &str, workspace: Option<&str>) -> bool {
        BUILTIN_SKILLS.iter().any(|(folder, _)| *folder == name)
            && self.find(name, workspace).is_err()
    }

    pub fn create(&self, body: &Value) -> Result<Value, ShadowReadError> {
        let name = validate_name(body.get("name").and_then(Value::as_str).unwrap_or_default())?;
        let instructions = body
            .get("instructions")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if instructions.is_empty() {
            return Ok(json!({"ok": false, "error": "skill instructions are required"}));
        }
        let scope = body
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("global");
        let base = self.base(scope, body.get("workspace").and_then(Value::as_str))?;
        let folder = base.join(&name);
        if folder.exists() {
            return Ok(json!({"ok": false, "error": "skill already exists"}));
        }
        std::fs::create_dir_all(&folder)?;
        write_skill(
            &folder,
            &name,
            body.get("description")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            instructions,
            "local",
        )?;
        Ok(json!({"ok": true, "name": name, "scope": scope}))
    }

    pub fn update(
        &self,
        name: &str,
        patch: &Value,
        workspace: Option<&str>,
    ) -> Result<Value, ShadowReadError> {
        let name = validate_name(name)?;
        if let Some(enabled) = patch.get("enabled").and_then(Value::as_bool) {
            self.set_enabled(&name, enabled)?;
        }
        if (patch.get("description").is_some() || patch.get("instructions").is_some())
            && self.is_builtin_only(&name, workspace)
        {
            return Ok(json!({"ok": false, "error": BUILTIN_READ_ONLY}));
        }
        if patch.get("description").is_some() || patch.get("instructions").is_some() {
            let (folder, _) = self.find(&name, workspace)?;
            let current =
                parse_skill(&std::fs::read_to_string(folder.join("SKILL.md"))?, &name)?.contract;
            let instructions = patch
                .get("instructions")
                .and_then(Value::as_str)
                .unwrap_or(&current.instructions);
            if instructions.trim().is_empty() {
                return Ok(json!({"ok": false, "error": "skill instructions are required"}));
            }
            write_skill(
                &folder,
                &name,
                patch
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(&current.description),
                instructions,
                &current.source,
            )?;
        }
        Ok(json!({"ok": true}))
    }

    pub fn delete(&self, name: &str, workspace: Option<&str>) -> Result<Value, ShadowReadError> {
        if self.is_builtin_only(name, workspace) {
            return Ok(json!({"ok": false, "error": BUILTIN_READ_ONLY}));
        }
        let (folder, _) = self.find(&validate_name(name)?, workspace)?;
        std::fs::remove_dir_all(folder)?;
        Ok(json!({"ok": true}))
    }

    pub fn move_skill(
        &self,
        name: &str,
        scope: &str,
        workspace: Option<&str>,
    ) -> Result<Value, ShadowReadError> {
        let name = validate_name(name)?;
        let (source, current_scope) = self.find(&name, workspace)?;
        if current_scope == scope {
            return Ok(json!({"ok": true, "scope": scope}));
        }
        let destination = self.base(scope, workspace)?.join(&name);
        if destination.exists() {
            return Ok(json!({"ok": false, "error": "skill exists in target scope"}));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(source, destination)?;
        Ok(json!({"ok": true, "scope": scope}))
    }

    pub fn resolve_folder(
        &self,
        name: &str,
        workspace: Option<&str>,
    ) -> Result<PathBuf, ShadowReadError> {
        self.find(&validate_name(name)?, workspace)
            .map(|(folder, _)| folder)
    }

    pub fn stage_upload(&self, encoded: &str, filename: &str) -> Result<Value, ShadowReadError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| ShadowReadError::Parse(error.to_string()))?;
        if bytes.len() > 25 * 1024 * 1024 {
            return Err(ShadowReadError::Parse(
                "skill upload exceeds the 25 MiB limit".to_string(),
            ));
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        let staged = self.state_dir.join("skills-staged").join(&token);
        std::fs::create_dir_all(&staged)?;
        if filename.to_ascii_lowercase().ends_with(".zip") {
            if let Err(error) = extract_zip(&bytes, &staged) {
                // Nothing refers to a staging folder that never got its skill; do not leave the
                // partly extracted files of a refused archive behind.
                let _ = std::fs::remove_dir_all(&staged);
                return Err(error);
            }
        } else {
            std::fs::write(staged.join("SKILL.md"), bytes)?;
        }
        let skill_path = find_skill_md(&staged)?;
        if skill_path.parent() != Some(staged.as_path()) {
            let root = skill_path.parent().unwrap();
            for entry in std::fs::read_dir(root)?.flatten() {
                std::fs::rename(entry.path(), staged.join(entry.file_name()))?;
            }
        }
        let parsed = parse_skill(&std::fs::read_to_string(staged.join("SKILL.md"))?, "")?.contract;
        let name = validate_name(&parsed.name)?;
        let files = list_relative_files(&staged)?;
        Ok(json!({
            "ok": true, "token": token, "name": name,
            "description": parsed.description, "instructions": parsed.instructions,
            "files": files.into_iter().filter(|path| path != "SKILL.md").collect::<Vec<_>>(),
        }))
    }

    pub fn confirm_upload(
        &self,
        token: &str,
        scope: &str,
        workspace: Option<&str>,
    ) -> Result<Value, ShadowReadError> {
        let token = validate_name(token)?;
        let staged = self.state_dir.join("skills-staged").join(token);
        let parsed = parse_skill(&std::fs::read_to_string(staged.join("SKILL.md"))?, "")?.contract;
        let name = validate_name(&parsed.name)?;
        let destination = self.base(scope, workspace)?.join(&name);
        if destination.exists() {
            return Ok(json!({"ok": false, "error": "skill already exists"}));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(staged, destination)?;
        Ok(json!({"ok": true, "name": name, "scope": scope}))
    }

    pub fn session_rows(
        &self,
        session_id: &str,
        workspace: Option<&str>,
    ) -> Result<Vec<Value>, ShadowReadError> {
        let overrides = self.session_overrides()?;
        let session = overrides.get(session_id).and_then(Value::as_object);
        Ok(self
            .list(workspace)?
            .into_iter()
            .map(|row| {
                let name = row["name"].as_str().unwrap_or_default();
                json!({
                    "name": name,
                    "description": row["description"],
                    "scope": row["scope"],
                    "side_effect": row["side_effect"],
                    "enabled": session.and_then(|value| value.get(name)).and_then(Value::as_bool).unwrap_or_else(|| row["enabled"].as_bool().unwrap_or(true)),
                })
            })
            .collect())
    }

    /// The enabled skills of a session that match `query`, best match first, at most
    /// [`FIND_SKILL_LIMIT`] of them. Only names and descriptions come back, so a long list of skills
    /// never costs the model more than ten lines (A6, progressive disclosure).
    pub fn find_for_session(
        &self,
        session_id: &str,
        workspace: Option<&str>,
        query: &str,
    ) -> Result<Value, ShadowReadError> {
        let words: Vec<String> = query
            .split_whitespace()
            .map(|word| word.to_lowercase())
            .collect();
        let mut matches: Vec<(usize, String, Value)> = Vec::new();
        for row in self.session_rows(session_id, workspace)? {
            if row["enabled"].as_bool() != Some(true) {
                continue;
            }
            let name = row["name"].as_str().unwrap_or_default().to_string();
            let text = format!(
                "{} {}",
                name.to_lowercase(),
                row["description"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
            );
            let score = words
                .iter()
                .filter(|word| text.contains(word.as_str()))
                .count();
            if words.is_empty() || score > 0 {
                matches.push((
                    score,
                    name.clone(),
                    json!({"name": name, "description": row["description"]}),
                ));
            }
        }
        matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        let total = matches.len();
        let skills: Vec<Value> = matches
            .into_iter()
            .take(FIND_SKILL_LIMIT)
            .map(|(_, _, skill)| skill)
            .collect();
        Ok(json!({"skills": skills, "total": total, "truncated": total > FIND_SKILL_LIMIT}))
    }

    /// The full text of one enabled skill of a session. A skill the session has turned off is
    /// not available, exactly as when the user picks it.
    pub fn read_for_session(
        &self,
        session_id: &str,
        workspace: Option<&str>,
        name: &str,
    ) -> Result<Value, ShadowReadError> {
        let is_enabled = self.session_rows(session_id, workspace)?.iter().any(|row| {
            row["name"].as_str() == Some(name) && row["enabled"].as_bool() == Some(true)
        });
        if !is_enabled {
            return Ok(
                json!({"ok": false, "error": format!("skill is unavailable for this session: {name}")}),
            );
        }
        let Some(row) = self
            .list(workspace)?
            .into_iter()
            .find(|row| row["name"].as_str() == Some(name))
        else {
            return Ok(
                json!({"ok": false, "error": format!("skill is unavailable for this session: {name}")}),
            );
        };
        Ok(json!({
            "ok": true, "name": name, "description": row["description"],
            "instructions": row["instructions"],
            "note": "A skill has no permissions of its own: every tool it suggests still goes through the usual approval.",
        }))
    }

    pub fn set_session(
        &self,
        session_id: &str,
        skill: &str,
        is_enabled: bool,
        should_clear: bool,
        workspace: Option<&str>,
    ) -> Result<Value, ShadowReadError> {
        let clear = should_clear;
        let enabled = is_enabled;
        validate_name(skill)?;
        let mut overrides = self.session_overrides()?;
        let session = overrides
            .entry(session_id.to_string())
            .or_insert_with(|| json!({}));
        let session = session.as_object_mut().unwrap();
        if clear {
            session.remove(skill);
        } else {
            session.insert(skill.to_string(), Value::Bool(enabled));
        }
        let bytes = serde_json::to_vec_pretty(&overrides)?;
        atomic_write(self.state_dir.join("session-skills.json"), &bytes)?;
        Ok(json!({"ok": true, "skills": self.session_rows(session_id, workspace)?}))
    }

    fn find(
        &self,
        name: &str,
        workspace: Option<&str>,
    ) -> Result<(PathBuf, &'static str), ShadowReadError> {
        if let Some(workspace) = workspace.filter(|value| !value.is_empty()) {
            let project = self.base("project", Some(workspace))?.join(name);
            if project.join("SKILL.md").is_file() && !project.is_symlink() {
                return Ok((project, "project"));
            }
        }
        let global = self.global_dir().join(name);
        if global.join("SKILL.md").is_file() && !global.is_symlink() {
            return Ok((global, "global"));
        }
        Err(ShadowReadError::Parse("skill not found".to_string()))
    }

    fn disabled(&self) -> Result<HashSet<String>, ShadowReadError> {
        let path = self.state_dir.join("skills-settings.json");
        let Some(value) = read_optional_json(&path)? else {
            return Ok(HashSet::new());
        };
        let root = value.as_object().ok_or_else(|| {
            ShadowReadError::Parse("skill settings root must be an object".to_string())
        })?;
        let disabled = root
            .get("disabled")
            .ok_or_else(|| {
                ShadowReadError::Parse("skill settings are missing disabled".to_string())
            })?
            .as_array()
            .ok_or_else(|| {
                ShadowReadError::Parse("skill settings disabled must be an array".to_string())
            })?;
        let mut names = HashSet::new();
        for value in disabled {
            let name = value.as_str().ok_or_else(|| {
                ShadowReadError::Parse(
                    "skill settings disabled entries must be strings".to_string(),
                )
            })?;
            names.insert(validate_name(name)?);
        }
        Ok(names)
    }

    fn set_enabled(&self, name: &str, is_enabled: bool) -> Result<(), ShadowReadError> {
        let enabled = is_enabled;
        let mut disabled = self.disabled()?;
        if enabled {
            disabled.remove(name);
        } else {
            disabled.insert(name.to_string());
        }
        let mut disabled = disabled.into_iter().collect::<Vec<_>>();
        disabled.sort();
        let bytes = serde_json::to_vec_pretty(&json!({"disabled": disabled}))?;
        atomic_write(self.state_dir.join("skills-settings.json"), &bytes)?;
        Ok(())
    }

    fn session_overrides(&self) -> Result<serde_json::Map<String, Value>, ShadowReadError> {
        let path = self.state_dir.join("session-skills.json");
        let Some(value) = read_optional_json(&path)? else {
            return Ok(serde_json::Map::new());
        };
        let overrides = value.as_object().cloned().ok_or_else(|| {
            ShadowReadError::Parse("session skill state root must be an object".to_string())
        })?;
        for (session_id, skills) in &overrides {
            if session_id.trim().is_empty() {
                return Err(ShadowReadError::Parse(
                    "session skill state contains an empty session id".to_string(),
                ));
            }
            let skills = skills.as_object().ok_or_else(|| {
                ShadowReadError::Parse("session skill state entries must be objects".to_string())
            })?;
            for (skill, enabled) in skills {
                validate_name(skill)?;
                if !enabled.is_boolean() {
                    return Err(ShadowReadError::Parse(
                        "session skill state flags must be booleans".to_string(),
                    ));
                }
            }
        }
        Ok(overrides)
    }
}

fn read_optional_json(path: &Path) -> Result<Option<Value>, ShadowReadError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(serde_json::from_str::<Value>(&text)?)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

struct ParsedSkill {
    contract: SkillContract,
}

fn parse_skill(text: &str, fallback: &str) -> Result<ParsedSkill, ShadowReadError> {
    Ok(ParsedSkill {
        contract: parse_contract(text, fallback)?,
    })
}

/// Parse and validate a Skill contract from `SKILL.md` text.
///
/// A malformed contract (unknown side-effect class, non-numeric timeout,
/// invalid JSON schema, bad worker binding) fails closed — the Skill is not
/// surfaced to the Runtime / Policy layer.
fn parse_contract(text: &str, fallback: &str) -> Result<SkillContract, ShadowReadError> {
    let mut contract = SkillContract {
        name: fallback.to_string(),
        cancellable: true,
        ..SkillContract::default()
    };
    contract.instructions = text.trim().to_string();

    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let key = key.trim();
                let value = value.trim();
                if value.is_empty() {
                    continue;
                }
                match key {
                    "name" => contract.name = value.to_string(),
                    "description" => contract.description = value.to_string(),
                    "source" => contract.source = value.to_string(),
                    "version" => contract.version = value.to_string(),
                    "compatibility" => contract.compatibility = value.to_string(),
                    "side_effect" => {
                        contract.side_effect = SideEffectClass::parse(value).ok_or_else(|| {
                            ShadowReadError::Parse(format!(
                                "skill declares an unknown side_effect class: {value}"
                            ))
                        })?;
                    }
                    "capabilities" => contract.capabilities = parse_string_list(value)?,
                    "permissions" => contract.permissions = parse_string_list(value)?,
                    "timeout_secs" => {
                        contract.timeout_secs = Some(value.parse::<u64>().map_err(|_| {
                            ShadowReadError::Parse(format!(
                                "skill timeout_secs must be a non-negative integer: {value}"
                            ))
                        })?);
                    }
                    "cancellable" => {
                        contract.cancellable = parse_bool(value).ok_or_else(|| {
                            ShadowReadError::Parse(format!(
                                "skill cancellable must be true or false: {value}"
                            ))
                        })?;
                    }
                    "worker" => {
                        validate_name(value)?;
                        contract.worker = Some(value.to_string());
                    }
                    "input_schema" => contract.input_schema = Some(parse_json_value(value)?),
                    "output_schema" => contract.output_schema = Some(parse_json_value(value)?),
                    _ => {}
                }
            }
            contract.instructions = rest[end + 4..].trim().to_string();
        }
    }
    Ok(contract)
}

/// Parse a string list that is either a JSON array or a comma-separated list.
fn parse_string_list(value: &str) -> Result<Vec<String>, ShadowReadError> {
    let trimmed = value.trim();
    if trimmed.starts_with('[') {
        let parsed: Vec<String> = serde_json::from_str(trimmed).map_err(|_| {
            ShadowReadError::Parse(format!("skill field must be a JSON string array: {value}"))
        })?;
        return Ok(parsed);
    }
    Ok(trimmed
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect())
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Parse a single-line JSON value (for input/output schemas).
fn parse_json_value(value: &str) -> Result<Value, ShadowReadError> {
    serde_json::from_str(value.trim()).map_err(|_| {
        ShadowReadError::Parse(format!("skill schema field is not valid JSON: {value}"))
    })
}

/// The `save_skill` tool: the model proposes a reusable method it has just worked out, the user
/// reviews the full text in the approval card, and only on approval is it written. The skill is
/// written with no permissions and no tools of its own, so saving one can never widen what the
/// agent may do (blueprint 4.7: methods can evolve, permissions cannot).
pub fn save_registration(store: Arc<SkillStore>) -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "skill.save".to_owned(),
        tool_name: "save_skill".to_owned(),
        side_effect: crate::skills::SideEffectClass::LocalWrite,
        description: "Save a reusable method as a skill for the user's skills list, after the user \
            approves it. Use it only when the user asks to keep how a task was done. Write general \
            steps that work for the next similar task; leave out names, numbers, paths and any other \
            private details from this one."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["name", "description", "instructions"],
            "properties": {
                "name": {"type": "string", "description": "Short name: letters, digits, '-' and '_' only (for example weekly-sales-report)."},
                "description": {"type": "string", "description": "One sentence on what the skill does and when to use it."},
                "instructions": {"type": "string", "description": "The method, as numbered steps written for another run of the agent."}
            }
        }),
        metadata: json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "filesystem", "capabilities": ["skill.save"]
        }),
        grants: CapabilityGrants::default(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let text = |key: &str| job.arguments.get(key).and_then(Value::as_str).unwrap_or("");
            let body = json!({
                "name": text("name"), "description": text("description"),
                "instructions": text("instructions"), "scope": "global",
            });
            match store.create(&body) {
                Ok(result) if result.get("ok").and_then(Value::as_bool) == Some(true) => {
                    CapabilityResult::completed(&job.job_id, result)
                }
                Ok(result) => {
                    let reason = result.get("error").and_then(Value::as_str).unwrap_or("could not save the skill");
                    CapabilityResult::failed(&job.job_id, reason, Some("skill_rejected"))
                }
                Err(error) => CapabilityResult::failed(&job.job_id, &error.to_string(), Some("skill_rejected")),
            }
        })),
    }
}

/// Two read-only tools that let the model consult skills on demand (A6, progressive disclosure).
/// The prompt carries these two descriptions and nothing else about skills, so its size does not
/// grow with the number of skills; the full text is read only after a skill is chosen.
pub fn lookup_registrations(store: Arc<SkillStore>) -> Vec<CapabilityRegistration> {
    let metadata = |capability: &str| {
        json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": [capability]
        })
    };
    let find_store = store.clone();
    let find = CapabilityRegistration {
        capability_id: "skill.find".to_owned(),
        tool_name: "find_skill".to_owned(),
        side_effect: SideEffectClass::Read,
        description: "Look up the user's saved skills by keywords. Returns up to ten names with a \
            one-line description. Use it when a task may match a method the user saved before."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["query"],
            "properties": {"query": {"type": "string", "description": "Keywords for what the task is about."}}
        }),
        metadata: metadata("skill.find"),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let query = job
                .arguments
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("");
            match find_store.find_for_session(
                job.session_id.as_deref().unwrap_or_default(),
                job.workspace.as_deref(),
                query,
            ) {
                Ok(result) => CapabilityResult::completed(&job.job_id, result),
                Err(error) => {
                    CapabilityResult::failed(&job.job_id, &error.to_string(), Some("skill_lookup"))
                }
            }
        })),
    };
    let read = CapabilityRegistration {
        capability_id: "skill.read".to_owned(),
        tool_name: "read_skill".to_owned(),
        side_effect: SideEffectClass::Read,
        description: "Read the full steps of one saved skill by name, after find_skill found it. \
            Follow the steps as guidance; the tools they call still need the usual approval."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["name"],
            "properties": {"name": {"type": "string", "description": "A skill name returned by find_skill."}}
        }),
        metadata: metadata("skill.read"),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(move |job, control| {
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let name = job
                .arguments
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("");
            match store.read_for_session(
                job.session_id.as_deref().unwrap_or_default(),
                job.workspace.as_deref(),
                name,
            ) {
                Ok(result) if result.get("ok").and_then(Value::as_bool) == Some(true) => {
                    CapabilityResult::completed(&job.job_id, result)
                }
                Ok(result) => {
                    let reason = result
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("skill is unavailable");
                    CapabilityResult::failed(&job.job_id, reason, Some("skill_unavailable"))
                }
                Err(error) => {
                    CapabilityResult::failed(&job.job_id, &error.to_string(), Some("skill_lookup"))
                }
            }
        })),
    };
    vec![find, read]
}

fn validate_name(name: &str) -> Result<String, ShadowReadError> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 64
        || name.contains("..")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(ShadowReadError::Parse("invalid skill name".to_string()));
    }
    Ok(name.to_string())
}

fn write_skill(
    folder: &Path,
    name: &str,
    description: &str,
    instructions: &str,
    source: &str,
) -> Result<(), ShadowReadError> {
    let text = format!(
        "---\nname: {name}\ndescription: {}\nsource: {source}\n---\n\n{}\n",
        description.replace(['\r', '\n'], " "),
        instructions.trim()
    );
    atomic_write(folder.join("SKILL.md"), text.as_bytes())?;
    Ok(())
}

/// Most bytes a skill archive may expand to.
const MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024;

fn extract_zip(bytes: &[u8], destination: &Path) -> Result<(), ShadowReadError> {
    extract_zip_within(bytes, destination, MAX_EXTRACTED_BYTES)
}

fn extract_zip_within(
    bytes: &[u8],
    destination: &Path,
    max_extracted: u64,
) -> Result<(), ShadowReadError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| ShadowReadError::Parse(error.to_string()))?;
    if archive.len() > 500 {
        return Err(ShadowReadError::Parse(
            "skill archive contains too many entries".to_string(),
        ));
    }
    let mut extracted_size = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ShadowReadError::Parse(error.to_string()))?;
        let Some(path) = entry.enclosed_name() else {
            return Err(ShadowReadError::Parse(
                "unsafe skill archive path".to_string(),
            ));
        };
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(ShadowReadError::Parse(
                "unsafe skill archive path".to_string(),
            ));
        }
        let output = destination.join(path);
        if entry.is_dir() {
            std::fs::create_dir_all(output)?;
            continue;
        }
        // The size in the archive's headers is the author's claim; refuse early on a claim that is
        // already too big, but count what the entry really expands to as it is written.
        if extracted_size.saturating_add(entry.size()) > max_extracted {
            return Err(extracted_limit_error());
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let room = max_extracted - extracted_size;
        let mut file = std::fs::File::create(&output)?;
        let written = std::io::copy(&mut (&mut entry).take(room.saturating_add(1)), &mut file)?;
        extracted_size = extracted_size.saturating_add(written);
        if extracted_size > max_extracted {
            drop(file);
            let _ = std::fs::remove_file(&output);
            return Err(extracted_limit_error());
        }
    }
    Ok(())
}

fn extracted_limit_error() -> ShadowReadError {
    ShadowReadError::Parse("skill archive exceeds the 100 MiB extracted limit".to_string())
}

fn find_skill_md(root: &Path) -> Result<PathBuf, ShadowReadError> {
    let candidates = list_files(root)?
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "SKILL.md"))
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(ShadowReadError::Parse(
            "upload must contain exactly one SKILL.md".to_string(),
        ));
    }
    Ok(candidates[0].clone())
}

fn list_files(root: &Path) -> Result<Vec<PathBuf>, ShadowReadError> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)?.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn walk_files(root: &Path) -> Result<usize, ShadowReadError> {
    Ok(list_files(root)?.len())
}

fn list_relative_files(root: &Path) -> Result<Vec<String>, ShadowReadError> {
    Ok(list_files(root)?
        .into_iter()
        .filter_map(|path| {
            path.strip_prefix(root)
                .ok()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-entry deflate archive of `size` zero bytes whose central directory claims the entry
    /// is `declared` bytes long. The declared size is the archive author's word, nothing more.
    fn archive_declaring(size: usize, declared: u32) -> Vec<u8> {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("SKILL.md", options).unwrap();
        writer.write_all(&vec![0_u8; size]).unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        let record = bytes
            .windows(4)
            .position(|window| window == b"PK\x01\x02")
            .expect("central directory record");
        // Uncompressed size: 24 bytes into the central directory file header.
        bytes[record + 24..record + 28].copy_from_slice(&declared.to_le_bytes());
        bytes
    }

    #[test]
    fn an_archive_is_held_to_its_real_size_not_the_size_it_declares() {
        let temp = tempfile::tempdir().unwrap();
        let bomb = archive_declaring(4 * 1024 * 1024, 10);
        let error = extract_zip_within(&bomb, temp.path(), 1024 * 1024)
            .expect_err("4 MiB of real content must not pass a 1 MiB limit");
        assert!(error.to_string().contains("extracted limit"), "{error}");
        let written = std::fs::metadata(temp.path().join("SKILL.md"))
            .map(|meta| meta.len())
            .unwrap_or(0);
        assert!(
            written <= 1024 * 1024 + 1,
            "{written} bytes reached the disk"
        );
    }

    #[test]
    fn a_refused_archive_leaves_no_staging_folder_behind() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(b"this is not a zip archive");
        assert!(store.stage_upload(&encoded, "broken.zip").is_err());
        let staged = temp.path().join("skills-staged");
        let leftover = std::fs::read_dir(&staged)
            .map(|dir| dir.count())
            .unwrap_or(0);
        assert_eq!(leftover, 0, "a failed upload kept its staging folder");
    }

    #[test]
    fn an_honest_archive_within_the_limit_is_extracted() {
        let temp = tempfile::tempdir().unwrap();
        let archive = archive_declaring(2048, 2048);
        extract_zip_within(&archive, temp.path(), 1024 * 1024).unwrap();
        assert_eq!(
            std::fs::metadata(temp.path().join("SKILL.md"))
                .unwrap()
                .len(),
            2048
        );
    }

    #[test]
    fn skill_crud_and_session_override_are_rust_owned() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        assert_eq!(
            store
                .create(&json!({"name": "brief", "description": "d", "instructions": "Do it"}))
                .unwrap()["ok"],
            true
        );
        assert_eq!(store.list(None).unwrap()[0]["name"], "brief");
        assert_eq!(
            store
                .set_session("s1", "brief", false, false, None)
                .unwrap()["skills"][0]["enabled"],
            false
        );
        assert_eq!(store.delete("brief", None).unwrap()["ok"], true);
    }

    #[test]
    fn save_skill_writes_a_permissionless_skill_and_refuses_bad_or_duplicate_ones() {
        use crate::capability::CapabilityHost;
        use crate::runtime::{ToolCall, ToolExecutionContext, ToolExecutor, ToolExitState};
        use std::sync::atomic::AtomicBool;
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(SkillStore::open(temp.path()).unwrap());
        let host = CapabilityHost::new();
        host.register(save_registration(store.clone())).unwrap();
        let context = ToolExecutionContext {
            session_id: "s".to_string(),
            run_id: "r".to_string(),
            workspace: None,
            timeout: std::time::Duration::from_secs(2),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: Default::default(),
            proxy: None,
        };
        let call = |name: &str, instructions: &str| ToolCall {
            id: "c".to_string(),
            name: "save_skill".to_string(),
            arguments: json!({"name": name, "description": "Weekly report", "instructions": instructions}),
        };
        let saved = host.execute(
            &call("weekly-report", "1. Read the sheet.\n2. Write the summary."),
            &context,
        );
        assert_eq!(saved.state, ToolExitState::Completed, "{:?}", saved.error);
        let own = |store: &SkillStore| -> Vec<Value> {
            store
                .list(None)
                .unwrap()
                .into_iter()
                .filter(|row| row["scope"] != "builtin")
                .collect()
        };
        let listed = own(&store);
        assert_eq!(listed[0]["name"], "weekly-report");
        // Saving never grants anything: no permissions, no side effects declared.
        assert_eq!(listed[0]["permissions"], json!([]));
        for (name, instructions) in [
            ("weekly-report", "x"),
            ("../escape", "x"),
            ("ok-name", "  "),
        ] {
            let refused = host.execute(&call(name, instructions), &context);
            assert_eq!(refused.state, ToolExitState::Failed, "{name}");
        }
        assert_eq!(own(&store).len(), 1);
    }

    fn lookup_host(store: &Arc<SkillStore>) -> crate::capability::CapabilityHost {
        let host = crate::capability::CapabilityHost::new();
        for registration in lookup_registrations(store.clone()) {
            host.register(registration).unwrap();
        }
        host
    }

    fn lookup_call(
        host: &crate::capability::CapabilityHost,
        session: &str,
        tool: &str,
        arguments: Value,
    ) -> crate::runtime::ToolResult {
        use crate::runtime::{ToolCall, ToolExecutionContext, ToolExecutor};
        use std::sync::atomic::AtomicBool;
        let context = ToolExecutionContext {
            session_id: session.to_string(),
            run_id: "r".to_string(),
            workspace: None,
            timeout: std::time::Duration::from_secs(2),
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(|_| {}),
            secrets: Default::default(),
            proxy: None,
        };
        host.execute(
            &ToolCall {
                id: "c".to_string(),
                name: tool.to_string(),
                arguments,
            },
            &context,
        )
    }

    #[test]
    fn find_skill_returns_enabled_matches_best_first_and_at_most_ten() {
        use crate::runtime::ToolExitState;
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(SkillStore::open(temp.path()).unwrap());
        for index in 0..12 {
            store
                .create(&json!({"name": format!("report-{index:02}"), "description": "Weekly sales report", "instructions": "Do it"}))
                .unwrap();
        }
        for (name, description) in [
            ("invoice", "Make an invoice from a report"),
            ("minutes", "Meeting minutes"),
        ] {
            store
                .create(&json!({"name": name, "description": description, "instructions": "Do it"}))
                .unwrap();
        }
        let host = lookup_host(&store);

        let sales = lookup_call(&host, "s1", "find_skill", json!({"query": "sales report"}));
        assert_eq!(sales.state, ToolExitState::Completed, "{:?}", sales.error);
        // 12 + invoice; the built-in data-analysis skill was removed with the R engine (ADR-0081).
        assert_eq!(sales.output["total"], 13);
        assert_eq!(sales.output["truncated"], true);
        let names: Vec<&str> = sales.output["skills"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|skill| skill["name"].as_str())
            .collect();
        assert_eq!(names.len(), 10);
        assert_eq!(names[0], "report-00");
        assert!(
            !names.contains(&"invoice"),
            "a one-word match ranks below a two-word match"
        );

        // A skill the session turned off cannot be found there, but still can in another session.
        store
            .set_session("s1", "minutes", false, false, None)
            .unwrap();
        let hidden = lookup_call(&host, "s1", "find_skill", json!({"query": "meeting"}));
        assert_eq!(hidden.output["total"], 0);
        let visible = lookup_call(&host, "s2", "find_skill", json!({"query": "meeting"}));
        assert_eq!(visible.output["skills"][0]["name"], "minutes");
        assert!(visible.output["skills"][0].get("instructions").is_none());
    }

    #[test]
    fn read_skill_returns_the_full_steps_of_an_enabled_skill_only() {
        use crate::runtime::ToolExitState;
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(SkillStore::open(temp.path()).unwrap());
        store
            .create(&json!({"name": "brief", "description": "Write a brief", "instructions": "1. Read.\n2. Write."}))
            .unwrap();
        let host = lookup_host(&store);

        let read = lookup_call(&host, "s1", "read_skill", json!({"name": "brief"}));
        assert_eq!(read.state, ToolExitState::Completed, "{:?}", read.error);
        assert_eq!(read.output["instructions"], "1. Read.\n2. Write.");
        assert!(read.output["note"]
            .as_str()
            .unwrap()
            .contains("no permissions"));

        store
            .set_session("s1", "brief", false, false, None)
            .unwrap();
        let off = lookup_call(&host, "s1", "read_skill", json!({"name": "brief"}));
        assert_eq!(off.state, ToolExitState::Failed);
        assert!(off.error.as_deref().unwrap().contains("unavailable"));
        let unknown = lookup_call(&host, "s2", "read_skill", json!({"name": "nope"}));
        assert_eq!(unknown.state, ToolExitState::Failed);
    }

    #[test]
    fn the_skill_tools_cost_the_same_prompt_space_for_any_number_of_skills() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(SkillStore::open(temp.path()).unwrap());
        let host = lookup_host(&store);
        let before = host.tool_schemas().to_string();
        for index in 0..100 {
            store
                .create(&json!({"name": format!("skill-{index:03}"), "description": "A saved method", "instructions": "Do it"}))
                .unwrap();
        }
        assert_eq!(host.tool_schemas().to_string(), before);
        let schemas = host.tool_schemas();
        let tools = schemas.as_array().unwrap();
        assert_eq!(tools.len(), 2);
        for tool in tools {
            assert_eq!(tool["function"]["metadata"]["side_effect"], "read");
        }
    }

    #[test]
    fn corrupt_skill_settings_fail_closed_without_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        store
            .create(&json!({"name": "brief", "instructions": "Do it"}))
            .unwrap();
        let path = temp.path().join("skills-settings.json");
        std::fs::write(&path, b"{not-json").unwrap();

        assert!(matches!(store.list(None), Err(ShadowReadError::Json(_))));
        assert!(matches!(
            store.update("brief", &json!({"enabled": false}), None),
            Err(ShadowReadError::Json(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"{not-json");
    }

    #[test]
    fn invalid_skill_settings_shape_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        std::fs::write(
            temp.path().join("skills-settings.json"),
            serde_json::to_vec(&json!({"disabled": "brief"})).unwrap(),
        )
        .unwrap();
        assert!(matches!(store.list(None), Err(ShadowReadError::Parse(_))));
    }

    #[test]
    fn corrupt_session_skill_state_fails_closed_without_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        store
            .create(&json!({"name": "brief", "instructions": "Do it"}))
            .unwrap();
        let path = temp.path().join("session-skills.json");
        std::fs::write(&path, b"{not-json").unwrap();

        assert!(matches!(
            store.session_rows("s1", None),
            Err(ShadowReadError::Json(_))
        ));
        assert!(matches!(
            store.set_session("s1", "brief", false, false, None),
            Err(ShadowReadError::Json(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"{not-json");
    }

    #[test]
    fn invalid_session_skill_shape_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        std::fs::write(
            temp.path().join("session-skills.json"),
            serde_json::to_vec(&json!({"s1": {"brief": "disabled"}})).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.session_rows("s1", None),
            Err(ShadowReadError::Parse(_))
        ));
    }

    // ------------------------------------------------------------------
    // R7 Task 5: Skill executable contract.
    // ------------------------------------------------------------------

    #[test]
    fn skill_contract_parses_full_front_matter() {
        let contract = parse_contract(
            "---\nname: brief\nversion: 1.2.0\ncompatibility: delta>=1.0\nside_effect: local_write\ncapabilities: file.read,file.write\npermissions: workspace.read\nworkspace.write\ntimeout_secs: 30\ncancellable: false\nworker: office\ninput_schema: {\"type\":\"object\"}\n---\nDo the summary.",
            "fallback",
        )
        .unwrap();
        assert_eq!(contract.name, "brief");
        assert_eq!(contract.version, "1.2.0");
        assert_eq!(contract.compatibility, "delta>=1.0");
        assert_eq!(contract.side_effect, SideEffectClass::LocalWrite);
        assert_eq!(contract.capabilities, vec!["file.read", "file.write"]);
        assert_eq!(contract.timeout_secs, Some(30));
        assert!(!contract.cancellable);
        assert_eq!(contract.worker.as_deref(), Some("office"));
        assert_eq!(contract.input_schema, Some(json!({"type": "object"})));
        assert_eq!(contract.instructions, "Do the summary.");
    }

    #[test]
    fn skill_contract_defaults_are_safe_when_fields_absent() {
        let contract = parse_contract("Do it", "fallback").unwrap();
        assert_eq!(contract.name, "fallback");
        assert_eq!(contract.side_effect, SideEffectClass::None);
        assert!(contract.capabilities.is_empty());
        assert!(contract.permissions.is_empty());
        assert_eq!(contract.timeout_secs, None);
        assert!(contract.cancellable);
        assert_eq!(contract.worker, None);
        assert_eq!(contract.input_schema, None);
    }

    #[test]
    fn skill_contract_unknown_side_effect_fails_closed() {
        assert!(matches!(
            parse_contract("---\nname: bad\nside_effect: explodes\n---\nDo it", "bad"),
            Err(ShadowReadError::Parse(_))
        ));
    }

    #[test]
    fn skill_contract_non_numeric_timeout_fails_closed() {
        assert!(matches!(
            parse_contract("---\nname: bad\ntimeout_secs: soon\n---\nDo it", "bad"),
            Err(ShadowReadError::Parse(_))
        ));
    }

    #[test]
    fn skill_contract_invalid_schema_fails_closed() {
        assert!(matches!(
            parse_contract("---\nname: bad\ninput_schema: {not json\n---\nDo it", "bad"),
            Err(ShadowReadError::Parse(_))
        ));
    }

    #[test]
    fn skill_contract_unknown_worker_name_fails_closed() {
        assert!(matches!(
            parse_contract("---\nname: bad\nworker: ../evil\n---\nDo it", "bad"),
            Err(ShadowReadError::Parse(_))
        ));
    }

    #[test]
    fn skill_contract_json_array_capabilities() {
        let contract = parse_contract(
            "---\nname: brief\ncapabilities: [\"file.read\",\"search\"]\n---\nDo it",
            "fallback",
        )
        .unwrap();
        assert_eq!(contract.capabilities, vec!["file.read", "search"]);
    }

    #[test]
    fn skill_list_exposes_contract_and_fails_closed_on_bad_contract() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        store
            .create(&json!({"name": "brief", "description": "d", "instructions": "Do it"}))
            .unwrap();
        // Overwrite with a full contract.
        std::fs::write(
            temp.path().join("skills").join("brief").join("SKILL.md"),
            "---\nname: brief\ndescription: d\nsource: local\nside_effect: external_write\ntimeout_secs: 10\n---\nDo it",
        )
        .unwrap();
        let rows = store.list(None).unwrap();
        assert_eq!(rows[0]["side_effect"], "external_write");
        assert_eq!(rows[0]["timeout_secs"], 10);

        // A corrupt contract must fail closed, not be silently skipped.
        std::fs::write(
            temp.path().join("skills").join("brief").join("SKILL.md"),
            "---\nname: brief\nside_effect: bogus\n---\nDo it",
        )
        .unwrap();
        assert!(matches!(store.list(None), Err(ShadowReadError::Parse(_))));
    }

    // --- built-in skills (B3) ---

    fn builtin_rows(store: &SkillStore) -> Vec<Value> {
        store
            .list(None)
            .unwrap()
            .into_iter()
            .filter(|row| row["scope"] == "builtin")
            .collect()
    }

    #[test]
    fn the_built_in_skills_ship_with_delta_and_hold_instructions_only() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        let rows = builtin_rows(&store);
        let names: Vec<&str> = rows
            .iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["five-image-note", "short-video-pack", "wechat-article"]
        );
        for row in &rows {
            let name = row["name"].as_str().unwrap();
            assert!(validate_name(name).is_ok(), "{name}");
            assert_eq!(row["source"], "builtin");
            assert_eq!(
                row["enabled"], true,
                "{name} is on until the user turns it off"
            );
            assert!(!row["description"].as_str().unwrap().is_empty(), "{name}");
            assert_eq!(row["side_effect"], "local_write", "{name}");
            assert_eq!(
                row["permissions"],
                json!([]),
                "{name} has no permissions of its own"
            );
            assert!(row["worker"].is_null(), "{name} runs no worker");
        }
    }

    #[test]
    fn find_skill_reaches_each_content_skill_in_chinese_and_english() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        let first = |query: &str| -> String {
            let found = store.find_for_session("s1", None, query).unwrap();
            found["skills"][0]["name"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        };
        assert_eq!(first("公众号文章"), "wechat-article");
        assert_eq!(first("WeChat article"), "wechat-article");
        assert_eq!(first("五图图文笔记"), "five-image-note");
        assert_eq!(first("小红书"), "five-image-note");
        assert_eq!(first("短视频素材包"), "short-video-pack");
        assert_eq!(first("video"), "short-video-pack");
    }

    #[test]
    fn each_content_skill_names_its_files_asks_once_cites_sources_and_defers_to_the_privacy_level()
    {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        let files = [
            (
                "wechat-article",
                vec!["article.md", "article.html", "publish-notes.md"],
            ),
            (
                "five-image-note",
                vec!["cards.pptx", "note.md", "image-briefs.md"],
            ),
            (
                "short-video-pack",
                vec!["script.md", "shots.xlsx", "captions.md", "checklist.md"],
            ),
        ];
        for (name, expected) in files {
            let read = store.read_for_session("s1", None, name).unwrap();
            assert_eq!(read["ok"], true, "{name}");
            let text = read["instructions"].as_str().unwrap();
            for file in expected {
                assert!(text.contains(file), "{name} must name {file}");
            }
            for needle in [
                "ask_user",
                "来源",
                "检查清单",
                "隐私级别",
                "没有来源的不要写成事实",
            ] {
                assert!(text.contains(needle), "{name} must mention {needle}");
            }
            // It says what it cannot do instead of pretending to.
            assert!(text.contains("不能"), "{name}");
            // A fixed rule of the skill pack: nothing here tells the model to skip an approval.
            assert!(
                !text.contains("无需批准") && !text.contains("不需要批准"),
                "{name}"
            );
        }
    }

    #[test]
    fn a_builtin_skill_can_be_turned_off_but_not_edited_or_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        let off = store
            .update("wechat-article", &json!({"enabled": false}), None)
            .unwrap();
        assert_eq!(off["ok"], true);
        let row = builtin_rows(&store)
            .into_iter()
            .find(|row| row["name"] == "wechat-article")
            .unwrap();
        assert_eq!(row["enabled"], false);
        let found = store.find_for_session("s1", None, "公众号").unwrap();
        assert_eq!(found["total"], 0, "a skill that is off is not offered");

        let edited = store
            .update(
                "wechat-article",
                &json!({"instructions": "something else"}),
                None,
            )
            .unwrap();
        assert_eq!(edited["ok"], false);
        assert!(edited["error"].as_str().unwrap().contains("built-in"));
        let deleted = store.delete("wechat-article", None).unwrap();
        assert_eq!(deleted["ok"], false);
        assert!(deleted["error"].as_str().unwrap().contains("built-in"));
        assert_eq!(builtin_rows(&store).len(), 3, "nothing was removed");
    }

    #[test]
    fn a_skill_of_the_users_own_with_the_same_name_replaces_the_builtin_and_can_be_edited() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        store
            .create(&json!({"name": "wechat-article", "description": "mine", "instructions": "My way."}))
            .unwrap();
        let rows = store.list(None).unwrap();
        let mine = rows
            .iter()
            .find(|row| row["name"] == "wechat-article")
            .unwrap();
        assert_eq!(mine["scope"], "global");
        assert_eq!(mine["instructions"], "My way.");
        assert_eq!(builtin_rows(&store).len(), 2);
        assert_eq!(
            store
                .update("wechat-article", &json!({"instructions": "Changed."}), None)
                .unwrap()["ok"],
            true
        );
        assert_eq!(store.delete("wechat-article", None).unwrap()["ok"], true);
        assert_eq!(builtin_rows(&store).len(), 3, "the built-in one is back");
    }

    #[test]
    fn the_files_the_skills_ask_for_can_be_made_with_create_document() {
        // five-image-note: five slides, a title and bullets each.
        let slides: Vec<Value> = (1..=5)
            .map(|n| json!({"title": format!("卡片 {n}"), "bullets": ["要点一", "要点二", "要点三"]}))
            .collect();
        let (_, bytes, _) =
            crate::document::create("cards.pptx", &json!({"slides": slides})).unwrap();
        let parsed = crate::document::parse(crate::document::Kind::Pptx, &bytes).unwrap();
        assert_eq!(parsed.blocks.len(), 5);

        // short-video-pack: a header row, shots, and a SUM row that must equal the target length.
        let rows = json!([
            [
                "镜号",
                "时长(秒)",
                "景别",
                "画面内容",
                "口播/字幕",
                "音效/配乐",
                "素材来源"
            ],
            [1, 3, "近景", "钩子", "你知道吗", "无", "自拍"],
            [2, 20, "中景", "要点一", "第一点", "轻音乐", "自拍"],
            [3, 22, "全景", "要点二与结尾", "第二点", "轻音乐", "自拍"],
            ["合计", "=SUM(B2:B4)", null, null, null, null, null]
        ]);
        let (_, bytes, _) = crate::document::create(
            "shots.xlsx",
            &json!({"sheets": [{"name": "分镜", "header": true, "rows": rows}]}),
        )
        .unwrap();
        let parsed = crate::document::parse(crate::document::Kind::Xlsx, &bytes).unwrap();
        let text = &parsed.blocks[0].text;
        assert!(
            text.contains("=SUM(B2:B4)") || text.contains("合计"),
            "{text}"
        );
    }

    /// Deterministic checks on every built-in skill (ADR-0076): the result (the skill names the
    /// tools it uses, and each is a real tool), the compliance (no wording that claims an approval
    /// is skipped), and the process (numbered steps, and a section that says what it cannot do).
    #[test]
    fn every_built_in_skill_passes_the_deterministic_checks() {
        let temp = tempfile::tempdir().unwrap();
        let store = SkillStore::open(temp.path()).unwrap();
        let host = crate::capability::CapabilityHost::product_defaults().unwrap();
        let registered: std::collections::HashSet<String> = host
            .tool_schemas()
            .as_array()
            .unwrap()
            .iter()
            .map(|schema| schema["function"]["name"].as_str().unwrap().to_owned())
            .collect();
        let forbidden = ["无需批准", "不需要批准", "跳过审批", "绕过审批"];
        let rows = builtin_rows(&store);
        assert!(rows.len() >= 3, "{}", rows.len());
        for row in &rows {
            let name = row["name"].as_str().unwrap();
            let read = store.read_for_session("s1", None, name).unwrap();
            let text = read["instructions"].as_str().unwrap();

            // Result: every tool the skill names is a tool the product registers.
            for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                let looks_like_tool = word.contains('_')
                    && [
                        "statistics_",
                        "report_",
                        "chart_",
                        "read_",
                        "create_",
                        "edit_",
                        "web_",
                        "ask_",
                    ]
                    .iter()
                    .any(|prefix| word.starts_with(prefix));
                if looks_like_tool {
                    assert!(
                        registered.contains(word),
                        "{name} names {word}, which is not a tool"
                    );
                }
            }

            // Compliance: nothing says an approval can be skipped.
            for phrase in forbidden {
                assert!(!text.contains(phrase), "{name} says {phrase}");
            }

            // Process: numbered steps, and a statement of what the skill cannot do.
            for number in ["1. ", "2. ", "3. "] {
                assert!(text.contains(number), "{name} has no step {number}");
            }
            assert!(
                text.contains("不能") || text.contains("做不了"),
                "{name} says nothing it cannot do"
            );
        }
    }
}
