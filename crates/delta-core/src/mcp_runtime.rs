//! Rust-owned MCP execution bridge.
//!
//! Configuration remains in McpStore. This module owns live MCP transports,
//! tool discovery, dynamic CapabilityHost registration, and process/session
//! lifecycle. MCP tools never bypass normal Delta policy/approval execution.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{json, Value};

use crate::capability::process_io::{spawn_line_reader, DropWhenFull};
use crate::capability::process_tree::ProcessTree;
use crate::capability::{
    CapabilityControl, CapabilityGrants, CapabilityHost, CapabilityJob, CapabilityRegistration,
    CapabilityResult, CapabilityRunner,
};

/// Protocol revision Delta speaks natively: every request carries its own `_meta`, and no
/// `initialize` handshake is sent.
const MCP_PROTOCOL_VERSION: &str = "2026-07-28";
/// Revision used only for servers that do not speak the modern protocol. It requires `initialize`.
const MCP_LEGACY_PROTOCOL_VERSION: &str = "2025-06-18";
/// How long the era probe waits for `server/discover` before it treats the server as legacy.
const MCP_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.octets()[0] == 0
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PinnedTarget {
    request_url: String,
    host_header: String,
    server_name: String,
}

fn resolve_target<F>(raw: &str, mut resolve: F) -> Result<PinnedTarget, String>
where
    F: FnMut(&str, u16) -> Result<Vec<IpAddr>, String>,
{
    let mut parsed = url::Url::parse(raw).map_err(|error| format!("invalid MCP URL: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("HTTP MCP requires an http(s) URL".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "HTTP MCP URL requires a host".to_string())?
        .to_string();
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return Err("HTTP MCP cannot target localhost".to_string());
    }
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "HTTP MCP URL requires a valid port".to_string())?;
    let addresses = resolve(&host, port)?;
    if addresses.is_empty() {
        return Err("HTTP MCP host did not resolve".to_string());
    }
    if addresses.iter().copied().any(is_private_ip) {
        return Err("HTTP MCP cannot target local or private network addresses".to_string());
    }

    let pinned_ip = addresses[0];
    let host_text = match parsed.host() {
        Some(url::Host::Ipv6(value)) => format!("[{value}]"),
        _ => host.clone(),
    };
    let host_header = match parsed.port() {
        Some(port) => format!("{host_text}:{port}"),
        None => host_text,
    };
    parsed
        .set_host(Some(&pinned_ip.to_string()))
        .map_err(|_| "HTTP MCP could not pin the validated address".to_string())?;

    Ok(PinnedTarget {
        request_url: parsed.to_string(),
        host_header,
        server_name: host,
    })
}

fn resolve_mcp_target(raw: &str) -> Result<PinnedTarget, String> {
    resolve_target(raw, |host, port| {
        (host, port)
            .to_socket_addrs()
            .map_err(|error| format!("resolve MCP host: {error}"))
            .map(|items| items.map(|address| address.ip()).collect())
    })
}

#[derive(Debug)]
struct PinnedTls {
    server_name: String,
    config: Arc<rustls::ClientConfig>,
}

impl PinnedTls {
    fn new(server_name: String) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Self {
            server_name,
            config: Arc::new(config),
        }
    }
}

impl ureq::TlsConnector for PinnedTls {
    fn connect(
        &self,
        _dns_name: &str,
        io: Box<dyn ureq::ReadWrite>,
    ) -> Result<Box<dyn ureq::ReadWrite>, ureq::Error> {
        let server_name = rustls::pki_types::ServerName::try_from(self.server_name.clone())
            .map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("invalid MCP TLS server name: {error}"),
                )
            })?;
        let connection =
            rustls::ClientConnection::new(self.config.clone(), server_name).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("create MCP TLS connection: {error}"),
                )
            })?;
        Ok(Box::new(PinnedStream(rustls::StreamOwned::new(
            connection, io,
        ))))
    }
}

#[derive(Debug)]
struct PinnedStream(rustls::StreamOwned<rustls::ClientConnection, Box<dyn ureq::ReadWrite>>);

impl ureq::ReadWrite for PinnedStream {
    fn socket(&self) -> Option<&TcpStream> {
        self.0.sock.socket()
    }
}

impl Read for PinnedStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Write for PinnedStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

fn sanitize_segment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn tool_prefix(server: &str) -> String {
    format!("mcp__{}__", sanitize_segment(server))
}

fn json_rpc_error(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    let code = error.get("code").and_then(Value::as_i64);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("MCP request failed");
    Some(match code {
        Some(code) => format!("MCP error {code}: {message}"),
        None => message.to_string(),
    })
}

/// The JSON-RPC response object for request `id`, from a plain JSON body or an SSE body.
fn parse_rpc_object(text: &str, id: u64) -> Result<Value, String> {
    let target = json!(id);
    let parse_candidate = |candidate: &str| -> Option<Value> {
        serde_json::from_str::<Value>(candidate)
            .ok()
            .filter(|value| value.get("id").is_some_and(|value_id| value_id == &target))
    };

    if let Some(value) = parse_candidate(text.trim()) {
        return Ok(value);
    }
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|line| parse_candidate(line.trim()))
        .next()
        .ok_or_else(|| "MCP response did not contain the matching JSON-RPC result".to_string())
}

