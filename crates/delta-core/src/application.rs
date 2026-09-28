//! Rust authority for connector configuration and product routing state.
//!
//! Product state and connector credentials live in one private authority file.
//! Secrets never cross the product IPC boundary; connector workers own remote protocol details.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::extension_manifest::EXTENSIONS_DIRNAME;
use crate::{durability::atomic_write_private, ShadowReadError};

/// Public manifest contract for connector product metadata.
///
/// Foundation owns the schema, validation, local connector state and credential
/// authority. Concrete first-party product catalogs are supplied by installed
/// extensions through `<state>/extensions/<source>/connector-catalog.json`; they are not compiled into Core.
pub const CONNECTOR_CATALOG_FILENAME: &str = "connector-catalog.json";
const CONNECTOR_CATALOG_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConnectorSpec {
    name: String,
    title: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    logo: String,
    #[serde(default)]
    blurb: String,
    #[serde(default)]
    about: String,
    #[serde(default)]
    access: Vec<String>,
    #[serde(default)]
    auth: String,
    #[serde(default)]
    two_way: bool,
    #[serde(default)]
    channels: bool,
    #[serde(default = "default_true")]
    available: bool,
    #[serde(default)]
    fields: Vec<Value>,
    #[serde(default)]
    instructions: Vec<String>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default = "default_brand_color")]
    brand_color: String,
    #[serde(default)]
    managed: bool,
    #[serde(default)]
    managed_profile: bool,
    #[serde(default)]
    risk_notice: String,
    #[serde(default)]
    ui: ConnectorUiSpec,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConnectorUiSpec {
    #[serde(default)]
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConnectorCatalogManifest {
    version: u32,
    #[serde(default)]
    source: String,
    #[serde(default)]
    connectors: Vec<ConnectorSpec>,
}

fn default_true() -> bool {
    true
}

fn default_brand_color() -> String {
    "#6b7280".to_string()
}

fn supported_detail_view(value: &str) -> bool {
    matches!(
        value,
        "" | "generic"
            | "accounts"
            | "mail_accounts_privacy"
            | "calendar_accounts"
            | "crm_portals_privacy"
            | "workspace_chat"
            | "code_host_installations"
    )
}

fn foundation_connectors() -> Vec<ConnectorSpec> {
    vec![ConnectorSpec {
        name: "email".to_string(),
        title: "Email (IMAP)".to_string(),
        icon: "✉".to_string(),
        logo: "email".to_string(),
        blurb: "Read, search, and send mail from any IMAP account.".to_string(),
        about: "Generic IMAP/SMTP mail access using provider-supported app credentials."
            .to_string(),
        access: vec![
            "Reads and searches mail over IMAP.".to_string(),
            "Sends mail through SMTP and can save approved attachments locally.".to_string(),
        ],
        auth: "app_password".to_string(),
        fields: vec![
            json!({"key": "address", "label": "Email address", "secret": false, "required": true, "help": "", "placeholder": ""}),
            json!({"key": "app_password", "label": "App password", "secret": true, "required": true, "help": "", "placeholder": ""}),
            json!({"key": "display_name", "label": "Display name", "secret": false, "required": false, "help": "", "placeholder": ""}),
            json!({"key": "imap_host", "label": "IMAP host (advanced)", "secret": false, "required": false, "help": "", "placeholder": ""}),
            json!({"key": "imap_port", "label": "IMAP port (advanced)", "secret": false, "required": false, "help": "", "placeholder": ""}),
            json!({"key": "smtp_host", "label": "SMTP host (advanced)", "secret": false, "required": false, "help": "", "placeholder": ""}),
            json!({"key": "smtp_port", "label": "SMTP port (advanced)", "secret": false, "required": false, "help": "", "placeholder": ""}),
        ],
        instructions: vec!["Enter the address and app password; use host fields only when auto-detection is unavailable.".to_string()],
        brand_color: default_brand_color(),
        available: true,
        ui: ConnectorUiSpec {
            detail: "generic".to_string(),
        },
        ..ConnectorSpec::default()
    }]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConnectorState {
    connected: bool,
    enabled: bool,
    account: Option<String>,
    #[serde(default)]
    tools: BTreeMap<String, bool>,
    #[serde(default)]
    details: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ApplicationState {
    #[serde(default)]
    connectors: BTreeMap<String, ConnectorState>,
    #[serde(default)]
    session_connections: BTreeMap<String, BTreeMap<String, bool>>,
    #[serde(default)]
    subscriptions: Vec<Value>,
    #[serde(default)]
    inbox_bindings: Vec<Value>,
    #[serde(default)]
    unrouted: Vec<Value>,
    #[serde(default)]
    recent_channels: Vec<Value>,
    dm_session: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ApplicationAuthority {
    #[serde(default)]
    state: ApplicationState,
    #[serde(default)]
    secrets: BTreeMap<String, BTreeMap<String, String>>,
}

pub struct ApplicationStore {
    authority_path: PathBuf,
    extensions_dir: PathBuf,
}

impl ApplicationStore {
    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, ShadowReadError> {
        std::fs::create_dir_all(state_dir.as_ref())?;
        let store = Self {
            authority_path: state_dir.as_ref().join("application.json"),
            extensions_dir: state_dir.as_ref().join(EXTENSIONS_DIRNAME),
        };
        store.read_authority()?;
        Ok(store)
    }

    fn read_catalog(&self) -> Result<Vec<ConnectorSpec>, ShadowReadError> {
        let mut connectors = foundation_connectors();
        let mut seen: BTreeSet<String> = connectors.iter().map(|item| item.name.clone()).collect();
        if !self.extensions_dir.is_dir() {
            return Ok(connectors);
        }

        let mut catalog_paths = Vec::new();
        for entry in std::fs::read_dir(&self.extensions_dir)? {
            let entry = entry?;
            let extension_dir = entry.path();
            if !extension_dir.is_dir() {
                continue;
            }
            let catalog = extension_dir.join(CONNECTOR_CATALOG_FILENAME);
            if catalog.is_file() {
                catalog_paths.push(catalog);
            }
        }
        catalog_paths.sort();

        for path in catalog_paths {
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!(
                        "Delta extension connector catalog ignored ({}): {error}",
                        path.display()
                    );
                    continue;
                }
            };
            let manifest: ConnectorCatalogManifest = match serde_json::from_str(&text) {
                Ok(manifest) => manifest,
                Err(error) => {
                    eprintln!(
                        "Delta extension connector catalog ignored ({}): {error}",
                        path.display()
                    );
                    continue;
                }
            };
            if manifest.version != CONNECTOR_CATALOG_VERSION {
                eprintln!(
                    "Delta extension connector catalog ignored ({}): unsupported version {}",
                    path.display(),
                    manifest.version
                );
                continue;
            }

            let source = manifest.source.trim();
            let directory_source = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if source.is_empty() || source != directory_source {
                eprintln!(
                    "Delta extension connector catalog ignored ({}): source '{}' does not match directory '{}'",
                    path.display(),
                    source,
                    directory_source
                );
                continue;
            }

            let mut staged = Vec::new();
            let mut staged_seen = seen.clone();
            let mut invalid = None;
            for mut spec in manifest.connectors {
                spec.name = spec.name.trim().to_string();
                spec.title = spec.title.trim().to_string();
                if spec.name.is_empty() || spec.title.is_empty() {
                    invalid = Some("connector catalog entries require name and title".to_string());
                    break;
                }
                if !staged_seen.insert(spec.name.clone()) {
                    invalid = Some(format!(
                        "connector catalog cannot replace an existing connector: {}",
                        spec.name
                    ));
                    break;
                }
                spec.ui.detail = spec.ui.detail.trim().to_string();
                if !supported_detail_view(&spec.ui.detail) {
                    invalid = Some(format!(
                        "unsupported connector detail view: {}",
                        spec.ui.detail
                    ));
                    break;
                }
                if spec.logo.is_empty() {
                    spec.logo = spec.name.clone();
                }
                if spec.icon.is_empty() {
                    spec.icon = spec.logo.clone();
                }
                staged.push(spec);
            }

            if let Some(error) = invalid {
                eprintln!(
                    "Delta extension connector catalog ignored ({}): {error}",
                    path.display()
                );
                continue;
            }

            seen = staged_seen;
            connectors.extend(staged);
        }

        Ok(connectors)
    }

    fn read_authority(&self) -> Result<ApplicationAuthority, ShadowReadError> {
        if !self.authority_path.exists() {
            return Ok(ApplicationAuthority::default());
        }
        let text = std::fs::read_to_string(&self.authority_path)?;
        Ok(serde_json::from_str(&text)?)
    }

    fn write_authority(&self, authority: &ApplicationAuthority) -> Result<(), ShadowReadError> {
        let bytes = serde_json::to_vec_pretty(authority)?;
        atomic_write_private(&self.authority_path, &bytes)?;
        Ok(())
    }

    fn read(&self) -> Result<ApplicationState, ShadowReadError> {
        Ok(self.read_authority()?.state)
    }

    fn write(&self, state: &ApplicationState) -> Result<(), ShadowReadError> {
        let mut authority = self.read_authority()?;
        authority.state = state.clone();
        self.write_authority(&authority)
    }

    pub fn connectors(&self) -> Result<Vec<Value>, ShadowReadError> {
        let state = self.read()?;
        Ok(self
            .read_catalog()?
            .into_iter()
            .map(|spec| {
                let current = state
                    .connectors
                    .get(&spec.name)
                    .cloned()
                    .unwrap_or_default();
                let tools = spec
                    .tools
                    .iter()
                    .map(|tool| {
                        let enabled = current.tools.get(tool).copied().unwrap_or(true);
                        json!({
                            "name": tool, "label": tool, "kind": "write",
                            "description": "Connector worker capability", "enabled": enabled,
                            "requires_approval": true,
                        })
                    })
                    .collect::<Vec<_>>();
                let mut row = json!({
                    "name": spec.name, "title": spec.title, "icon": spec.icon, "logo": spec.logo,
                    "blurb": spec.blurb, "about": spec.about, "access": spec.access,
                    "auth": spec.auth, "two_way": spec.two_way, "channels": spec.channels,
                    "available": spec.available, "fields": spec.fields,
                    "instructions": spec.instructions,
                    "connected": current.connected, "account": current.account,
                    "enabled": current.enabled, "brand_color": spec.brand_color,
                    "allowed_users": [], "tools": tools, "managed": spec.managed,
                    "managed_profile": spec.managed_profile, "risk_notice": spec.risk_notice,
                    "ui": spec.ui,
                });
                if let Some(object) = row.as_object_mut() {
                    for (key, value) in current.details {
                        object.insert(key, value);
                    }
                }
                row
            })
            .collect())
    }

    pub fn connect(
        &self,
        name: &str,
        fields: &BTreeMap<String, String>,
    ) -> Result<Value, ShadowReadError> {
        let catalog = self.read_catalog()?;
        let Some(descriptor) = catalog.iter().find(|descriptor| descriptor.name == name) else {
            return Ok(json!({"ok": false, "error": "unknown connector"}));
        };
        if fields.is_empty() && descriptor.auth != "none" {
            return Ok(json!({"ok": false, "error": "connector credentials are required"}));
        }
        let mut authority = self.read_authority()?;
        authority.secrets.insert(name.to_string(), fields.clone());
        let account = fields
            .get("email")
            .or_else(|| fields.get("account"))
            .or_else(|| fields.get("workspace"))
            .cloned()
            .unwrap_or_else(|| "Connected".to_string());
        let previous = authority.state.connectors.remove(name).unwrap_or_default();
        authority.state.connectors.insert(
            name.to_string(),
            ConnectorState {
                connected: true,
                enabled: true,
                account: Some(account.clone()),
                tools: previous.tools,
                details: previous.details,
            },
        );
        self.write_authority(&authority)?;
        Ok(json!({"ok": true, "account": account}))
    }

    /// Resolve the exact secret keys granted to one capability job.
    ///
    /// Keys use the canonical `connector.field` form.
    /// Missing connectors/fields are omitted rather than widened. Callers never
    /// receive the full authority map.
    pub fn resolve_capability_secrets(
        &self,
        keys: &[String],
    ) -> Result<BTreeMap<String, String>, ShadowReadError> {
        let authority = self.read_authority()?;
        let mut resolved = BTreeMap::new();
        for requested in keys {
            let split = requested.split_once('.');
            let Some((connector, field)) = split else {
                continue;
            };
            let Some(value) = authority
                .secrets
                .get(connector)
                .and_then(|profile| profile.get(field))
            else {
                continue;
            };
            resolved.insert(requested.clone(), value.clone());
        }
        Ok(resolved)
    }

    pub fn disconnect(&self, name: &str) -> Result<Value, ShadowReadError> {
        let mut authority = self.read_authority()?;
        let removed = authority.state.connectors.remove(name).is_some();
        authority.secrets.remove(name);
        self.write_authority(&authority)?;
        Ok(json!({"ok": removed}))
    }

    pub fn update_tools(
        &self,
        name: &str,
        enabled: &BTreeMap<String, bool>,
    ) -> Result<Value, ShadowReadError> {
        let mut state = self.read()?;
        let connector = state.connectors.entry(name.to_string()).or_default();
        for (tool, value) in enabled {
            connector.tools.insert(tool.clone(), *value);
        }
        let tools = connector.tools.clone();
        self.write(&state)?;
        Ok(json!({"ok": true, "tools": tools}))
    }

    pub fn action(
        &self,
        name: &str,
        action: &str,
        payload: &Value,
    ) -> Result<Value, ShadowReadError> {
        let mut state = self.read()?;
        let connector = state.connectors.entry(name.to_string()).or_default();
        let array_add = |details: &mut BTreeMap<String, Value>, key: &str, value: String| {
            let values = details.entry(key.to_string()).or_insert_with(|| json!([]));
            if let Some(values) = values.as_array_mut() {
                if !values.iter().any(|item| item.as_str() == Some(&value)) {
                    values.push(Value::String(value));
                }
            }
        };
        let array_remove = |details: &mut BTreeMap<String, Value>, key: &str, value: &str| {
            if let Some(values) = details.get_mut(key).and_then(Value::as_array_mut) {
                values.retain(|item| item.as_str() != Some(value));
            }
        };
        let response = match action {
            "allow_user" => {
                array_add(
                    &mut connector.details,
                    "allowed_users",
                    payload
                        .get("user_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                );
                json!({"ok": true})
            }
            "disallow_user" => {
                array_remove(
                    &mut connector.details,
                    "allowed_users",
                    payload
                        .get("user_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
                json!({"ok": true})
            }
            "add_approval_owner" => {
                array_add(
                    &mut connector.details,
                    "approval_owner_ids",
                    payload
                        .get("user_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                );
                json!({"ok": true})
            }
            "remove_approval_owner" => {
                array_remove(
                    &mut connector.details,
                    "approval_owner_ids",
                    payload
                        .get("user_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                );
                json!({"ok": true})
            }
            "set_filters" => {
                connector
                    .details
                    .insert("filters".to_string(), payload.clone());
                json!({"ok": true, "filters": payload})
            }
            "set_hidden_fields" => {
                let fields = payload.get("fields").cloned().unwrap_or_else(|| json!([]));
                connector
                    .details
                    .insert("hidden_fields".to_string(), fields.clone());
                json!({"ok": true, "hidden_fields": fields})
            }
            "directory" => json!({"ok": true, "members": []}),
            "channels" => json!({"ok": true, "channels": []}),
            "github_status" => {
                json!({"ok": true, "mode": "", "relay": {"state": "offline", "reconnects": 0, "last_event_at": null, "last_error": ""}, "installs": {}, "missed": {}})
            }
            "slack_status" => {
                json!({"mode": "", "relay": {"state": "offline", "reconnects": 0, "last_event_at": null, "last_error": ""}, "teams": {}})
            }
            "resolve_unauthorized" => json!({"ok": true}),
            _ => json!({"ok": false, "error": format!("unsupported connector action: {action}")}),
        };
        self.write(&state)?;
        Ok(response)
    }

    pub fn session_connections(&self, session_id: &str) -> Result<Value, ShadowReadError> {
        let state = self.read()?;
        let overrides = state.session_connections.get(session_id);
        let connected = state.connectors.iter().filter(|(_, connector)| connector.connected)
            .map(|(name, connector)| json!({
                "connector": name,
                "enabled": overrides.and_then(|values| values.get(name)).copied().unwrap_or(connector.enabled),
                "detail": connector.account.clone().unwrap_or_default(),
            })).collect::<Vec<_>>();
        Ok(json!({"connected": connected, "recommended": [], "attention": 0}))
    }

    pub fn set_session_connection(
        &self,
        session_id: &str,
        connector: &str,
        is_enabled: bool,
        should_clear: bool,
    ) -> Result<Value, ShadowReadError> {
        let clear = should_clear;
        let enabled = is_enabled;
        let mut state = self.read()?;
        let overrides = state
            .session_connections
            .entry(session_id.to_string())
            .or_default();
        if clear {
            overrides.remove(connector);
        } else {
            overrides.insert(connector.to_string(), enabled);
        }
        self.write(&state)?;
        Ok(json!({"ok": true}))
    }

    pub fn subscriptions(&self) -> Result<Vec<Value>, ShadowReadError> {
        Ok(self.read()?.subscriptions)
    }

    pub fn subscribe(&self, session_id: &str, channel: &str) -> Result<Value, ShadowReadError> {
        if session_id.is_empty() || channel.trim().is_empty() {
            return Ok(json!({"ok": false, "error": "session and channel are required"}));
        }
        let mut state = self.read()?;
        state.subscriptions.retain(|item| {
            item.get("session_id").and_then(Value::as_str) != Some(session_id)
                || item.get("channel").and_then(Value::as_str) != Some(channel)
        });
        state.subscriptions.push(
            json!({"session_id": session_id, "session_title": "", "agent": "delta",
            "channel": channel, "channel_name": null, "routing_target": null, "collision": false}),
        );
        self.write(&state)?;
        Ok(json!({"ok": true, "channel": channel}))
    }

    pub fn unsubscribe(&self, session_id: &str, channel: &str) -> Result<Value, ShadowReadError> {
        let mut state = self.read()?;
        let before = state.subscriptions.len();
        state.subscriptions.retain(|item| {
            item.get("session_id").and_then(Value::as_str) != Some(session_id)
                || item.get("channel").and_then(Value::as_str) != Some(channel)
        });
        let removed = state.subscriptions.len() != before;
        self.write(&state)?;
        Ok(json!({"ok": true, "removed": removed}))
    }

    pub fn inbox_bindings(&self) -> Result<Vec<Value>, ShadowReadError> {
        Ok(self.read()?.inbox_bindings)
    }

    pub fn set_inbox_binding(
        &self,
        name: &str,
        channel: Option<&str>,
        target: &str,
    ) -> Result<Value, ShadowReadError> {
        let mut state = self.read()?;
        state
            .inbox_bindings
            .retain(|item| item.get("name").and_then(Value::as_str) != Some(name));
        state
            .inbox_bindings
            .push(json!({"name": name, "channel": channel, "target": target}));
        let bindings = state.inbox_bindings.clone();
        self.write(&state)?;
        Ok(json!({"ok": true, "bindings": bindings}))
    }

    pub fn unrouted(&self) -> Result<Vec<Value>, ShadowReadError> {
        Ok(self.read()?.unrouted)
    }
    pub fn recent_channels(&self) -> Result<Vec<Value>, ShadowReadError> {
        Ok(self.read()?.recent_channels)
    }
    pub fn dm_route(&self) -> Result<Option<String>, ShadowReadError> {
        Ok(self.read()?.dm_session)
    }

    pub fn set_dm_route(&self, session_id: &str) -> Result<Value, ShadowReadError> {
        let mut state = self.read()?;
        state.dm_session = (!session_id.is_empty()).then(|| session_id.to_string());
        let route = state.dm_session.clone();
        self.write(&state)?;
        Ok(json!({"ok": true, "dm_session": route}))
    }

    pub fn connected_names(&self) -> Result<BTreeSet<String>, ShadowReadError> {
        Ok(self
            .read()?
            .connectors
            .into_iter()
            .filter(|(_, state)| state.connected && state.enabled)
            .map(|(name, _)| name)
            .collect())
    }

    pub fn tool_available(
        &self,
        connector_name: &str,
        tool_name: &str,
        session_id: Option<&str>,
    ) -> Result<bool, ShadowReadError> {
        let state = self.read()?;
        let Some(connector) = state.connectors.get(connector_name) else {
            return Ok(false);
        };
        if !connector.connected || !connector.enabled {
            return Ok(false);
        }
        if let Some(session_id) = session_id {
            if state
                .session_connections
                .get(session_id)
                .and_then(|items| items.get(connector_name))
                == Some(&false)
            {
                return Ok(false);
            }
        }
        Ok(connector.tools.get(tool_name).copied().unwrap_or(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_secrets_never_cross_the_product_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let store = ApplicationStore::open(temp.path()).unwrap();
        store
            .connect(
                "email",
                &BTreeMap::from([("token".to_string(), "secret-value".to_string())]),
            )
            .unwrap();
        assert!(!serde_json::to_string(&store.connectors().unwrap())
            .unwrap()
            .contains("secret-value"));
        assert!(store.connected_names().unwrap().contains("email"));
    }

    #[test]
    fn capability_secret_keys_require_canonical_dot_syntax() {
        let temp = tempfile::tempdir().unwrap();
        let store = ApplicationStore::open(temp.path()).unwrap();
        store
            .connect(
                "email",
                &BTreeMap::from([("token".to_string(), "secret-value".to_string())]),
            )
            .unwrap();

        let resolved = store
            .resolve_capability_secrets(&["email.token".to_string(), "email:token".to_string()])
            .unwrap();
        assert_eq!(
            resolved.get("email.token").map(String::as_str),
            Some("secret-value")
        );
        assert!(!resolved.contains_key("email:token"));
    }

    #[test]
    fn subscriptions_are_idempotent_and_removable() {
        let temp = tempfile::tempdir().unwrap();
        let store = ApplicationStore::open(temp.path()).unwrap();
        store.subscribe("s1", "slack:C1").unwrap();
        store.subscribe("s1", "slack:C1").unwrap();
        assert_eq!(store.subscriptions().unwrap().len(), 1);
        assert_eq!(
            store.unsubscribe("s1", "slack:C1").unwrap()["removed"],
            true
        );
    }

    #[test]
    fn corrupt_consolidated_authority_fails_closed_and_is_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let authority_path = temp.path().join("application.json");
        std::fs::write(&authority_path, b"{not-json").unwrap();

        assert!(matches!(
            ApplicationStore::open(temp.path()),
            Err(ShadowReadError::Json(_))
        ));
        assert_eq!(std::fs::read(&authority_path).unwrap(), b"{not-json");
    }

    #[test]
    fn connect_and_disconnect_commit_state_and_secrets_together() {
        let temp = tempfile::tempdir().unwrap();
        let store = ApplicationStore::open(temp.path()).unwrap();
        store
            .connect(
                "email",
                &BTreeMap::from([("token".to_string(), "secret-value".to_string())]),
            )
            .unwrap();
        let connected = store.read_authority().unwrap();
        assert!(connected.state.connectors["email"].connected);
        assert_eq!(connected.secrets["email"]["token"], "secret-value");

        store.disconnect("email").unwrap();
        let disconnected = store.read_authority().unwrap();
        assert!(!disconnected.state.connectors.contains_key("email"));
        assert!(!disconnected.secrets.contains_key("email"));
    }

    #[cfg(unix)]
    #[test]
    fn application_authority_is_persisted_private() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let store = ApplicationStore::open(temp.path()).unwrap();
        store
            .connect(
                "email",
                &BTreeMap::from([("token".to_string(), "secret-value".to_string())]),
            )
            .unwrap();
        let mode = std::fs::metadata(temp.path().join("application.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn foundation_catalog_contains_only_generic_email_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let store = ApplicationStore::open(temp.path()).unwrap();
        let rows = store.connectors().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "email");
        assert_eq!(rows[0]["auth"], "app_password");
        assert!(!rows[0]["fields"].as_array().unwrap().is_empty());
    }

    fn write_catalog(root: &Path, source: &str, connectors: Value) {
        let dir = root.join(EXTENSIONS_DIRNAME).join(source);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(CONNECTOR_CATALOG_FILENAME),
            serde_json::to_vec_pretty(&json!({
                "version": CONNECTOR_CATALOG_VERSION,
                "source": source,
                "connectors": connectors,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn extension_catalog_is_loaded_without_exposing_secrets() {
        let temp = tempfile::tempdir().unwrap();
        write_catalog(
            temp.path(),
            "test-extension",
            json!([{
                "name": "suite_chat",
                "title": "Suite Chat",
                "logo": "suite-chat",
                "blurb": "Extension-owned connector",
                "auth": "token",
                "brand_color": "#123456",
                "two_way": true,
                "channels": true,
                "ui": {"detail": "accounts"},
                "fields": [{"key": "token", "label": "Token", "secret": true, "required": true}]
            }]),
        );
        let store = ApplicationStore::open(temp.path()).unwrap();
        store
            .connect(
                "suite_chat",
                &BTreeMap::from([("token".to_string(), "top-secret".to_string())]),
            )
            .unwrap();
        let rows = store.connectors().unwrap();
        let row = rows
            .iter()
            .find(|row| row["name"] == "suite_chat")
            .expect("extension connector in catalog");
        assert_eq!(row["brand_color"], "#123456");
        assert_eq!(row["two_way"], true);
        assert_eq!(row["ui"]["detail"], "accounts");
        assert!(!row.to_string().contains("top-secret"));
    }

    #[test]
    fn multiple_extension_catalogs_coexist() {
        let temp = tempfile::tempdir().unwrap();
        write_catalog(
            temp.path(),
            "alpha",
            json!([{"name": "alpha_connector", "title": "Alpha", "ui": {"detail": "generic"}}]),
        );
        write_catalog(
            temp.path(),
            "beta",
            json!([{"name": "beta_connector", "title": "Beta", "ui": {"detail": "accounts"}}]),
        );

        let store = ApplicationStore::open(temp.path()).unwrap();
        let rows = store.connectors().unwrap();
        let names = rows
            .iter()
            .filter_map(|row| row["name"].as_str())
            .collect::<BTreeSet<_>>();
        assert!(names.contains("email"));
        assert!(names.contains("alpha_connector"));
        assert!(names.contains("beta_connector"));
    }

    #[test]
    fn invalid_extension_catalog_is_isolated() {
        let temp = tempfile::tempdir().unwrap();
        write_catalog(
            temp.path(),
            "bad",
            json!([{
                "name": "unsafe_ui",
                "title": "Unsafe UI",
                "ui": {"detail": "arbitrary_component"}
            }]),
        );
        write_catalog(
            temp.path(),
            "good",
            json!([{"name": "good_connector", "title": "Good", "ui": {"detail": "generic"}}]),
        );

        let store = ApplicationStore::open(temp.path()).unwrap();
        let rows = store.connectors().unwrap();
        assert!(rows.iter().any(|row| row["name"] == "email"));
        assert!(rows.iter().any(|row| row["name"] == "good_connector"));
        assert!(!rows.iter().any(|row| row["name"] == "unsafe_ui"));
    }

    #[test]
    fn extension_catalog_cannot_replace_foundation_baseline() {
        let temp = tempfile::tempdir().unwrap();
        write_catalog(
            temp.path(),
            "replacement",
            json!([{"name": "email", "title": "Replacement"}]),
        );

        let store = ApplicationStore::open(temp.path()).unwrap();
        let rows = store.connectors().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "email");
        assert_eq!(rows[0]["title"], "Email (IMAP)");
    }
}
