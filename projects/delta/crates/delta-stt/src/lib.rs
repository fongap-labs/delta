//! A local, offline speech-to-text engine.
//!
//! This crate deliberately has no Tauri, UI, clipboard, or global-shortcut dependency. Hosts
//! own their own UX and permission flows; they use [`Dictation`] for microphone capture,
//! model provisioning, and final transcription.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, Stream, StreamConfig,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};

/// The local voice model: SenseVoice Small (FunAudioLLM), int8, converted for sherpa-onnx. It is
/// built for short dictation: it recognises Mandarin, Cantonese, English, Japanese and Korean
/// with automatic language detection, adds punctuation and writes numbers as digits, and
/// transcribes a sentence in a fraction of a second on an ordinary CPU.
pub const DEFAULT_MODEL_NAME: &str = "SenseVoice Small";
/// Directory (under the host's model directory) that holds the model's files.
pub const DEFAULT_MODEL_DIR: &str = "sensevoice-small-int8-2024-07-17";

/// One file of the model, pinned by size and SHA-256. Whatever arrives must match the pin.
struct ModelAsset {
    file: &'static str,
    bytes: u64,
    sha256: &'static str,
    /// Where the file can be fetched, tried in order. Empty when the file is unpacked from an archive.
    urls: &'static [&'static str],
}

/// The official SenseVoice release archive (sherpa-onnx, int8, 2024-07-17). It holds the two model
/// files in `MODEL_ASSETS`; the app downloads it once, checks its hash, and unpacks those files.
const MODEL_ARCHIVE: ModelAsset = ModelAsset {
    file: "sensevoice-int8-2024-07-17.tar.bz2",
    bytes: 163_002_883,
    sha256: "7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e",
    urls: &["https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"],
};

/// The two files the recognizer loads. Their pins are checked after unpacking from `MODEL_ARCHIVE`.
const MODEL_ASSETS: [ModelAsset; 2] = [
    ModelAsset {
        file: "model.int8.onnx",
        bytes: 239_233_841,
        sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
        urls: &[],
    },
    ModelAsset {
        file: "tokens.txt",
        bytes: 315_894,
        sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
        urls: &[],
    },
];
/// Total download size of the model, in bytes.
pub const DEFAULT_MODEL_BYTES: u64 = MODEL_ASSETS[0].bytes + MODEL_ASSETS[1].bytes;
/// The model reads 16 kHz mono audio.
const SAMPLE_RATE: u32 = 16_000;
/// Files the previous (Whisper) engine left in the model directory; removed once SenseVoice is
/// installed, or with it.
const LEGACY_MODEL_FILES: [&str; 4] = [
    "ggml-base.bin",
    "ggml-base.bin.part",
    "ggml-base.bin.verified",
    "ggml-base.bin.ready",
];

/// Auto-detect a system HTTP(S) proxy for outbound model downloads, WITHOUT persisting it.
/// Order: standard env vars (`HTTPS_PROXY`/`ALL_PROXY`/`HTTP_PROXY` and lower-case), then —
/// on Windows — the per-user WinINET/IE proxy from the registry. Returns a proxy URL ready
/// for `ureq::Proxy::new`, or `None`. The result is used for one download only and is never
/// written back to config or env, so a transient corporate/VPN proxy doesn't become sticky.
fn detect_system_proxy() -> Option<String> {
    for var in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(normalize_proxy_url(v));
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(p) = windows_internet_proxy() {
            return Some(normalize_proxy_url(&p));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(p) = macos_system_proxy() {
            return Some(normalize_proxy_url(&p));
        }
    }
    // Linux has no single desktop-wide proxy store; GNOME's gsettings covers the majority
    // of desktop users. Spawning a console tool is fine here — the download runs on a
    // background worker thread, not the UI process's visible lifetime.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(p) = linux_gnome_proxy() {
            return Some(normalize_proxy_url(&p));
        }
    }
    None
}

/// macOS per-user system proxy via `scutil --proxy`. Prefers the HTTPS entry (the model
/// download is HTTPS) and falls back to the HTTP one; `*Enable : 0` means off.
#[cfg(target_os = "macos")]
fn macos_system_proxy() -> Option<String> {
    let output = std::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // "<key> : <value>" lines inside the dictionary; parse the HTTPS pair, then HTTP.
    let value = |key: &str| -> Option<String> {
        text.lines()
            .find(|line| line.trim_start().starts_with(key))
            .and_then(|line| line.split(':').nth(1))
            .map(|v| v.trim().trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
    };
    let enabled = |key: &str| value(key).is_some_and(|v| v == "1");
    if enabled("HTTPSEnable") {
        if let (Some(host), Some(port)) = (value("HTTPSProxy"), value("HTTPSPort")) {
            return Some(format!("{host}:{port}"));
        }
    }
    if enabled("HTTPEnable") {
        if let (Some(host), Some(port)) = (value("HTTPProxy"), value("HTTPPort")) {
            return Some(format!("{host}:{port}"));
        }
    }
    // SOCKS-only setups still carry traffic, but ureq's Proxy speaks HTTP CONNECT; a
    // socks5:// URL is passed through untouched so ureq can decide.
    if enabled("SOCKSEnable") {
        if let (Some(host), Some(port)) = (value("SOCKSProxy"), value("SOCKSPort")) {
            return Some(format!("socks5://{host}:{port}"));
        }
    }
    None
}

/// GNOME/desktop Linux proxy via `gsettings` (org.gnome.system.proxy). `mode` must be
/// 'manual'; the https:// schema is preferred (the model download is HTTPS), http:// as
/// fallback. Absent gsettings (servers, minimal WMs) yields None — env vars were the
/// portable contract checked before this.
#[cfg(all(unix, not(target_os = "macos")))]
fn linux_gnome_proxy() -> Option<String> {
    let get = |schema: &str, key: &str| -> Option<String> {
        let output = std::process::Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!value.is_empty()).then_some(value)
    };
    // mode is a GTK enum rendered as 'manual' / 'none' (with quotes).
    if get("org.gnome.system.proxy", "mode")?.trim_matches('\'') != "manual" {
        return None;
    }
    let host_of = |schema: &str| {
        let host = get(schema, "host")?.trim_matches('\'').to_string();
        let port = get(schema, "port")?.parse::<u16>().ok()?;
        (!host.is_empty()).then_some(format!("{host}:{port}"))
    };
    host_of("org.gnome.system.proxy.https").or_else(|| host_of("org.gnome.system.proxy.http"))
}