/// The first JSON-RPC object in a body, whatever its id. Used to read the reply to a probe, which
/// may be an error that does not echo the id.
fn first_rpc_object(text: &str) -> Option<Value> {
    let is_rpc = |value: &Value| value.get("jsonrpc").is_some();
    if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
        if is_rpc(&value) {
            return Some(value);
        }
    }
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(is_rpc)
}

/// The result of one JSON-RPC response, or the error it carries.
fn result_of(response: &Value) -> Result<Value, String> {
    if let Some(error) = json_rpc_error(response) {
        return Err(error);
    }
    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| "MCP response is missing result".to_string())?;
    // Delta declares no input-request capabilities, so a server that asks for input cannot be served.
    if result.get("resultType").and_then(Value::as_str) == Some("input_required") {
        return Err("MCP server requested more input, which Delta does not support".to_string());
    }
    Ok(result)
}

/// Protocol era of one server. A server's era is fixed for the life of its connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpEra {
    Modern,
    Legacy,
}

/// Classify the reply to a `server/discover` probe.
///
/// A `DiscoverResult` that lists the modern version means a modern server. A recognized modern
/// error means a modern server that refuses this version, which is a failure and not a fallback.
/// Any other reply means a legacy server that must be reached through `initialize`.
fn classify_discover(response: &Value) -> Result<McpEra, String> {
    if response.get("error").is_some() {
        let code = response
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_i64);
        if matches!(code, Some(-32022..=-32020)) {
            return Err(json_rpc_error(response).unwrap_or_default());
        }
        return Ok(McpEra::Legacy);
    }
    match response
        .get("result")
        .and_then(|result| result.get("supportedVersions"))
        .and_then(Value::as_array)
    {
        Some(versions)
            if versions
                .iter()
                .any(|item| item.as_str() == Some(MCP_PROTOCOL_VERSION)) =>
        {
            Ok(McpEra::Modern)
        }
        Some(_) => Err(format!(
            "MCP server does not support protocol version {MCP_PROTOCOL_VERSION}"
        )),
        None => Ok(McpEra::Legacy),
    }
}

/// Add the per-request protocol fields that every modern request must carry.
fn modern_params(params: Value) -> Value {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": MCP_PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "Delta", "version": "0.1.0"},
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    params
}

/// The `Mcp-Name` header value for a name. Anything outside printable ASCII is sent Base64 encoded
/// with the sentinel form the transport defines.
fn mcp_name_header(name: &str) -> String {
    if name.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
        name.to_string()
    } else {
        format!(
            "=?base64?{}?=",
            base64::engine::general_purpose::STANDARD.encode(name)
        )
    }
}

/// The `Mcp-Name` source of a request. Only `tools/call` carries one in this client.
fn mcp_name_of(method: &str, params: &Value) -> Option<String> {
    if method != "tools/call" {
        return None;
    }
    params
        .get("name")
        .and_then(Value::as_str)
        .map(mcp_name_header)
}

/// A server may page its tool list. A server that never stops paging, or lists far more than any
/// agent could use, would otherwise hold this thread and its memory forever.
const MAX_TOOL_PAGES: usize = 50;
const MAX_MCP_TOOLS: usize = 1000;

fn collect_tool_pages(
    mut fetch_page: impl FnMut(Value) -> Result<Value, String>,
) -> Result<Vec<Value>, String> {
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_TOOL_PAGES {
        let params = cursor
            .as_ref()
            .map(|cursor| json!({"cursor": cursor}))
            .unwrap_or_else(|| json!({}));
        let result = fetch_page(params)?;
        tools.extend(
            result
                .get("tools")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        if tools.len() > MAX_MCP_TOOLS {
            return Err(format!("MCP server lists more than {MAX_MCP_TOOLS} tools"));
        }
        cursor = result
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty());
        if cursor.is_none() {
            return Ok(tools);
        }
    }
    Err(format!(
        "MCP server kept paging its tool list past {MAX_TOOL_PAGES} pages"
    ))
}

/// Lines a stdio server may have waiting before it is made to wait on its pipe (stdout) or before
/// further diagnostics are dropped (stderr), and the longest line either keeps.
const STDIO_QUEUED_LINES: usize = 4096;
const STDIO_MAX_LINE_BYTES: usize = 32 * 1024 * 1024;

struct StdioClient {
    child: Child,
    /// The server and everything it started, so ending the client ends all of it.
    tree: ProcessTree,
    stdin: ChildStdin,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
    next_id: u64,
    /// True once the probe finds a modern server. Modern requests carry `_meta` on every call.
    is_modern: bool,
}

/// Why a stdio request did not produce a reply.
enum ReplyFailure {
    Cancelled,
    TimedOut,
    Exited(String),
}

impl StdioClient {
    fn connect(config: &Value) -> Result<(Self, Vec<Value>), String> {
        let command = config
            .get("command")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "stdio MCP requires command".to_string())?;
        let args = config
            .get("args")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let mut process = Command::new(command);
        process
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();

