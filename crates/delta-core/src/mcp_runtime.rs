//! Rust-owned MCP execution bridge.
//!
//! Configuration remains in McpStore. This module owns live MCP transports,
//! tool discovery, dynamic CapabilityHost registration, and process/session
//! lifecycle. MCP tools never bypass normal Delta policy/approval execution.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::capability::{
    CapabilityControl, CapabilityGrants, CapabilityHost, CapabilityJob, CapabilityRegistration,
    CapabilityResult, CapabilityRunner,
};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

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

fn parse_rpc_payload(text: &str, id: u64) -> Result<Value, String> {
    let target = json!(id);
    let parse_candidate = |candidate: &str| -> Option<Value> {
        serde_json::from_str::<Value>(candidate)
            .ok()
            .filter(|value| value.get("id").is_some_and(|value_id| value_id == &target))
    };

    let value = if let Some(value) = parse_candidate(text.trim()) {
        value
    } else {
        text.lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .filter_map(|line| parse_candidate(line.trim()))
            .next()
            .ok_or_else(|| {
                "MCP response did not contain the matching JSON-RPC result".to_string()
            })?
    };
    if let Some(error) = json_rpc_error(&value) {
        return Err(error);
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "MCP response is missing result".to_string())
}

struct StdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: Receiver<String>,
    stderr: Receiver<String>,
    next_id: u64,
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

        let mut child = process
            .spawn()
            .map_err(|error| format!("start MCP server: {error}"))?;
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

        let (stdout_tx, stdout_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("delta-mcp-stdio-out".to_string())
            .spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if stdout_tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("start MCP stdout reader: {error}"))?;

        let (stderr_tx, stderr_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("delta-mcp-stdio-err".to_string())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if stderr_tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("start MCP stderr reader: {error}"))?;

        let mut client = Self {
            child,
            stdin,
            stdout: stdout_rx,
            stderr: stderr_rx,
            next_id: 1,
        };
        client.initialize()?;
        let tools = client.list_tools()?;
        Ok((client, tools))
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
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

    fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        control: Option<&CapabilityControl>,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.write_value(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;

        let deadline = Instant::now() + timeout;
        loop {
            if control.is_some_and(CapabilityControl::is_cancelled) {
                self.stop();
                return Err("cancelled".to_string());
            }
            if Instant::now() >= deadline {
                self.stop();
                return Err(format!("MCP request timed out: {method}"));
            }
            match self.stdout.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    let Ok(value) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if self.respond_server_request(&value)? {
                        continue;
                    }
                    if value.get("id") != Some(&json!(id)) {
                        continue;
                    }
                    if let Some(error) = json_rpc_error(&value) {
                        return Err(error);
                    }
                    return value
                        .get("result")
                        .cloned()
                        .ok_or_else(|| "MCP response is missing result".to_string());
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    let mut stderr = Vec::new();
                    while let Ok(line) = self.stderr.try_recv() {
                        stderr.push(line);
                    }
                    return Err(if stderr.is_empty() {
                        "MCP server exited".to_string()
                    } else {
                        format!("MCP server exited: {}", stderr.join(" | "))
                    });
                }
            }
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
                "protocolVersion": MCP_PROTOCOL_VERSION,
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
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor
                .as_ref()
                .map(|cursor| json!({"cursor": cursor}))
                .unwrap_or_else(|| json!({}));
            let result = self.request("tools/list", params, Duration::from_secs(20), None)?;
            tools.extend(
                result
                    .get("tools")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|value| !value.is_empty());
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
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
        };
        let init = client.request(
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"roots": {"listChanged": false}},
                "clientInfo": {"name": "Delta", "version": "0.1.0"}
            }),
            Duration::from_secs(20),
        )?;
        *client.protocol_version.lock().unwrap() = init
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(str::to_string);
        client.notify(
            "notifications/initialized",
            json!({}),
            Duration::from_secs(10),
        )?;
        let tools = client.list_tools()?;
        Ok((client, tools))
    }

    fn request_builder(&self, timeout: Duration) -> ureq::Request {
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
        if let Some(value) = self.session_id.lock().unwrap().as_deref() {
            request = request.set("Mcp-Session-Id", value);
        }
        if let Some(value) = self.protocol_version.lock().unwrap().as_deref() {
            request = request.set("MCP-Protocol-Version", value);
        }
        request
    }

    fn send(&self, payload: Value, timeout: Duration, id: Option<u64>) -> Result<Value, String> {
        let response =
            self.request_builder(timeout)
                .send_json(payload)
                .map_err(|error| match error {
                    ureq::Error::Status(code, response) => {
                        let body = response.into_string().unwrap_or_default();
                        format!("MCP HTTP {code}: {body}")
                    }
                    ureq::Error::Transport(error) => format!("MCP HTTP transport: {error}"),
                })?;
        if let Some(value) = response.header("Mcp-Session-Id") {
            *self.session_id.lock().unwrap() = Some(value.to_string());
        }
        if id.is_none() {
            return Ok(json!({}));
        }
        let body = response
            .into_string()
            .map_err(|error| format!("read MCP HTTP response: {error}"))?;
        parse_rpc_payload(&body, id.expect("request id"))
    }

    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.send(
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
            timeout,
            Some(id),
        )
    }

    fn notify(&self, method: &str, params: Value, timeout: Duration) -> Result<(), String> {
        self.send(
            json!({"jsonrpc": "2.0", "method": method, "params": params}),
            timeout,
            None,
        )
        .map(|_| ())
    }

    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor
                .as_ref()
                .map(|cursor| json!({"cursor": cursor}))
                .unwrap_or_else(|| json!({}));
            let result = self.request("tools/list", params, Duration::from_secs(20))?;
            tools.extend(
                result
                    .get("tools")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|value| !value.is_empty());
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
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
            let metadata = json!({
                "category": "mcp",
                "server": name,
                "remote_tool": remote_name,
                "risk_level": if requires_approval { "medium" } else { "low" },
                "requires_approval": requires_approval,
                "capabilities": ["mcp"],
            });
            let registration = CapabilityRegistration {
                capability_id: format!("mcp.{name}.{remote_name}"),
                tool_name: local_name.clone(),
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
}
