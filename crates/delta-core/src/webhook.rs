//! The optional completion webhook (roadmap B5).
//!
//! The user may give one URL. When a turn finishes, Delta posts a short JSON summary to it. The
//! webhook is an outbound route like a model endpoint, so it is judged the same way before every
//! send (ADR-0057): the workspace's egress level and the class of the host decide whether the
//! post may go. A webhook can not ask the user, so a level that asks each time sends nothing; a
//! refused post is recorded and never retried. A redirect is never followed.
//!
//! The post carries only an event name, the session id, where the turn came from, when it
//! finished and how many files it produced, unless the user turns on `include_details`. Details
//! add the session title, the first lines of the final answer and the file names; they leave the
//! machine only when the user asked for them. Every attempt is written to the audit log with its
//! host, class, level and outcome, and never with the body.
//!
//! The setting is one owner-only file, `webhook.json`, in the state directory.

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

use crate::approval::{ApprovalRecordInput, ApprovalWriter};
use crate::durability::atomic_write_private;
use crate::egress::{self, EgressLevel, EndpointClass, EndpointInfo, ModelDecision};
use crate::ShadowReadError;

const SETTINGS_FILE: &str = "webhook.json";
const SETTINGS_VERSION: u32 = 1;
const MAX_URL_CHARS: usize = 2048;
/// Characters of the final answer a detailed post carries.
pub const SUMMARY_CHARS: usize = 280;
/// Files a detailed post names; the count always says how many there were.
const MAX_LISTED_ARTIFACTS: usize = 20;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Settings {
    version: u32,
    url: Option<String>,
    is_enabled: bool,
    include_details: bool,
}

fn settings_path(state_dir: &Path) -> PathBuf {
    state_dir.join(SETTINGS_FILE)
}

fn load(state_dir: &Path) -> Result<Settings, ShadowReadError> {
    let path = settings_path(state_dir);
    if !path.exists() {
        return Ok(Settings {
            version: SETTINGS_VERSION,
            ..Settings::default()
        });
    }
    let text = std::fs::read_to_string(&path).map_err(|error| {
        ShadowReadError::Parse(format!(
            "failed to read the webhook setting {}: {error}",
            path.display()
        ))
    })?;
    let settings: Settings = serde_json::from_str(&text).map_err(|error| {
        ShadowReadError::Parse(format!(
            "invalid webhook setting {}: {error}",
            path.display()
        ))
    })?;
    if settings.version != SETTINGS_VERSION {
        return Err(ShadowReadError::Parse(format!(
            "webhook setting {} has version {}, expected {SETTINGS_VERSION}",
            path.display(),
            settings.version
        )));
    }
    Ok(settings)
}

fn settings_json(settings: &Settings) -> Value {
    json!({
        "ok": true,
        "url": settings.url,
        "is_enabled": settings.is_enabled,
        "include_details": settings.include_details,
    })
}

/// The webhook setting as the desktop app shows it.
pub fn settings(state_dir: &Path) -> Result<Value, ShadowReadError> {
    Ok(settings_json(&load(state_dir)?))
}

/// Check a webhook URL: http or https, a host, no credentials in it, a sane length. Credentials
/// in a URL would be written to the audit log and shown in the settings page.
pub fn validate_url(raw: &str) -> Result<Url, String> {
    let text = raw.trim();
    if text.chars().count() > MAX_URL_CHARS {
        return Err(format!("the URL is longer than {MAX_URL_CHARS} characters"));
    }
    let url = Url::parse(text).map_err(|_| "the URL is not valid".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("the URL must start with http:// or https://".to_owned());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("the URL has no host".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("the URL must not contain a user name or password".to_owned());
    }
    Ok(url)
}

/// Save the webhook setting. An empty `url` clears it. Changing it is written to the audit log.
pub fn set_settings(
    state_dir: &Path,
    url: Option<&str>,
    is_enabled: bool,
    is_detailed: bool,
) -> Result<Value, String> {
    let url = match url.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => Some(validate_url(text)?.to_string()),
        None => None,
    };
    if is_enabled && url.is_none() {
        return Err("a URL is needed to turn the webhook on".to_owned());
    }
    let settings = Settings {
        version: SETTINGS_VERSION,
        url,
        is_enabled,
        include_details: is_detailed,
    };
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
    atomic_write_private(settings_path(state_dir), &bytes).map_err(|error| error.to_string())?;
    audit(
        state_dir,
        "",
        "changed",
        "ok",
        None,
        json!({
            "host": settings.url.as_deref().map(egress::url_host),
            "is_enabled": settings.is_enabled,
            "include_details": settings.include_details,
        }),
    );
    Ok(settings_json(&settings))
}