/// Ensure the proxy URL carries a scheme (ureq requires one; a bare `host:port` is the form
/// the Windows registry stores).
fn normalize_proxy_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    }
}

#[cfg(target_os = "windows")]
fn windows_internet_proxy() -> Option<String> {
    use std::process::Command;
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    // GUI processes spawning console tools (reg.exe) pop a visible console window
    // unless suppressed.
    fn quiet(command: &mut Command) -> &mut Command {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW)
    }
    // ProxyEnable == 1 means a system proxy is configured (0 = direct access).
    let enable = quiet(Command::new("reg").args(["query", KEY, "/v", "ProxyEnable"]))
        .output()
        .ok()?;
    if !String::from_utf8_lossy(&enable.stdout).contains("0x1") {
        return None;
    }
    let server = quiet(Command::new("reg").args(["query", KEY, "/v", "ProxyServer"]))
        .output()
        .ok()?;
    let server_str = String::from_utf8_lossy(&server.stdout);
    let line = server_str.lines().find(|l| l.contains("ProxyServer"))?;
    let value = line.rsplit("REG_SZ").next()?.trim();
    let value = value.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if value.is_empty() {
        return None;
    }
    // Multi-scheme form: "http=host:80;https=host:80" — prefer the https= entry (the model
    // download is HTTPS). A bare `host:port` falls through to the first segment.
    let chosen = value
        .split(';')
        .find(|s| s.trim_start().starts_with("https="))
        .map(|s| s.trim().trim_start_matches("https="))
        .or_else(|| value.split(';').next().map(|s| s.trim()))
        .filter(|s| !s.is_empty())?;
    Some(chosen.to_string())
}

/// Build an HTTP agent bound to `proxy`, or an explicit no-proxy direct agent. Direct
/// connections pass `proxy(None)` so ureq 3's default `proxy-from-env` behavior (it reads
/// HTTP_PROXY/HTTPS_PROXY/ALL_PROXY) can never silently route a "direct" request through an
/// ambient proxy — the model download strategy stays: real direct first, system proxy only
/// on a connection-class failure.
fn build_http_agent(proxy: Option<ureq::Proxy>) -> ureq::Agent {
    let connect = Duration::from_secs(30);
    let recv = Duration::from_secs(30);
    ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(connect))
            .timeout_recv_response(Some(recv))
            // No overall body deadline: a 142 MB model download legitimately takes minutes.
            // A stalled read is bounded per-read by TimeoutRead (ureq 3 has only an overall
            // body timeout, not a per-read one, so the per-read bound is enforced locally).
            .timeout_recv_body(None)
            .proxy(proxy)
            .build(),
    )
}

/// Only a network-path failure may fall back to the system proxy. Once the server has
/// returned an HTTP response, the result is authoritative — a 404 is not "try again through
/// a proxy". `fetch_model_response` calls `.call()`, whose errors are all connection/header
/// phase (the body is read separately), so an `Io` here is a connection-class error.
fn should_retry_proxy(error: &ureq::Error) -> bool {
    matches!(
        error,
        ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::Timeout(_)
            | ureq::Error::Io(_)
    )
}

/// Fetch the model response on the route the product chose. `System` keeps today's probe:
/// direct first, then the auto-detected system proxy only on a transport-level failure —
/// once a server has answered, the result is authoritative (a 404 is never "retry through
/// a proxy"). `Direct` and `Manual` are strict: exactly that route, no fallback, because
/// the user (or the product's settings) chose it on purpose. `start` > 0 asks the server
/// to resume the transfer from that byte (`Range`); a mirror that ignores the header
/// answers 200 with the whole file, which the caller reads as "restart from zero".
fn fetch_model_response(
    url: &str,
    route: &DownloadRoute,
    start: u64,
) -> Result<ureq::http::Response<ureq::Body>, String> {
    // One shared request shape for all three routes: the (optional) Range header is the
    // only difference, so a probe retry and a proxy retry both resume from the same byte.
    let request = |agent: &ureq::Agent| {
        let request = agent.get(url);
        if start > 0 {
            request.header("Range", &format!("bytes={start}-"))
        } else {
            request
        }
    };
    match route {
        DownloadRoute::Direct => request(&build_http_agent(None))
            .call()
            .map_err(|err| format!("Could not download the local voice model: {err}")),
        DownloadRoute::Manual(proxy_url) => {
            let proxy = ureq::Proxy::new(proxy_url).map_err(|err| {
                format!("Could not configure the proxy {proxy_url} for the model download: {err}")
            })?;
            request(&build_http_agent(Some(proxy)))
                .call()
                .map_err(|err| {
                    format!(
                        "Could not download the local voice model (through the configured proxy): {err}"
                    )
                })
        }
        DownloadRoute::System => {
            let direct = build_http_agent(None);
            match request(&direct).call() {
                Ok(r) => Ok(r),
                Err(e) if should_retry_proxy(&e) => {
                    // Direct connection failed — retry through the system proxy if one is detected.
                    match detect_system_proxy() {
                        Some(proxy_url) => {
                            let proxy = ureq::Proxy::new(&proxy_url).map_err(|err| {
                                format!("Could not configure the system proxy: {err}")
                            })?;
                            request(&build_http_agent(Some(proxy)))
                                .call()
                                .map_err(|err| {
                                    format!(
                                        "Could not download the local voice model (tried direct, then the system proxy): {err}"
                                    )
                                })
                        }
                        None => Err(format!("Could not download the local voice model: {e}")),
                    }
                }
                Err(other) => Err(format!("Could not download the local voice model: {other}")),
            }
        }
    }
}