        for name in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "SystemRoot",
            "WINDIR",
            "TEMP",
            "TMP",
        ] {
            if let Some(value) = std::env::var_os(name) {
                process.env(name, value);
            }
        }
        if let Some(env) = config.get("env").and_then(Value::as_object) {
            for (key, value) in env {
                if let Some(value) = value.as_str() {
                    process.env(key, value);
                }
            }
        }
        if let Some(cwd) = config.get("cwd").and_then(Value::as_str) {
            process.current_dir(cwd);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(0x0800_0000);
        }
        ProcessTree::prepare(&mut process);

        let mut child = process
            .spawn()
            .map_err(|error| format!("start MCP server: {error}"))?;
        let tree = ProcessTree::attach(&child);
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "MCP server stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "MCP server stdout unavailable".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "MCP server stderr unavailable".to_string())?;

        let (stdout_tx, stdout_rx) = mpsc::sync_channel(STDIO_QUEUED_LINES);
        spawn_line_reader(
            "delta-mcp-stdio-out",
            stdout,
            STDIO_MAX_LINE_BYTES,
            stdout_tx,
            |line| line,
        )
        .map_err(|error| format!("start MCP stdout reader: {error}"))?;

        let (stderr_tx, stderr_rx) = mpsc::sync_channel(STDIO_QUEUED_LINES);
        spawn_line_reader(
            "delta-mcp-stdio-err",
            stderr,
            STDIO_MAX_LINE_BYTES,
            DropWhenFull(stderr_tx),
            |line| line,
        )
        .map_err(|error| format!("start MCP stderr reader: {error}"))?;

        let mut client = Self {
            child,
            tree,
            stdin,
            stdout: stdout_rx,
            stderr: stderr_rx,
            next_id: 1,
            is_modern: false,
        };
        if client.probe_era()? == McpEra::Legacy {
            client.initialize()?;
        }
        let tools = client.list_tools()?;
        Ok((client, tools))
    }

    fn stop(&mut self) {
        self.tree.kill(&mut self.child);
        let _ = self.child.wait();
    }

    fn write_value(&mut self, value: &Value) -> Result<(), String> {
        let payload = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        self.stdin
            .write_all(&payload)
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("write MCP request: {error}"))
    }

    fn respond_server_request(&mut self, value: &Value) -> Result<bool, String> {
        let Some(id) = value.get("id").cloned() else {
            return Ok(false);
        };
        let Some(method) = value.get("method").and_then(Value::as_str) else {
            return Ok(false);
        };
        let result = match method {
            "ping" => json!({}),
            "roots/list" => json!({"roots": []}),
            _ => {
                return self
                    .write_value(&json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32601, "message": "Client method not supported"}
                    }))
                    .map(|_| true)
            }
        };
        self.write_value(&json!({"jsonrpc": "2.0", "id": id, "result": result}))?;
        Ok(true)
    }

    /// Wait for the full response object to request `id`, answering server requests on the way.
    fn await_reply(
        &mut self,
        id: u64,
        timeout: Duration,
        control: Option<&CapabilityControl>,
    ) -> Result<Value, ReplyFailure> {
        let deadline = Instant::now() + timeout;
        loop {
            if control.is_some_and(CapabilityControl::is_cancelled) {
                return Err(ReplyFailure::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(ReplyFailure::TimedOut);
            }
            match self.stdout.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    let Ok(value) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if self.respond_server_request(&value).unwrap_or(false) {
                        continue;
                    }
                    if value.get("id") == Some(&json!(id)) {
                        return Ok(value);
                    }
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    let mut stderr = Vec::new();
                    while let Ok(line) = self.stderr.try_recv() {
                        stderr.push(line);
                    }
                    return Err(ReplyFailure::Exited(if stderr.is_empty() {
                        "MCP server exited".to_string()
                    } else {
                        format!("MCP server exited: {}", stderr.join(" | "))
                    }));
                }
            }
        }
    }

    /// Send one request and return its full response object. A timeout or cancel ends the child.
    fn request_object(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        control: Option<&CapabilityControl>,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let params = if self.is_modern {
            modern_params(params)
        } else {
            params
        };
        self.write_value(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        match self.await_reply(id, timeout, control) {
            Ok(value) => Ok(value),
            Err(ReplyFailure::Cancelled) => {
                self.stop();
                Err("cancelled".to_string())
            }
            Err(ReplyFailure::TimedOut) => {
                self.stop();
                Err(format!("MCP request timed out: {method}"))
            }
            Err(ReplyFailure::Exited(message)) => Err(message),
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        control: Option<&CapabilityControl>,
    ) -> Result<Value, String> {
        let response = self.request_object(method, params, timeout, control)?;
        result_of(&response)
    }

    /// Decide the server's era with `server/discover`.
    ///
    /// The probe must not end the child on timeout: a timed-out probe means a legacy server, and
    /// the same process then has to take the `initialize` handshake.
    fn probe_era(&mut self) -> Result<McpEra, String> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.write_value(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "server/discover",
            "params": modern_params(json!({})),
        }))?;
        match self.await_reply(id, MCP_PROBE_TIMEOUT, None) {
            Ok(response) => {
                let era = classify_discover(&response)?;
                self.is_modern = era == McpEra::Modern;
                Ok(era)
            }
            Err(ReplyFailure::TimedOut) => Ok(McpEra::Legacy),
            Err(ReplyFailure::Cancelled) => Err("cancelled".to_string()),
            Err(ReplyFailure::Exited(message)) => Err(message),
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.write_value(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
    }

    fn initialize(&mut self) -> Result<(), String> {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": MCP_LEGACY_PROTOCOL_VERSION,
                "capabilities": {"roots": {"listChanged": false}},
                "clientInfo": {"name": "Delta", "version": "0.1.0"}
            }),
            Duration::from_secs(20),
            None,
        )?;
        if result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err("MCP initialize response missing protocolVersion".to_string());
        }
        self.notify("notifications/initialized", json!({}))
    }

    fn list_tools(&mut self) -> Result<Vec<Value>, String> {
        collect_tool_pages(|params| {
            self.request("tools/list", params, Duration::from_secs(20), None)
        })
    }

    fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
        timeout: Duration,
        control: &CapabilityControl,
    ) -> Result<Value, String> {
        self.request(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
            timeout,
            Some(control),
        )
    }
}