/// What a finished turn tells the webhook.
#[derive(Debug, Clone)]
pub struct Completion {
    pub session_id: String,
    /// `chat` for a turn the user started, `automation` for a scheduled run.
    pub source: &'static str,
    pub title: Option<String>,
    pub summary: String,
    /// (path, name) of every file the turn delivered.
    pub artifacts: Vec<(String, String)>,
    /// Seconds since the Unix epoch.
    pub finished_at: u64,
}

/// The first `limit` characters of `text` with its whitespace collapsed, and an ellipsis when cut.
pub fn summarize(text: &str, limit: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= limit {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}

/// The JSON body of a post.
pub fn payload(completion: &Completion, is_detailed: bool) -> Value {
    let mut body = json!({
        "event": "turn_completed",
        "source": completion.source,
        "session_id": completion.session_id,
        "finished_at": completion.finished_at,
        "artifact_count": completion.artifacts.len(),
    });
    if is_detailed {
        let files: Vec<Value> = completion
            .artifacts
            .iter()
            .take(MAX_LISTED_ARTIFACTS)
            .map(|(path, name)| json!({"path": path, "name": name}))
            .collect();
        body["title"] = json!(completion.title);
        body["summary"] = json!(completion.summary);
        body["artifacts"] = Value::Array(files);
    }
    body
}

/// What happened to one post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The webhook is off or has no URL; nothing was decided and nothing is recorded.
    Off,
    /// The server answered 2xx.
    Sent { host: String, status: u16 },
    /// The post was not made; the reason names the level and the host.
    Skipped { host: String, reason: String },
    /// The post was made and did not succeed.
    Failed { host: String, error: String },
}

impl Outcome {
    /// The outcome as the settings page shows it after a test.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Off => json!({"result": "off"}),
            Self::Sent { host, status } => {
                json!({"result": "sent", "host": host, "status": status})
            }
            Self::Skipped { host, reason } => {
                json!({"result": "skipped", "host": host, "reason": reason})
            }
            Self::Failed { host, error } => {
                json!({"result": "failed", "host": host, "error": error})
            }
        }
    }
}

/// Makes the connection use the addresses that were checked instead of resolving the name again.
#[derive(Debug)]
struct Pinned(Vec<SocketAddr>);

impl ureq::Resolver for Pinned {
    fn resolve(&self, _netloc: &str) -> io::Result<Vec<SocketAddr>> {
        Ok(self.0.clone())
    }
}

/// Judge the webhook against the workspace's egress level and make the post if it is allowed.
/// Blocking: callers that must not wait use [`deliver_in_background`].
pub fn deliver(state_dir: &Path, workspace: &str, completion: &Completion) -> Outcome {
    let settings = match load(state_dir) {
        Ok(settings) => settings,
        Err(error) => {
            return Outcome::Skipped {
                host: String::new(),
                reason: format!("the webhook setting could not be read: {error}"),
            }
        }
    };
    let Some(url) = settings.url.as_deref().filter(|_| settings.is_enabled) else {
        return Outcome::Off;
    };
    send(
        state_dir,
        workspace,
        url,
        &payload(completion, settings.include_details),
    )
}

/// Post a fixed test message to the saved URL, under the same judgement as a real post. A test
/// posts even while the webhook is switched off, so the URL can be tried before it is enabled.
pub fn test(state_dir: &Path, workspace: &str) -> Outcome {
    let settings = match load(state_dir) {
        Ok(settings) => settings,
        Err(error) => {
            return Outcome::Skipped {
                host: String::new(),
                reason: format!("the webhook setting could not be read: {error}"),
            }
        }
    };
    let Some(url) = settings.url.as_deref() else {
        return Outcome::Skipped {
            host: String::new(),
            reason: "save a URL first".to_owned(),
        };
    };
    send(
        state_dir,
        workspace,
        url,
        &json!({"event": "test", "source": "settings", "message": "Delta webhook test"}),
    )
}

/// [`deliver`] on its own thread, so a slow or dead server never holds up the next turn.
pub fn deliver_in_background(state_dir: PathBuf, workspace: String, completion: Completion) {
    std::thread::spawn(move || {
        deliver(&state_dir, &workspace, &completion);
    });
}