/// Bounds each blocking `read()` on the model body to a per-read timeout, independently of
/// the overall transfer duration. A 142 MB download legitimately takes minutes, but a
/// stalled connection (no data for the read timeout) must surface as an error so the cancel
/// flag — only observed between reads — stays responsive. ureq 3 has only an *overall* body
/// timeout (`timeout_recv_body`), so the per-read bound is enforced here with a detached
/// reader thread + a channel: the worker pumps reads; the consumer receives with a timeout.
struct TimeoutRead {
    rx: Receiver<std::io::Result<(usize, Vec<u8>)>>,
    per_read: Duration,
}

impl TimeoutRead {
    /// Spawn a detached worker that pumps reads from `reader` into a channel until the
    /// receiver is dropped. A blocked read leaves the worker waiting; for a one-shot model
    /// download that is acceptable — the worker exits once the underlying connection closes.
    fn new(mut reader: ureq::BodyReader<'static>, per_read: Duration) -> Self {
        let (tx, rx) = mpsc::channel::<std::io::Result<(usize, Vec<u8>)>>();
        thread::spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let res = reader.read(&mut buf).map(|n| (n, buf[..n].to_vec()));
                if tx.send(res).is_err() {
                    break; // receiver dropped — stop the worker
                }
            }
        });
        TimeoutRead { rx, per_read }
    }
}