impl Drop for StdioClient {
    fn drop(&mut self) {
        self.stop();
    }
}

struct HttpClient {
    url: String,
    host_header: String,
    headers: BTreeMap<String, String>,
    agent: ureq::Agent,
    session_id: Mutex<Option<String>>,
    protocol_version: Mutex<Option<String>>,
    next_id: AtomicU64,
    /// True for a modern server. Modern requests carry `_meta` and the per-request headers, and
    /// they never use `Mcp-Session-Id`.
    is_modern: AtomicBool,
}

impl HttpClient {
    fn connect(config: &Value) -> Result<(Self, Vec<Value>), String> {
        let url = config
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| "HTTP MCP requires an http(s) URL".to_string())?;
        let target = resolve_mcp_target(url)?;
        if config.get("auth").and_then(Value::as_str) == Some("oauth")
            && config
                .get("headers")
                .and_then(Value::as_object)
                .and_then(|headers| headers.get("Authorization"))
                .and_then(Value::as_str)
                .is_none()
        {
            return Err("MCP OAuth requires an access token before connecting".to_string());
        }

        let headers = config
            .get("headers")
            .and_then(Value::as_object)
            .map(|values| {
                values
                    .iter()
                    .filter(|(key, _)| !key.eq_ignore_ascii_case("host"))
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_string()))
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let mut builder = ureq::AgentBuilder::new().redirects(0);
        if url.starts_with("https://") {
            builder = builder.tls_connector(Arc::new(PinnedTls::new(target.server_name.clone())));
        }
        let client = Self {
            url: target.request_url,
            host_header: target.host_header,
            headers,
            agent: builder.build(),
            session_id: Mutex::new(None),
            protocol_version: Mutex::new(None),
            next_id: AtomicU64::new(1),
            is_modern: AtomicBool::new(true),
        };
        let tools = client.establish()?;
        Ok((client, tools))
    }

    /// Settle the server's era, complete the legacy handshake if one is needed, and list tools.
    fn establish(&self) -> Result<Vec<Value>, String> {
        if self.probe_era()? == McpEra::Legacy {
            self.is_modern.store(false, Ordering::SeqCst);
            self.initialize()?;
        }
        self.list_tools()
    }

    /// Decide the server's era with `server/discover`, sent as a modern request.
    ///
    /// A recognized modern error from an HTTP server arrives in the body of a 4xx response, so the
    /// body is read whatever the status. Any other reply means a legacy server.
    fn probe_era(&self) -> Result<McpEra, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "server/discover",
            "params": modern_params(json!({})),
        });
        let (_, body) = self.post(&payload, MCP_PROBE_TIMEOUT, "server/discover", None)?;
        match first_rpc_object(&body) {
            Some(response) => classify_discover(&response),
            None => Ok(McpEra::Legacy),
        }
    }

    /// Legacy handshake, used only when the probe found a legacy server.
    fn initialize(&self) -> Result<(), String> {
        let init = self.request(
            "initialize",
            json!({
                "protocolVersion": MCP_LEGACY_PROTOCOL_VERSION,
                "capabilities": {"roots": {"listChanged": false}},
                "clientInfo": {"name": "Delta", "version": "0.1.0"}
            }),
            Duration::from_secs(20),
        )?;
        *self.protocol_version.lock().unwrap() = init
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.notify(
            "notifications/initialized",
            json!({}),
            Duration::from_secs(10),
        )
    }

    fn request_builder(
        &self,
        timeout: Duration,
        method: &str,
        name: Option<&str>,
        is_modern: bool,
    ) -> ureq::Request {
        let mut request = self
            .agent
            .post(&self.url)
            .timeout(timeout)
            .set("Accept", "application/json, text/event-stream")
            .set("Content-Type", "application/json");
        for (key, value) in &self.headers {
            request = request.set(key, value);
        }
        request = request.set("Host", &self.host_header);
        if is_modern {
            request = request
                .set("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
                .set("Mcp-Method", method);
            if let Some(name) = name {
                request = request.set("Mcp-Name", name);
            }
        } else {
            if let Some(value) = self.session_id.lock().unwrap().as_deref() {
                request = request.set("Mcp-Session-Id", value);
            }
            if let Some(value) = self.protocol_version.lock().unwrap().as_deref() {
                request = request.set("MCP-Protocol-Version", value);
            }
        }
        request
    }

    /// Post one message and return the status and body, whatever the status is. A 4xx body can
    /// carry a modern JSON-RPC error that the caller needs to read.
    fn post(
        &self,
        payload: &Value,
        timeout: Duration,
        method: &str,
        name: Option<&str>,
    ) -> Result<(u16, String), String> {
        let is_modern = self.is_modern.load(Ordering::SeqCst);
        let response = match self
            .request_builder(timeout, method, name, is_modern)
            .send_json(payload.clone())
        {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(error)) => {
                return Err(format!("MCP HTTP transport: {error}"));
            }
        };
        let status = response.status();
        if !is_modern {
            if let Some(value) = response.header("Mcp-Session-Id") {
                *self.session_id.lock().unwrap() = Some(value.to_string());
            }
        }
        let body = response
            .into_string()
            .map_err(|error| format!("read MCP HTTP response: {error}"))?;
        Ok((status, body))
    }

    fn send(
        &self,
        payload: Value,
        timeout: Duration,
        id: Option<u64>,
        method: &str,
        name: Option<&str>,
    ) -> Result<Value, String> {
        let (status, body) = self.post(&payload, timeout, method, name)?;
        if status >= 400 {
            return Err(format!("MCP HTTP {status}: {body}"));
        }
        match id {
            None => Ok(json!({})),
            Some(id) => parse_rpc_object(&body, id),
        }
    }

    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let params = if self.is_modern.load(Ordering::SeqCst) {
            modern_params(params)
        } else {
            params
        };
        let name = mcp_name_of(method, &params);
        let response = self.send(
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            timeout,
            Some(id),
            method,
            name.as_deref(),
        )?;
        result_of(&response)
    }

    fn notify(&self, method: &str, params: Value, timeout: Duration) -> Result<(), String> {
        self.send(
            json!({"jsonrpc": "2.0", "method": method, "params": params}),
            timeout,
            None,
            method,
            None,
        )
        .map(|_| ())
    }

    fn list_tools(&self) -> Result<Vec<Value>, String> {
        collect_tool_pages(|params| self.request("tools/list", params, Duration::from_secs(20)))
    }

    fn call_tool(&self, name: &str, arguments: Value, timeout: Duration) -> Result<Value, String> {
        self.request(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
            timeout,
        )
    }
}

