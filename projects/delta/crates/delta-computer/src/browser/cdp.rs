//! A small Chrome DevTools Protocol client over one websocket.
//!
//! A reader thread owns the incoming side: it routes each reply to the call waiting for it and
//! hands every event to the handler. The handler matters for safety, not only convenience: the
//! network guard answers `Fetch.requestPaused` events, and a paused request that nobody answers
//! stalls the page, so events must be served even while no tool call is running.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::{Message, WebSocket};

/// How long the reader waits for data before giving writers a turn at the socket.
const READ_SLICE: Duration = Duration::from_millis(40);

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;
type EventHandler = dyn Fn(&Cdp, Value) + Send + Sync + 'static;

#[derive(Clone)]
pub struct Cdp {
    socket: Arc<Mutex<WebSocket<TcpStream>>>,
    next_id: Arc<AtomicU64>,
    pending: Pending,
    alive: Arc<AtomicBool>,
    /// Writers waiting for the socket. The reader holds it for up to a read slice at a time and a
    /// std mutex is not fair, so it steps aside while anyone is waiting.
    writers: Arc<AtomicU64>,
}

impl Cdp {
    /// Connect to the browser's debugging endpoint (`ws://127.0.0.1:<port><path>`).
    pub fn connect(
        port: u16,
        path: &str,
        handler: impl Fn(&Cdp, Value) + Send + Sync + 'static,
    ) -> Result<Cdp, String> {
        let stream = TcpStream::connect(("127.0.0.1", port))
            .map_err(|error| format!("could not reach the browser: {error}"))?;
        stream.set_nodelay(true).ok();
        let url = format!("ws://127.0.0.1:{port}{path}");
        let (socket, _) = tungstenite::client(url.as_str(), stream)
            .map_err(|error| format!("could not talk to the browser: {error}"))?;
        // Only after the handshake: the timeout is what lets the reader share the socket.
        socket
            .get_ref()
            .set_read_timeout(Some(READ_SLICE))
            .map_err(|error| error.to_string())?;
        let cdp = Cdp {
            socket: Arc::new(Mutex::new(socket)),
            next_id: Arc::new(AtomicU64::new(1)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            alive: Arc::new(AtomicBool::new(true)),
            writers: Arc::new(AtomicU64::new(0)),
        };
        let handler: Arc<EventHandler> = Arc::new(handler);
        let reader = cdp.clone();
        std::thread::Builder::new()
            .name("delta-browser-reader".into())
            .spawn(move || reader.read_loop(handler))
            .map_err(|error| error.to_string())?;
        Ok(cdp)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Stop reading and fail every call still waiting.
    pub fn close(&self) {
        self.alive.store(false, Ordering::Release);
        self.fail_all("the browser connection closed");
        if let Ok(mut socket) = self.socket.lock() {
            let _ = socket.close(None);
        }
    }

    fn fail_all(&self, message: &str) {
        let waiting: Vec<_> = self
            .pending
            .lock()
            .map(|mut pending| pending.drain().collect())
            .unwrap_or_default();
        for (_, sender) in waiting {
            let _ = sender.send(Err(message.to_owned()));
        }
    }

    fn read_loop(&self, handler: Arc<EventHandler>) {
        while self.is_alive() {
            while self.writers.load(Ordering::Acquire) > 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
            let message = {
                let Ok(mut socket) = self.socket.lock() else {
                    break;
                };
                socket.read()
            };
            match message {
                Ok(Message::Text(text)) => {
                    if let Ok(value) = serde_json::from_str::<Value>(text.as_str()) {
                        self.route(value, &handler);
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    // Nothing arrived in this slice; let a writer in.
                    std::thread::yield_now();
                }
                Err(_) => break,
            }
        }
        self.alive.store(false, Ordering::Release);
        self.fail_all("the browser connection closed");
    }

    fn route(&self, value: Value, handler: &Arc<EventHandler>) {
        if let Some(id) = value.get("id").and_then(Value::as_u64) {
            let sender = self.pending.lock().ok().and_then(|mut p| p.remove(&id));
            if let Some(sender) = sender {
                let outcome = match value.get("error") {
                    Some(error) => Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("the browser refused the request")
                        .to_owned()),
                    None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = sender.send(outcome);
            }
        } else if value.get("method").is_some() {
            handler(self, value);
        }
    }

    fn write(
        &self,
        id: u64,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<(), String> {
        if !self.is_alive() {
            return Err("the browser connection closed".to_owned());
        }
        let mut request = json!({"id": id, "method": method, "params": params});
        if let Some(session) = session {
            request["sessionId"] = json!(session);
        }
        self.writers.fetch_add(1, Ordering::AcqRel);
        let locked = self.socket.lock();
        self.writers.fetch_sub(1, Ordering::AcqRel);
        let mut socket = locked.map_err(|_| "browser lock poisoned")?;
        socket
            .send(Message::text(request.to_string()))
            .map_err(|error| format!("could not send to the browser: {error}"))
    }

    /// Send a command and do not wait for the answer (used from the event handler).
    pub fn notify(&self, method: &str, params: Value, session: Option<&str>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.write(id, method, params, session);
    }

    /// Send a command and wait for its answer, giving up on `cancel` or after `timeout`.
    pub fn call(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Value, String> {
        let (sender, receiver) = mpsc::channel();
        // Register before sending: the answer can beat the registration otherwise.
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.pending
            .lock()
            .map_err(|_| "browser lock poisoned")?
            .insert(id, sender);
        if let Err(error) = self.write(id, method, params, session) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            return Err(error);
        }
        let deadline = Instant::now() + timeout;
        loop {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(outcome) => return outcome,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("the browser connection closed".to_owned())
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if cancelled() {
                        return Err("cancelled".to_owned());
                    }
                    if Instant::now() >= deadline {
                        return Err(format!("the browser did not answer {method} in time"));
                    }
                }
            }
        }
    }
}