impl Read for TimeoutRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.rx.recv_timeout(self.per_read) {
            Ok(Ok((n, data))) => {
                buf[..n].copy_from_slice(&data);
                Ok(n)
            }
            Ok(Err(e)) => Err(e),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Voice model download stalled — no data for the read timeout.",
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(0),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DictationStatus {
    pub recording: bool,
    pub model_installed: bool,
    pub model_verified: bool,
    pub test_passed: bool,
    pub download_in_progress: bool,
    pub model_name: &'static str,
    pub model_bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

struct Recording {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

type SampleBuffer = Arc<Mutex<Vec<f32>>>;
type LiveRecording = Arc<Mutex<Option<(SampleBuffer, u32)>>>;

/// A reusable single-microphone dictation session manager.
///
/// It records only while a host has explicitly started a session; audio is held in memory for
/// that session and is never persisted. The downloaded recognition model is the only data kept
/// under `model_dir`.
/// The proxy decision the host hands the downloader, so a voice-model download leaves the
/// machine by the same route the rest of the product uses. `Direct` and `Manual` are strict
/// — no fallback, because the user (or the product) chose that route on purpose. `System`
/// keeps today's probe: direct first, the system proxy only when the host is unreachable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DownloadRoute {
    /// Direct first, the system proxy only on a transport-level failure (today's behaviour).
    #[default]
    System,
    /// Never proxied, never falls back.
    Direct,
    /// Always through this proxy URL, never falls back.
    Manual(String),
}

pub struct Dictation {
    /// Directory holding the model's files (see [`DEFAULT_MODEL_DIR`]).
    model_dir: PathBuf,
    /// The directory the host handed in; legacy engine files live directly in it.
    base_dir: PathBuf,
    verified_marker_path: PathBuf,
    ready_marker_path: PathBuf,
    /// The loaded recognizer, kept after first use: loading takes over a second, a sentence
    /// takes a fraction of one. Dropped whenever the model files change.
    recognizer: Mutex<Option<Arc<OfflineRecognizer>>>,
    commands: Sender<Command>,
    recording: Arc<Mutex<bool>>,
    // Live handle onto the in-flight recording's sample buffer (set by the capture worker
    // for the duration of a session) so hosts can meter input loudness for UI feedback.
    live: LiveRecording,
    download_in_progress: AtomicBool,
    cancel_download: AtomicBool,
    /// Which route a model download takes, set by the host from the product's network
    /// settings (read at download time, not at construction, so a settings change applies
    /// to the next download without a restart).
    download_route: Mutex<DownloadRoute>,
}

enum Command {
    Start(Sender<Result<(), String>>),
    Stop(Sender<Result<RecordedAudio, String>>),
    Cancel(Sender<()>),
}

struct RecordedAudio {
    samples: Vec<f32>,
    sample_rate: u32,
}

impl Dictation {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        // CPAL's CoreAudio stream is intentionally !Send. Keep it on one dedicated owner thread
        // rather than unsafely forcing it through Tauri's Send + Sync application state.
        let (commands, receiver) = mpsc::channel();
        let recording = Arc::new(Mutex::new(false));
        let live = Arc::new(Mutex::new(None));
        let worker_recording = recording.clone();
        let worker_live = live.clone();
        thread::spawn(move || capture_worker(receiver, worker_recording, worker_live));
        let base_dir = model_dir.into();
        let model_dir = base_dir.join(DEFAULT_MODEL_DIR);
        Self {
            verified_marker_path: model_dir.join("model.verified"),
            ready_marker_path: model_dir.join("model.ready"),
            model_dir,
            base_dir,
            recognizer: Mutex::new(None),
            commands,
            recording,
            live,
            download_in_progress: AtomicBool::new(false),
            cancel_download: AtomicBool::new(false),
            download_route: Mutex::new(DownloadRoute::System),
        }
    }

    /// Which route a model download takes. Read again at every download, so a host that
    /// re-reads its network settings applies them without restarting the app.
    pub fn set_download_route(&self, route: DownloadRoute) {
        *self.download_route.lock().unwrap() = route;
    }

    pub fn status(&self) -> DictationStatus {
        let model_installed = MODEL_ASSETS
            .iter()
            .all(|asset| self.model_dir.join(asset.file).is_file());
        let model_verified =
            model_installed && matches_model_marker(&self.model_dir, &self.verified_marker_path);
        DictationStatus {
            recording: self.recording.lock().map(|r| *r).unwrap_or(false),
            model_installed,
            model_verified,
            test_passed: model_verified && self.ready_marker_path.is_file(),
            download_in_progress: self.download_in_progress.load(Ordering::SeqCst),
            model_name: DEFAULT_MODEL_NAME,
            model_bytes: DEFAULT_MODEL_BYTES,
        }
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// Downloads the default model atomically. Hosts should call this only after an explicit
    /// user action because it is a sizeable download.
    pub fn install_default_model(&self) -> Result<(), String> {
        self.install_model_progress(|_| {})
    }

    /// Downloads and verifies the default model atomically, reporting byte progress to the host.
    /// A canceled/failed transfer never replaces a previously verified model.
    pub fn install_model_progress(
        &self,
        on_progress: impl FnMut(DownloadProgress),
    ) -> Result<(), String> {
        if self.status().model_verified {
            return Ok(());
        }
        self.begin_download()?;
        self.run_download(on_progress)
    }

    /// Starts the same download on a background thread and returns at once. The transfer does not
    /// depend on whichever page asked for it, so leaving the screen does not stop it; `on_done`
    /// reports how it ended. A model that is already verified, or already downloading, is not
    /// started again.
    pub fn install_model_background<P, D>(
        self: &Arc<Self>,
        on_progress: P,
        on_done: D,
    ) -> Result<(), String>
    where
        P: FnMut(DownloadProgress) + Send + 'static,
        D: FnOnce(Result<(), String>) + Send + 'static,
    {
        if self.status().model_verified {
            return Ok(());
        }
        // Claimed here, before the thread starts, so the caller's status read already shows the download.
        self.begin_download()?;
        let dictation = Arc::clone(self);
        thread::spawn(move || {
            let result = dictation.run_download(on_progress);
            on_done(result);
        });
        Ok(())
    }

    fn begin_download(&self) -> Result<(), String> {
        self.download_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "The local voice model is already downloading.".to_owned())?;
        self.cancel_download.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// The body of a download whose claim is already held; always releases it on the way out.
    fn run_download(&self, mut on_progress: impl FnMut(DownloadProgress)) -> Result<(), String> {
        // One route for the whole model: read once, before any byte moves, so a settings
        // change mid-download cannot split one model across two networks.
        let route = self.download_route.lock().unwrap().clone();
        let result = (|| {
            fs::create_dir_all(&self.model_dir)
                .map_err(|e| format!("Could not create model directory: {e}"))?;
            on_progress(DownloadProgress {
                downloaded_bytes: 0,
                total_bytes: DEFAULT_MODEL_BYTES,
            });
            if MODEL_ASSETS
                .iter()
                .any(|asset| verify_asset(&self.model_dir.join(asset.file), asset).is_err())
            {
                self.install_from_archive(&route, &mut on_progress)?;
            }
            on_progress(DownloadProgress {
                downloaded_bytes: DEFAULT_MODEL_BYTES,
                total_bytes: DEFAULT_MODEL_BYTES,
            });
            self.drop_recognizer();
            write_verification_marker(&self.model_dir, &self.verified_marker_path)?;
            let _ = fs::remove_file(&self.ready_marker_path);
            self.remove_legacy_files();
            Ok(())
        })();

        self.download_in_progress.store(false, Ordering::SeqCst);
        self.cancel_download.store(false, Ordering::SeqCst);
        result
    }

    /// Installs both model files from the official archive. The archive is downloaded once (resumable,
    /// checked by its own hash); the two files are then unpacked and each checked against its pin.
    fn install_from_archive(
        &self,
        route: &DownloadRoute,
        on_progress: &mut impl FnMut(DownloadProgress),
    ) -> Result<(), String> {
        let archive = self.model_dir.join(MODEL_ARCHIVE.file);
        if verify_asset(&archive, &MODEL_ARCHIVE).is_err() {
            // The transfer counts archive bytes; scale them onto the model total the page shows.
            let mut scaled = |progress: DownloadProgress| {
                on_progress(DownloadProgress {
                    downloaded_bytes: progress
                        .downloaded_bytes
                        .saturating_mul(DEFAULT_MODEL_BYTES)
                        / MODEL_ARCHIVE.bytes,
                    total_bytes: DEFAULT_MODEL_BYTES,
                })
            };
            self.download_asset(&MODEL_ARCHIVE, &archive, 0, route, &mut scaled)?;
        }
        extract_model_files(&archive, &self.model_dir)?;
        let _ = fs::remove_file(&archive);
        Ok(())
    }

    /// Downloads one model file to `target` (through a `.part` file that is verified before it
    /// takes the final name), trying each of the asset's URLs in turn. `base` is how many bytes
    /// of the whole model earlier files already account for, so progress spans the full model.
    fn download_asset(
        &self,
        asset: &ModelAsset,
        target: &Path,
        base: u64,
        route: &DownloadRoute,
        on_progress: &mut impl FnMut(DownloadProgress),
    ) -> Result<(), String> {
        let partial = target.with_extension("part");
        let mut last_error = String::new();
        for url in asset.urls {
            match self.download_url(url, &partial, asset, base, route, on_progress) {
                Ok(()) => {
                    if target.exists() {
                        fs::remove_file(target)
                            .map_err(|e| format!("Could not replace the local voice model: {e}"))?;
                    }
                    return fs::rename(&partial, target)
                        .map_err(|e| format!("Could not install the local voice model: {e}"));
                }
                Err(DownloadError::Canceled) => {
                    let _ = fs::remove_file(&partial);
                    return Err("Voice model download canceled.".to_owned());
                }
                Err(DownloadError::Failed(message)) => {
                    let _ = fs::remove_file(&partial);
                    last_error = message;
                }
            }
        }
        Err(last_error)
    }

    fn download_url(
        &self,
        url: &str,
        partial: &Path,
        asset: &ModelAsset,
        base: u64,
        route: &DownloadRoute,
        on_progress: &mut impl FnMut(DownloadProgress),
    ) -> Result<(), DownloadError> {
        // Per-read timeout, not overall: the model legitimately takes minutes to arrive, but a
        // stalled connection must surface as an error — the cancel flag is only observed
        // between reads, so an indefinitely blocked read would also make Cancel unresponsive.
        // The route is the product's decision (system probe, strict direct, or a fixed proxy).

        // Resume from an earlier attempt: whatever the .part file already holds is reused via a
        // Range request. A 206 answer appends; any full-entity answer (200) means the mirror does
        // not honor ranges (or the byte offset is gone), and the transfer restarts cleanly.
        let mut start = fs::metadata(partial).map(|m| m.len()).unwrap_or(0);
        if start >= asset.bytes {
            // Enough bytes are already on disk (an interrupted transfer got everything);
            // only the final verification was missing.
            return verify_asset(partial, asset).map_err(DownloadError::Failed);
        }

        let response = fetch_model_response(url, route, start).map_err(DownloadError::Failed)?;
        // A resumed transfer is only valid on 206; anything else is the whole file.
        let resumed = response.status() == 206 && start > 0;
        if !resumed {
            start = 0;
        }
        let mut output = if resumed {
            fs::OpenOptions::new()
                .append(true)
                .open(partial)
                .map_err(|e| {
                    DownloadError::Failed(format!("Could not save the local voice model: {e}"))
                })?
        } else {
            let _ = fs::remove_file(partial);
            fs::File::create(partial).map_err(|e| {
                DownloadError::Failed(format!("Could not save the local voice model: {e}"))
            })?
        };
        let mut input =
            TimeoutRead::new(response.into_body().into_reader(), Duration::from_secs(30));
        let mut downloaded = start;
        let mut last_reported = start;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            if self.cancel_download.load(Ordering::SeqCst) {
                return Err(DownloadError::Canceled);
            }
            let count = input.read(&mut buffer).map_err(|e| {
                DownloadError::Failed(format!("Could not download the local voice model: {e}"))
            })?;
            if count == 0 {
                break;
            }
            output.write_all(&buffer[..count]).map_err(|e| {
                DownloadError::Failed(format!("Could not save the local voice model: {e}"))
            })?;
            downloaded += count as u64;
            if downloaded.saturating_sub(last_reported) >= 512 * 1024 {
                last_reported = downloaded;
                on_progress(DownloadProgress {
                    downloaded_bytes: base + downloaded.min(asset.bytes),
                    total_bytes: DEFAULT_MODEL_BYTES,
                });
            }
        }
        output.flush().map_err(|e| {
            DownloadError::Failed(format!(
                "Could not finish saving the local voice model: {e}"
            ))
        })?;
        drop(output);
        verify_asset(partial, asset).map_err(DownloadError::Failed)
    }
    fn drop_recognizer(&self) {
        if let Ok(mut cached) = self.recognizer.lock() {
            *cached = None;
        }
    }

    /// Removes what the previous (Whisper) engine left behind.
    fn remove_legacy_files(&self) {
        for name in LEGACY_MODEL_FILES {
            let _ = fs::remove_file(self.base_dir.join(name));
        }
    }

    /// The loaded recognizer, loading it on first use.
    fn recognizer(&self) -> Result<Arc<OfflineRecognizer>, String> {
        let mut cached = self
            .recognizer
            .lock()
            .map_err(|_| "The local voice model is unavailable.".to_owned())?;
        if let Some(recognizer) = cached.as_ref() {
            return Ok(recognizer.clone());
        }
        let recognizer = Arc::new(load_recognizer(&self.model_dir)?);
        *cached = Some(recognizer.clone());
        Ok(recognizer)
    }

    /// Verifies an already-installed model (including installs made by older app versions).
    pub fn verify_default_model(&self) -> Result<(), String> {
        for asset in &MODEL_ASSETS {
            verify_asset(&self.model_dir.join(asset.file), asset)?;
        }
        write_verification_marker(&self.model_dir, &self.verified_marker_path)
    }

    pub fn cancel_model_download(&self) {
        self.cancel_download.store(true, Ordering::SeqCst);
    }

    pub fn mark_test_passed(&self) -> Result<(), String> {
        if !self.status().model_verified {
            return Err("Verify the local voice model before testing it.".to_owned());
        }
        fs::write(&self.ready_marker_path, b"ready")
            .map_err(|e| format!("Could not save the voice input test result: {e}"))
    }

    pub fn delete_default_model(&self) -> Result<(), String> {
        self.cancel_model_download();
        self.cancel();
        self.drop_recognizer();
        self.remove_legacy_files();
        if self.model_dir.exists() {
            fs::remove_dir_all(&self.model_dir)
                .map_err(|e| format!("Could not remove {}: {e}", self.model_dir.display()))?;
        }
        Ok(())
    }

    /// Begins microphone capture. A host must call [`stop_and_transcribe`](Self::stop_and_transcribe)
    /// or [`cancel`](Self::cancel) before a new recording can start.
    pub fn start(&self) -> Result<(), String> {
        if !self.status().model_verified {
            return Err("Set up and verify Voice Input in Settings first.".to_owned());
        }
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::Start(reply))
            .map_err(|_| "Dictation is unavailable because its audio worker stopped.".to_owned())?;
        result
            .recv()
            .map_err(|_| "Dictation is unavailable because its audio worker stopped.".to_owned())?
    }