enum SessionKind {
    Stdio(Mutex<StdioClient>),
    Http(HttpClient),
}

struct ConnectedSession {
    kind: SessionKind,
    tools: Vec<Value>,
}

impl ConnectedSession {
    fn connect(config: &Value) -> Result<Self, String> {
        if config.get("url").is_some()
            || config
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| matches!(value, "http" | "https" | "streamable-http"))
        {
            let (client, tools) = HttpClient::connect(config)?;
            Ok(Self {
                kind: SessionKind::Http(client),
                tools,
            })
        } else {
            let (client, tools) = StdioClient::connect(config)?;
            Ok(Self {
                kind: SessionKind::Stdio(Mutex::new(client)),
                tools,
            })
        }
    }

    fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
        control: &CapabilityControl,
    ) -> Result<Value, String> {
        if control.is_cancelled() {
            return Err("cancelled".to_string());
        }
        match &self.kind {
            SessionKind::Stdio(client) => client
                .lock()
                .map_err(|_| "MCP stdio lock poisoned".to_string())?
                .call_tool(name, arguments, timeout, control),
            SessionKind::Http(client) => {
                let result = client.call_tool(name, arguments, timeout)?;
                if control.is_cancelled() {
                    Err("cancelled".to_string())
                } else {
                    Ok(result)
                }
            }
        }
    }
}

struct McpToolRunner {
    session: Arc<ConnectedSession>,
    remote_tool: String,
}

impl CapabilityRunner for McpToolRunner {
    fn run(&self, job: &CapabilityJob, control: &CapabilityControl) -> CapabilityResult {
        if control.is_cancelled() {
            return CapabilityResult::cancelled(&job.job_id);
        }
        match self.session.call_tool(
            &self.remote_tool,
            job.arguments.clone(),
            Duration::from_secs(job.timeout_secs.max(1)),
            control,
        ) {
            Ok(value) => CapabilityResult::completed(&job.job_id, value),
            Err(error) if error == "cancelled" => CapabilityResult::cancelled(&job.job_id),
            Err(error) => CapabilityResult::failed(&job.job_id, &error, Some("mcp_call")),
        }
    }
}

#[derive(Default)]
pub struct McpRuntime {
    sessions: Mutex<BTreeMap<String, Arc<ConnectedSession>>>,
}

impl McpRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connect(&self, name: &str, config: &Value, host: &CapabilityHost) -> Value {
        let prefix = tool_prefix(name);
        self.disconnect(name, host);

        let session = match ConnectedSession::connect(config) {
            Ok(session) => Arc::new(session),
            Err(error) => {
                return json!({
                    "ok": false,
                    "available": false,
                    "started": false,
                    "status": if error.contains("OAuth") { "needs_auth" } else { "error" },
                    "error": error,
                })
            }
        };

        let requires_approval = config
            .get("requires_approval")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut local_names = BTreeSet::new();
        let mut public_tools = Vec::new();