fn send(state_dir: &Path, workspace: &str, url: &str, body: &Value) -> Outcome {
    let host = egress::url_host(url);
    let skipped = |reason: String, class: &str, level: Option<EgressLevel>| {
        audit(
            state_dir,
            workspace,
            "skipped",
            "denied",
            Some(&reason),
            json!({"host": host, "class": class, "level": level.map(EgressLevel::as_str)}),
        );
        Outcome::Skipped {
            host: host.clone(),
            reason,
        }
    };
    let level = match egress::level_for(state_dir, workspace) {
        Ok(level) => level,
        Err(error) => {
            return skipped(
                format!("the egress settings could not be read, so nothing was sent: {error}"),
                "unknown",
                None,
            )
        }
    };
    let Some(level) = level else {
        return skipped(
            "Choose where this workspace's content may be sent (Settings ▸ Privacy) before the webhook sends anything.".to_owned(),
            "unknown",
            None,
        );
    };
    let endpoint = match egress::classify_endpoint(state_dir, url) {
        Ok(endpoint) => endpoint,
        // Open and Ask each time do not depend on the class; the others can not decide without it.
        Err(_) if matches!(level, EgressLevel::Open | EgressLevel::AskEachTime) => EndpointInfo {
            host: host.clone(),
            class: EndpointClass::Public,
            addrs: Vec::new(),
        },
        Err(error) => {
            return skipped(
                format!("this workspace's egress level needs to know where {host} is, and it could not be checked: {error}. Nothing was sent."),
                "unresolved",
                Some(level),
            )
        }
    };
    let class = endpoint.class.as_str();
    match egress::decide_model_request(Some(level), &endpoint.class) {
        ModelDecision::Allow => {}
        ModelDecision::Ask => {
            return skipped(
                format!("This workspace asks each time before content is sent, and a webhook can not ask. Nothing was sent to {host}."),
                class,
                Some(level),
            )
        }
        ModelDecision::Deny(reason) => {
            return skipped(
                format!("Not sent to {host}: {reason}."),
                class,
                Some(level),
            )
        }
        ModelDecision::NotChosen => {
            return skipped(
                "Choose where this workspace's content may be sent (Settings ▸ Privacy) before the webhook sends anything.".to_owned(),
                class,
                Some(level),
            )
        }
    }
    // A class that came from the addresses holds only for those addresses.
    let relies_on_addresses = endpoint.class == EndpointClass::Private
        && matches!(
            level,
            EgressLevel::PrivateOnly | EgressLevel::ApprovedEndpoints
        );
    let mut builder = ureq::AgentBuilder::new()
        .user_agent(concat!("Delta/", env!("CARGO_PKG_VERSION"), " (+webhook)"))
        .redirects(0)
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT);
    if relies_on_addresses && !endpoint.addrs.is_empty() {
        builder = builder.resolver(Pinned(endpoint.addrs.clone()));
    }
    let result = builder
        .build()
        .post(url)
        .set("Content-Type", "application/json")
        .send_string(&body.to_string());
    // With redirects switched off, ureq hands back a 3xx as a response, not as an error.
    let failure = match &result {
        Ok(response) if (200..300).contains(&response.status()) => None,
        Ok(response) if (300..400).contains(&response.status()) => Some(format!(
            "the server answered HTTP {}; a webhook redirect is not followed",
            response.status()
        )),
        Ok(response) => Some(format!("the server answered HTTP {}", response.status())),
        Err(ureq::Error::Status(code, _)) => Some(format!("the server answered HTTP {code}")),
        Err(error) => Some(format!("the post failed: {error}")),
    };
    let (outcome, stage, status, detail) = match (failure, &result) {
        (None, Ok(response)) => (
            Outcome::Sent {
                host: host.clone(),
                status: response.status(),
            },
            "sent",
            "ok",
            None,
        ),
        (error, _) => {
            let error = error.unwrap_or_default();
            (
                Outcome::Failed {
                    host: host.clone(),
                    error: error.clone(),
                },
                "failed",
                "error",
                Some(error),
            )
        }
    };
    audit(
        state_dir,
        workspace,
        stage,
        status,
        detail.as_deref(),
        json!({"host": host, "class": class, "level": level.as_str()}),
    );
    outcome
}

/// Write one webhook event to the audit log. A failure to write is reported on standard error and
/// never changes the outcome.
fn audit(
    state_dir: &Path,
    workspace: &str,
    stage: &str,
    status: &str,
    reason: Option<&str>,
    arguments: Value,
) {
    let path = state_dir.join("audit_events.db");
    let result = ApprovalWriter::open(path.to_string_lossy().as_ref()).and_then(|mut writer| {
        writer.record(
            ApprovalRecordInput {
                session_id: String::new(),
                agent: None,
                workspace: Some(workspace.to_owned()),
                connector: None,
                tool: "notification.webhook".to_owned(),
                stage: stage.to_owned(),
                status: Some(status.to_owned()),
                approval: None,
                arguments: Some(arguments),
                result_preview: None,
                reason: reason.map(str::to_owned),
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
        eprintln!("webhook: could not write the audit log: {error}");
    }
}

#[cfg(test)]
pub(crate) mod tests;