    /// Stops capture and returns a final local transcript. This is intentionally synchronous so
    /// hosts can run it off their UI thread and decide how to present completion/error states.
    pub fn stop_and_transcribe(&self) -> Result<String, String> {
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::Stop(reply))
            .map_err(|_| "Dictation is unavailable because its audio worker stopped.".to_owned())?;
        let RecordedAudio {
            samples,
            sample_rate,
        } = result.recv().map_err(|_| {
            "Dictation is unavailable because its audio worker stopped.".to_owned()
        })??;
        if samples.len() < (sample_rate as usize / 4) {
            return Ok(String::new());
        }
        let recognizer = self.recognizer()?;
        transcribe(&recognizer, &resample_mono(&samples, sample_rate))
    }

    /// Instantaneous input loudness of the in-flight recording, 0.0..=1.0 — RMS over the
    /// most recent ~100ms, scaled so conversational speech spans most of the range. 0.0
    /// while not recording. Cheap enough to poll at UI frame-ish rates.
    pub fn input_level(&self) -> f32 {
        let live = match self.live.lock() {
            Ok(guard) => guard,
            Err(_) => return 0.0,
        };
        let Some((samples, sample_rate)) = live.as_ref() else {
            return 0.0;
        };
        let Ok(samples) = samples.lock() else {
            return 0.0;
        };
        let window = (*sample_rate as usize / 10).max(1);
        let tail = &samples[samples.len().saturating_sub(window)..];
        if tail.is_empty() {
            return 0.0;
        }
        let mean_square: f32 = tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32;
        (mean_square.sqrt() * 8.0).clamp(0.0, 1.0)
    }

    /// Discards the current in-memory recording without retaining or transcribing it.
    pub fn cancel(&self) {
        let (reply, done) = mpsc::channel();
        if self.commands.send(Command::Cancel(reply)).is_ok() {
            let _ = done.recv();
        }
    }
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut output = String::with_capacity(bytes.len() * 2);

    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