        for tool in &session.tools {
            let Some(remote_name) = tool.get("name").and_then(Value::as_str) else {
                continue;
            };
            let segment = sanitize_segment(remote_name);
            if segment.is_empty() {
                continue;
            }
            let local_name = format!("{prefix}{segment}");
            if !local_names.insert(local_name.clone()) {
                host.unregister_prefix(&prefix);
                return json!({"ok": false, "error": "MCP tool names collide after normalization"});
            }
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(remote_name)
                .to_string();
            let parameters = tool
                .get("inputSchema")
                .cloned()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            // A remote server is a fixed service at its URL; a local server is outside Delta's view
            // (ADR-0061).
            let remote_url = config.get("url").and_then(Value::as_str);
            let metadata = json!({
                "category": "mcp",
                "server": name,
                "remote_tool": remote_name,
                "risk_level": if requires_approval { "medium" } else { "low" },
                "requires_approval": requires_approval,
                "capabilities": ["mcp"],
                "destination": if remote_url.is_some() { "fixed_public" } else { "local_process" },
                "destination_url": remote_url,
            });
            let registration = CapabilityRegistration {
                capability_id: format!("mcp.{name}.{remote_name}"),
                tool_name: local_name.clone(),
                side_effect: crate::skills::SideEffectClass::ExternalWrite,
                description: description.clone(),
                parameters: parameters.clone(),
                metadata,
                grants: CapabilityGrants::default(),
                workspace_write: false,
                runner: Arc::new(McpToolRunner {
                    session: session.clone(),
                    remote_tool: remote_name.to_string(),
                }),
            };
            if let Err(error) = host.register(registration) {
                host.unregister_prefix(&prefix);
                return json!({"ok": false, "error": error});
            }
            public_tools.push(json!({
                "name": local_name,
                "remote_name": remote_name,
                "description": description,
                "inputSchema": parameters,
            }));
        }