enum DownloadError {
    Canceled,
    Failed(String),
}

/// Unpacks the model files from the archive into `dir`. Each file is written under a `.part` name
/// and checked against its own pin before it takes its final name.
fn extract_model_files(archive: &Path, dir: &Path) -> Result<(), String> {
    let file = fs::File::open(archive)
        .map_err(|e| format!("Could not open the voice model archive: {e}"))?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(file));
    let entries = tar
        .entries()
        .map_err(|e| format!("Could not read the voice model archive: {e}"))?;
    let mut found = [false; 2];
    for entry in entries {
        let mut entry =
            entry.map_err(|e| format!("Could not read the voice model archive: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("Could not read the voice model archive: {e}"))?
            .into_owned();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(index) = MODEL_ASSETS.iter().position(|asset| asset.file == name) else {
            continue;
        };
        let asset = &MODEL_ASSETS[index];
        let target = dir.join(asset.file);
        let partial = target.with_extension("part");
        entry
            .unpack(&partial)
            .map_err(|e| format!("Could not save the local voice model: {e}"))?;
        verify_asset(&partial, asset)?;
        if target.exists() {
            fs::remove_file(&target)
                .map_err(|e| format!("Could not replace the local voice model: {e}"))?;
        }
        fs::rename(&partial, &target)
            .map_err(|e| format!("Could not install the local voice model: {e}"))?;
        found[index] = true;
    }
    if found.iter().all(|present| *present) {
        Ok(())
    } else {
        Err("The voice model archive does not contain the expected files.".to_owned())
    }
}

fn verify_asset(path: &Path, asset: &ModelAsset) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|e| format!("Could not read the local voice model: {e}"))?;
    if metadata.len() != asset.bytes {
        return Err(format!(
            "The local voice model is incomplete ({} of {} bytes).",
            metadata.len(),
            asset.bytes
        ));
    }
    let mut file =
        fs::File::open(path).map_err(|e| format!("Could not read the local voice model: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| format!("Could not verify the local voice model: {e}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = hasher.finalize();
    let actual = encode_lower_hex(digest.as_ref());
    if actual != asset.sha256 {
        return Err(
            "The local voice model failed its checksum. Repair the download in Settings."
                .to_owned(),
        );
    }
    Ok(())
}

fn file_modified_millis(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

/// The marker records, per model file, the pinned hash and the file's modification time: a file
/// that was verified once and has not been touched since need not be hashed again.
fn expected_marker(model_dir: &Path) -> Option<String> {
    let mut marker = String::new();
    for asset in &MODEL_ASSETS {
        let modified = file_modified_millis(&model_dir.join(asset.file))?;
        marker.push_str(&format!("{}\n{modified}\n", asset.sha256));
    }
    Some(marker)
}

fn write_verification_marker(model_dir: &Path, marker_path: &Path) -> Result<(), String> {
    let marker = expected_marker(model_dir)
        .ok_or_else(|| "Could not read the installed voice model timestamp.".to_owned())?;
    fs::write(marker_path, marker)
        .map_err(|e| format!("Could not record voice model verification: {e}"))
}

fn matches_model_marker(model_dir: &Path, marker_path: &Path) -> bool {
    for asset in &MODEL_ASSETS {
        match fs::metadata(model_dir.join(asset.file)) {
            Ok(metadata) if metadata.len() == asset.bytes => {}
            _ => return false,
        }
    }
    let Ok(recorded) = fs::read_to_string(marker_path) else {
        return false;
    };
    expected_marker(model_dir).is_some_and(|expected| expected == recorded)
}

fn capture_worker(
    receiver: Receiver<Command>,
    recording_status: Arc<Mutex<bool>>,
    live: LiveRecording,
) {
    let mut recording: Option<Recording> = None;
    let set_live = |value: Option<(Arc<Mutex<Vec<f32>>>, u32)>| {
        if let Ok(mut guard) = live.lock() {
            *guard = value;
        }
    };
    for command in receiver {
        match command {
            Command::Start(reply) => {
                if recording.is_some() {
                    let _ = reply.send(Err("Dictation is already recording.".to_owned()));
                    continue;
                }
                match start_recording() {
                    Ok(next) => {
                        set_live(Some((next.samples.clone(), next.sample_rate)));
                        recording = Some(next);
                        if let Ok(mut active) = recording_status.lock() {
                            *active = true;
                        }
                        let _ = reply.send(Ok(()));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            Command::Stop(reply) => {
                set_live(None);
                let result = recording
                    .take()
                    .ok_or_else(|| "Dictation is not recording.".to_owned())
                    .and_then(finish_recording);
                if let Ok(mut active) = recording_status.lock() {
                    *active = false;
                }
                let _ = reply.send(result);
            }
            Command::Cancel(reply) => {
                set_live(None);
                recording.take();
                if let Ok(mut active) = recording_status.lock() {
                    *active = false;
                }
                let _ = reply.send(());
            }
        }
    }
}

fn start_recording() -> Result<Recording, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| {
        if cfg!(target_os = "macos") {
            "No microphone is available. Check your Mac sound settings.".to_owned()
        } else if cfg!(target_os = "windows") {
            "No microphone is available. Check your Windows sound settings.".to_owned()
        } else {
            "No microphone is available. Check your system's sound settings.".to_owned()
        }
    })?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Could not open the microphone: {e}"))?;
    let config: StreamConfig = supported.into();
    let samples = Arc::new(Mutex::new(Vec::new()));
    let stream = build_stream(&device, &config, supported.sample_format(), samples.clone())?;
    stream
        .play()
        .map_err(|e| format!("Could not start microphone recording: {e}"))?;
    Ok(Recording {
        stream,
        samples,
        sample_rate: config.sample_rate,
    })
}

fn finish_recording(recording: Recording) -> Result<RecordedAudio, String> {
    let Recording {
        stream,
        samples,
        sample_rate,
    } = recording;
    drop(stream);
    let samples = samples
        .lock()
        .map_err(|_| "Could not read the recorded audio.".to_owned())?
        .clone();
    Ok(RecordedAudio {
        samples,
        sample_rate,
    })
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    samples: Arc<Mutex<Vec<f32>>>,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let on_error = |error| eprintln!("[delta-stt] microphone stream error: {error}");
    match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                *config,
                move |data: &[f32], _| append_frames(&samples, data, channels, |sample| sample),
                on_error,
                None,
            )
            .map_err(|e| format!("Could not create microphone stream: {e}")),
        SampleFormat::I16 => device
            .build_input_stream(
                *config,
                move |data: &[i16], _| {
                    append_frames(&samples, data, channels, |sample| {
                        sample as f32 / i16::MAX as f32
                    })
                },
                on_error,
                None,
            )
            .map_err(|e| format!("Could not create microphone stream: {e}")),
        SampleFormat::U16 => device
            .build_input_stream(
                *config,
                move |data: &[u16], _| {
                    append_frames(&samples, data, channels, |sample| {
                        (sample as f32 / u16::MAX as f32) * 2.0 - 1.0
                    })
                },
                on_error,
                None,
            )
            .map_err(|e| format!("Could not create microphone stream: {e}")),
        other => Err(format!("Unsupported microphone sample format: {other:?}")),
    }
}

fn append_frames<T>(
    target: &Arc<Mutex<Vec<f32>>>,
    data: &[T],
    channels: usize,
    convert: impl Fn(T) -> f32,
) where
    T: Copy,
{
    let Ok(mut output) = target.lock() else {
        return;
    };
    output.reserve(data.len() / channels.max(1));
    for frame in data.chunks(channels.max(1)) {
        let sum: f32 = frame.iter().copied().map(&convert).sum();
        output.push(sum / frame.len() as f32);
    }
}

fn resample_mono(input: &[f32], source_rate: u32) -> Vec<f32> {
    if source_rate == SAMPLE_RATE {
        return input.to_vec();
    }
    let output_len = (input.len() as u64 * SAMPLE_RATE as u64 / source_rate as u64) as usize;
    let ratio = source_rate as f64 / SAMPLE_RATE as f64;
    (0..output_len)
        .map(|i| {
            let position = i as f64 * ratio;
            let left = position.floor() as usize;
            let right = (left + 1).min(input.len().saturating_sub(1));
            let fraction = (position - left as f64) as f32;
            input[left] * (1.0 - fraction) + input[right] * fraction
        })
        .collect()
}

fn load_recognizer(model_dir: &Path) -> Result<OfflineRecognizer, String> {
    let path_text = |name: &str| {
        model_dir
            .join(name)
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| "The local voice model path is not valid text.".to_owned())
    };
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
        model: Some(path_text(MODEL_ASSETS[0].file)?),
        // The language is detected from the speech itself.
        language: Some("auto".to_owned()),
        // Punctuation and digits (inverse text normalisation) for text that reads as written.
        use_itn: true,
    };
    config.model_config.tokens = Some(path_text(MODEL_ASSETS[1].file)?);
    config.model_config.num_threads = thread::available_parallelism()
        .map(|n| n.get().min(4) as i32)
        .unwrap_or(2);
    OfflineRecognizer::create(&config)
        .ok_or_else(|| "Could not load the local voice model. Repair it in Settings.".to_owned())
}