        self.sessions
            .lock()
            .unwrap()
            .insert(name.to_string(), session);
        json!({
            "ok": true,
            "available": true,
            "started": true,
            "status": "connected",
            "tool_count": public_tools.len(),
            "tools": public_tools,
        })
    }

    pub fn disconnect(&self, name: &str, host: &CapabilityHost) -> bool {
        host.unregister_prefix(&tool_prefix(name));
        self.sessions.lock().unwrap().remove(name).is_some()
    }

    pub fn disconnect_all(&self, host: &CapabilityHost) {
        let names = self
            .sessions
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for name in names {
            self.disconnect(&name, host);
        }
    }

    pub fn tools(&self, name: &str) -> Value {
        let sessions = self.sessions.lock().unwrap();
        let Some(session) = sessions.get(name) else {
            return json!({"ok": false, "available": false, "tools": [], "error": "MCP server is not connected"});
        };
        let prefix = tool_prefix(name);
        let tools = session
            .tools
            .iter()
            .filter_map(|tool| {
                let remote = tool.get("name").and_then(Value::as_str)?;
                Some(json!({
                    "name": format!("{}{}", prefix, sanitize_segment(remote)),
                    "remote_name": remote,
                    "description": tool.get("description").cloned().unwrap_or(Value::Null),
                    "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                }))
            })
            .collect::<Vec<_>>();
        json!({"ok": true, "available": true, "tools": tools})
    }

    pub fn decorate_rows(&self, rows: &mut [Value]) {
        let sessions = self.sessions.lock().unwrap();
        for row in rows {
            let Some(name) = row.get("name").and_then(Value::as_str) else {
                continue;
            };
            if let Some(session) = sessions.get(name) {
                row["status"] = Value::String("connected".to_string());
                row["tool_count"] = json!(session.tools.len());
                row["last_error"] = Value::Null;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};

    use super::*;

    #[test]
    fn mcp_target_pins_address() {
        let target = resolve_target("https://example.com:8443/mcp?x=1", |host, port| {
            assert_eq!(host, "example.com");
            assert_eq!(port, 8443);
            Ok(vec!["93.184.216.34".parse().unwrap()])
        })
        .unwrap();

        assert_eq!(target.request_url, "https://93.184.216.34:8443/mcp?x=1");
        assert_eq!(target.host_header, "example.com:8443");
        assert_eq!(target.server_name, "example.com");
    }

    #[test]
    fn mcp_target_rejects_private_answer() {
        let error = resolve_target("https://example.com/mcp", |_, _| {
            Ok(vec!["127.0.0.1".parse().unwrap()])
        })
        .unwrap_err();

        assert!(error.contains("private"));
    }

    #[test]
    fn mcp_target_rejects_split_answer() {
        let error = resolve_target("https://example.com/mcp", |_, _| {
            Ok(vec![
                "93.184.216.34".parse().unwrap(),
                "10.0.0.1".parse().unwrap(),
            ])
        })
        .unwrap_err();

        assert!(error.contains("private"));
    }

    #[test]
    fn modern_params_carry_the_per_request_protocol_fields() {
        let params = modern_params(json!({"name": "search"}));
        assert_eq!(params["name"], "search");
        assert_eq!(
            params["_meta"]["io.modelcontextprotocol/protocolVersion"],
            MCP_PROTOCOL_VERSION
        );
        assert_eq!(
            params["_meta"]["io.modelcontextprotocol/clientCapabilities"],
            json!({})
        );
        assert_eq!(
            params["_meta"]["io.modelcontextprotocol/clientInfo"]["name"],
            "Delta"
        );
    }

    #[test]
    fn discover_reply_selects_the_era() {
        let modern = json!({"jsonrpc": "2.0", "id": 1, "result": {"resultType": "complete", "supportedVersions": [MCP_PROTOCOL_VERSION]}});
        assert_eq!(classify_discover(&modern), Ok(McpEra::Modern));

        // A recognized modern error is a modern server that refuses this version: no fallback.
        let refused = json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32022, "message": "Unsupported protocol version"}});
        assert!(classify_discover(&refused).unwrap_err().contains("-32022"));

        // Any other error is a legacy server that does not know server/discover.
        let legacy = json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32601, "message": "Method not found"}});
        assert_eq!(classify_discover(&legacy), Ok(McpEra::Legacy));
    }

    #[test]
    fn discover_without_the_modern_version_is_refused() {
        let old =
            json!({"jsonrpc": "2.0", "id": 1, "result": {"supportedVersions": ["2025-11-25"]}});
        assert!(classify_discover(&old)
            .unwrap_err()
            .contains(MCP_PROTOCOL_VERSION));
    }

    #[test]
    fn input_required_results_are_refused() {
        let asked = json!({"jsonrpc": "2.0", "id": 1, "result": {"resultType": "input_required", "inputRequests": {}}});
        assert!(result_of(&asked).unwrap_err().contains("more input"));
        let complete =
            json!({"jsonrpc": "2.0", "id": 1, "result": {"resultType": "complete", "tools": []}});
        assert!(result_of(&complete).is_ok());
    }

    #[test]
    fn mcp_name_header_encodes_only_what_is_not_plain_ascii() {
        assert_eq!(mcp_name_header("search_docs"), "search_docs");
        let encoded = mcp_name_header("搜索");
        let payload = encoded
            .strip_prefix("=?base64?")
            .and_then(|rest| rest.strip_suffix("?="))
            .unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "搜索");
        assert_eq!(mcp_name_of("tools/list", &json!({})), None);
    }

    #[test]
    fn first_rpc_object_reads_plain_and_sse_bodies() {
        let plain = r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32022,"message":"x"}}"#;
        assert!(first_rpc_object(plain).is_some());
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        assert!(first_rpc_object(sse).is_some());
        assert!(first_rpc_object("<html>not mcp</html>").is_none());
    }

    /// A loopback HTTP server. It hands each request's raw head and parsed body to the test before
    /// it replies, so the test can read the request without racing the client.
    fn spawn_fake_server(
        respond: impl Fn(&Value) -> (u16, String) + Send + 'static,
    ) -> (u16, Receiver<(String, Value)>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    head.push_str(&line);
                }
                let mut body = vec![0u8; length];
                reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                let (status, reply) = respond(&request);
                sender.send((head, request)).unwrap();
                let response = format!(
                    "HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (port, receiver)
    }

    fn loopback_client(port: u16) -> HttpClient {
        HttpClient {
            url: format!("http://127.0.0.1:{port}/mcp"),
            host_header: format!("127.0.0.1:{port}"),
            headers: BTreeMap::new(),
            agent: ureq::AgentBuilder::new().redirects(0).build(),
            session_id: Mutex::new(None),
            protocol_version: Mutex::new(None),
            next_id: AtomicU64::new(1),
            is_modern: AtomicBool::new(true),
        }
    }

    fn rpc_reply(request: &Value, result: Value) -> (u16, String) {
        (
            200,
            json!({"jsonrpc": "2.0", "id": request["id"], "result": result}).to_string(),
        )
    }

    /// Run `work` on a thread and fail the test if it has not finished in 20 seconds, so a client
    /// that never returns cannot hang the suite.
    fn within_twenty_seconds<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        rx.recv_timeout(Duration::from_secs(20))
            .expect("the call did not return within 20 seconds")
    }

    fn paging_server(tools_per_page: usize) -> u16 {
        let (port, requests) = spawn_fake_server(move |request| match request["method"].as_str() {
            Some("server/discover") => rpc_reply(
                request,
                json!({"resultType": "complete", "supportedVersions": [MCP_PROTOCOL_VERSION]}),
            ),
            _ => rpc_reply(
                request,
                json!({
                    "resultType": "complete",
                    "tools": (0..tools_per_page).map(|i| json!({"name": format!("t{i}")})).collect::<Vec<_>>(),
                    "nextCursor": "more",
                }),
            ),
        });
        // Keep receiving so the fake server can go on answering.
        std::thread::spawn(move || for _ in requests {});
        port
    }

    #[test]
    fn a_server_that_never_stops_paging_is_cut_off() {
        let port = paging_server(1);
        let outcome = within_twenty_seconds(move || loopback_client(port).establish());
        let error = outcome.expect_err("an endless listing must be refused");
        assert!(error.contains("pages"), "{error}");
    }

    #[test]
    fn a_server_that_lists_too_many_tools_is_refused() {
        let port = paging_server(MAX_MCP_TOOLS + 1);
        let outcome = within_twenty_seconds(move || loopback_client(port).establish());
        let error = outcome.expect_err("too many tools must be refused");
        assert!(error.contains("more than"), "{error}");
    }

    #[cfg(unix)]
    fn stdio_server(script_body: &str) -> Value {
        let script = format!(
            "while IFS= read -r L; do ID=$(printf '%s' \"$L\" | sed 's/.*\"id\":\\([0-9][0-9]*\\).*/\\1/'); {script_body} done"
        );
        json!({"command": "/bin/sh", "args": ["-c", script]})
    }

    #[cfg(unix)]
    const STDIO_ANSWERS: &str = r#"
        printf '\377 stderr noise\n' >&2
        printf '\377 stdout noise\n'
        case "$L" in
          *server/discover*) printf '{"jsonrpc":"2.0","id":%s,"result":{"resultType":"complete","supportedVersions":["VERSION"]}}\n' "$ID";;
          *tools/list*) printf '{"jsonrpc":"2.0","id":%s,"result":{"resultType":"complete","tools":[{"name":"echo"}]}}\n' "$ID";;
        esac"#;

    #[cfg(unix)]
    #[test]
    fn a_stdio_server_that_prints_bytes_that_are_not_utf8_still_lists_its_tools() {
        let config = stdio_server(&STDIO_ANSWERS.replace("VERSION", MCP_PROTOCOL_VERSION));
        let outcome =
            within_twenty_seconds(move || StdioClient::connect(&config).map(|(_, tools)| tools));
        let tools = outcome.expect("the server answers after its noise");
        assert_eq!(tools[0]["name"], "echo");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dropping_a_stdio_client_ends_the_processes_its_server_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let body = format!(
            "[ -e {pid} ] || {{ sleep 30 & echo $! > {pid}; }}; {}",
            STDIO_ANSWERS.replace("VERSION", MCP_PROTOCOL_VERSION),
            pid = pid_file.display()
        );
        let config = stdio_server(&body);
        let client =
            within_twenty_seconds(move || StdioClient::connect(&config).map(|(client, _)| client))
                .expect("connect");
        let pid: u32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = |pid: u32| {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .map(|stat| {
                    !stat
                        .rsplit(')')
                        .next()
                        .unwrap_or("")
                        .trim_start()
                        .starts_with('Z')
                })
                .unwrap_or(false)
        };
        assert!(
            alive(pid),
            "the server's background process should be running"
        );
        drop(client);
        std::thread::sleep(Duration::from_millis(300));
        assert!(!alive(pid), "process {pid} outlived the client");
    }

    #[test]
    fn modern_http_server_skips_initialize_and_sends_per_request_fields() {
        let (port, requests) = spawn_fake_server(|request| match request["method"].as_str() {
            Some("server/discover") => rpc_reply(
                request,
                json!({"resultType": "complete", "supportedVersions": [MCP_PROTOCOL_VERSION]}),
            ),
            Some("tools/list") => rpc_reply(
                request,
                json!({"resultType": "complete", "tools": [{"name": "search"}]}),
            ),
            _ => (500, String::new()),
        });
        let tools = loopback_client(port).establish().unwrap();
        assert_eq!(tools[0]["name"], "search");

        let sent: Vec<(String, Value)> = requests.try_iter().collect();
        let methods: Vec<&str> = sent
            .iter()
            .map(|(_, body)| body["method"].as_str().unwrap())
            .collect();
        assert_eq!(methods, ["server/discover", "tools/list"]);

        let (headers, body) = &sent[1];
        assert!(headers.contains(&format!("MCP-Protocol-Version: {MCP_PROTOCOL_VERSION}")));
        assert!(headers.contains("Mcp-Method: tools/list"));
        assert!(!headers.to_ascii_lowercase().contains("mcp-session-id"));
        assert_eq!(
            body["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
            MCP_PROTOCOL_VERSION
        );
    }

    #[test]
    fn legacy_http_server_gets_the_initialize_handshake() {
        let (port, requests) = spawn_fake_server(|request| {
            match request["method"].as_str() {
            // A legacy server does not know server/discover and answers with method not found.
            Some("server/discover") => (
                200,
                json!({"jsonrpc": "2.0", "id": request["id"], "error": {"code": -32601, "message": "Method not found"}}).to_string(),
            ),
            Some("initialize") => rpc_reply(request, json!({"protocolVersion": MCP_LEGACY_PROTOCOL_VERSION})),
            Some("notifications/initialized") => (202, String::new()),
            Some("tools/list") => rpc_reply(request, json!({"tools": [{"name": "search"}]})),
            _ => (500, String::new()),
        }
        });
        let tools = loopback_client(port).establish().unwrap();
        assert_eq!(tools[0]["name"], "search");

        let sent: Vec<(String, Value)> = requests.try_iter().collect();
        let methods: Vec<&str> = sent
            .iter()
            .map(|(_, body)| body["method"].as_str().unwrap())
            .collect();
        assert_eq!(
            methods,
            [
                "server/discover",
                "initialize",
                "notifications/initialized",
                "tools/list"
            ]
        );

        let (headers, body) = &sent[3];
        assert!(headers.contains(&format!(
            "MCP-Protocol-Version: {MCP_LEGACY_PROTOCOL_VERSION}"
        )));
        assert!(!headers.contains("Mcp-Method"));
        assert!(body["params"].get("_meta").is_none());
    }

    #[test]
    fn modern_server_refusing_the_version_is_an_error_not_a_fallback() {
        let (port, requests) = spawn_fake_server(|_| {
            (
                400,
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32022, "message": "Unsupported protocol version", "data": {"supported": [MCP_PROTOCOL_VERSION]}}}).to_string(),
            )
        });
        let error = loopback_client(port).establish().unwrap_err();
        assert!(error.contains("-32022"));
        let methods: Vec<String> = requests
            .try_iter()
            .map(|(_, body)| body["method"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(methods, ["server/discover"]);
    }
}