fn transcribe(recognizer: &OfflineRecognizer, samples: &[f32]) -> Result<String, String> {
    let stream = recognizer.create_stream();
    stream.accept_waveform(SAMPLE_RATE as i32, samples);
    recognizer.decode(&stream);
    let result = stream
        .get_result()
        .ok_or_else(|| "Could not read the transcript.".to_owned())?;
    Ok(result.text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{
        encode_lower_hex, resample_mono, should_retry_proxy, write_verification_marker, Dictation,
        DEFAULT_MODEL_BYTES, MODEL_ARCHIVE, MODEL_ASSETS,
    };

    #[test]
    fn sha256_digest_formats_as_lowercase_hex() {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        assert_eq!(
            encode_lower_hex(hasher.finalize().as_ref()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn proxy_fallback_only_for_connection_class_errors() {
        use std::io;
        use ureq::{Error, Timeout};

        // Network-path failures → retry through the system proxy.
        assert!(should_retry_proxy(&Error::HostNotFound));
        assert!(should_retry_proxy(&Error::ConnectionFailed));
        assert!(should_retry_proxy(&Error::Timeout(Timeout::Connect)));
        assert!(should_retry_proxy(&Error::Timeout(Timeout::RecvResponse)));
        assert!(should_retry_proxy(&Error::Io(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "host unreachable"
        ))));

        // The server already answered → the result is authoritative, no proxy fallback.
        assert!(!should_retry_proxy(&Error::StatusCode(403)));
        assert!(!should_retry_proxy(&Error::StatusCode(404)));
        assert!(!should_retry_proxy(&Error::StatusCode(500)));
        assert!(!should_retry_proxy(&Error::BadUri("no host".to_string())));
        assert!(!should_retry_proxy(&Error::InvalidProxyUrl));
    }

    #[test]
    fn resampling_preserves_a_16khz_stream() {
        let input = vec![0.0, 0.5, -0.5];
        assert_eq!(resample_mono(&input, 16_000), input);
    }

    #[test]
    fn resampling_converts_duration() {
        let input = vec![0.0; 48_000];
        assert_eq!(resample_mono(&input, 48_000).len(), 16_000);
    }

    #[test]
    fn model_assets_are_pinned_to_the_published_sherpa_onnx_files() {
        assert_eq!(MODEL_ASSETS[0].bytes, 239_233_841);
        assert_eq!(MODEL_ASSETS[1].bytes, 315_894);
        assert_eq!(DEFAULT_MODEL_BYTES, 239_549_735);
        for asset in &MODEL_ASSETS {
            assert_eq!(asset.sha256.len(), 64);
        }
        assert_eq!(MODEL_ARCHIVE.sha256.len(), 64);
        assert_eq!(MODEL_ARCHIVE.bytes, 163_002_883);
        assert!(MODEL_ARCHIVE
            .urls
            .iter()
            .all(|url| url.starts_with("https://") && url.ends_with(".tar.bz2")));
    }

    fn model_dir_with_files(unique: &str) -> (std::path::PathBuf, Dictation) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("delta-stt-{unique}-{nanos}"));
        let dictation = Dictation::new(&dir);
        fs::create_dir_all(dictation.model_dir()).unwrap();
        for asset in &MODEL_ASSETS {
            fs::File::create(dictation.model_dir().join(asset.file))
                .unwrap()
                .set_len(asset.bytes)
                .unwrap();
        }
        (dir, dictation)
    }

    #[test]
    fn readiness_requires_a_verified_model_and_persists_after_a_test() {
        let (dir, dictation) = model_dir_with_files("readiness");
        assert!(dictation.status().model_installed);
        assert!(!dictation.status().model_verified);
        write_verification_marker(dictation.model_dir(), &dictation.verified_marker_path).unwrap();
        assert!(dictation.status().model_verified);
        assert!(!dictation.status().test_passed);
        dictation.mark_test_passed().unwrap();
        assert!(dictation.status().test_passed);
        dictation.delete_default_model().unwrap();
        assert!(!dictation.status().model_installed);
        drop(dictation);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_model_with_a_missing_or_resized_file_is_not_verified() {
        let (dir, dictation) = model_dir_with_files("partial");
        write_verification_marker(dictation.model_dir(), &dictation.verified_marker_path).unwrap();
        assert!(dictation.status().model_verified);
        let tokens = dictation.model_dir().join(MODEL_ASSETS[1].file);
        fs::File::create(&tokens).unwrap().set_len(10).unwrap();
        assert!(!dictation.status().model_verified);
        fs::remove_file(&tokens).unwrap();
        assert!(!dictation.status().model_installed);
        drop(dictation);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deleting_the_model_also_removes_what_the_whisper_engine_left() {
        let (dir, dictation) = model_dir_with_files("legacy");
        fs::write(dir.join("ggml-base.bin"), b"old").unwrap();
        fs::write(dir.join("ggml-base.bin.verified"), b"old").unwrap();
        dictation.delete_default_model().unwrap();
        assert!(!dir.join("ggml-base.bin").exists());
        assert!(!dir.join("ggml-base.bin.verified").exists());
        drop(dictation);
        fs::remove_dir_all(dir).unwrap();
    }

    /// Needs the real model: `DELTA_STT_MODEL_DIR` must hold `model.int8.onnx` and `tokens.txt`,
    /// and `DELTA_STT_SAMPLE_WAV` a 16 kHz mono 16-bit WAV of Mandarin speech. Run with
    /// `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs the downloaded SenseVoice model"]
    fn transcribes_mandarin_speech_with_the_real_model() {
        let model_dir = std::env::var("DELTA_STT_MODEL_DIR").expect("DELTA_STT_MODEL_DIR");
        let wav =
            fs::read(std::env::var("DELTA_STT_SAMPLE_WAV").expect("DELTA_STT_SAMPLE_WAV")).unwrap();
        // 44-byte canonical header, then little-endian 16-bit samples.
        let samples: Vec<f32> = wav[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        let recognizer = super::load_recognizer(std::path::Path::new(&model_dir)).unwrap();
        let text = super::transcribe(&recognizer, &samples).unwrap();
        assert!(text.contains("开饭时间"), "got {text:?}");
    }
}
