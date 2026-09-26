//! Session-owned runtime control surface and background worker loop.
//!
//! RuntimeHost owns agent-turn orchestration. RuntimeHandle owns short-lived
//! control operations, queueing and worker-thread lifecycle.

use super::*;

impl RuntimeHandle {
    /// Start a dedicated worker thread for `host`. The returned handle is idle;
    /// callers must register it before calling [`run`](Self::run).
    pub fn spawn(mut host: RuntimeHost) -> Result<Self, String> {
        let session_id = host.session_id.clone();
        let cancel = host.cancel.clone();
        let provider_cancel = host.provider_cancel.clone();
        let steering = host.steering.clone();
        let state = Arc::new(Mutex::new(RuntimeState::Idle));
        host.runtime_state = Some(state.clone());
        let approvals = host.approvals.clone();
        let interactions = host.interactions.clone();
        let follow_ups = Arc::new(Mutex::new(VecDeque::new()));
        let messages = Arc::new(RwLock::new(host.messages.clone()));
        let (command_tx, command_rx) = mpsc::channel();

        let worker_state = state.clone();
        let worker_follow_ups = follow_ups.clone();
        let worker_messages = messages.clone();
        let worker_cancel = cancel.clone();
        let worker_provider_cancel = provider_cancel.clone();
        std::thread::Builder::new()
            .name(format!("delta-runtime-{session_id}"))
            .spawn(move || {
                runtime_worker(
                    host,
                    command_rx,
                    worker_state,
                    worker_follow_ups,
                    worker_messages,
                    worker_cancel,
                    worker_provider_cancel,
                )
            })
            .map_err(|error| format!("failed to start runtime worker: {error}"))?;

        Ok(Self {
            session_id,
            command_tx,
            cancel,
            provider_cancel,
            steering,
            follow_ups,
            state,
            messages,
            approvals,
            interactions,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn state(&self) -> RuntimeState {
        *self.state.lock().unwrap()
    }

    pub fn messages(&self) -> Vec<Value> {
        self.messages.read().unwrap().clone()
    }

    pub fn resolve_approval(
        &self,
        tool_call_id: Option<&str>,
        decision: &str,
    ) -> Result<String, String> {
        let decision = ApprovalDecision::parse(decision)?;
        self.approvals.resolve(tool_call_id, decision)
    }

    pub fn pending_approvals(&self) -> Vec<String> {
        self.approvals.pending_ids()
    }

    pub fn resolve_interaction(
        &self,
        kind: &str,
        tool_call_id: Option<&str>,
        response: Value,
    ) -> Result<String, String> {
        self.interactions.resolve(kind, tool_call_id, response)
    }

    pub fn run(&self, input: String, source: Option<Value>) -> Result<String, String> {
        self.run_with_attachments(input, Vec::new(), source)
    }

    pub fn run_with_attachments(
        &self,
        input: String,
        attachments: Vec<Value>,
        source: Option<Value>,
    ) -> Result<String, String> {
        let run_id = uuid_v4();
        self.enqueue_operation(RuntimeOperation::Run(QueuedRun {
            input,
            attachments,
            source,
            run_id: run_id.clone(),
        }))?;
        Ok(run_id)
    }

    pub fn resume(&self) -> Result<String, String> {
        let run_id = uuid_v4();
        self.enqueue_operation(RuntimeOperation::Resume {
            run_id: run_id.clone(),
        })?;
        Ok(run_id)
    }

    pub fn retry(&self) -> Result<String, String> {
        let run_id = uuid_v4();
        self.enqueue_operation(RuntimeOperation::Retry {
            run_id: run_id.clone(),
        })?;
        Ok(run_id)
    }

    fn enqueue_operation(&self, operation: RuntimeOperation) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if state.is_active() {
            return Err(format!(
                "session {} already has an active run ({state:?})",
                self.session_id
            ));
        }
        self.cancel.store(false, Ordering::SeqCst);
        self.provider_cancel.store(false, Ordering::SeqCst);
        self.steering.lock().unwrap().clear();
        self.follow_ups.lock().unwrap().clear();
        *state = RuntimeState::Running;
        if self
            .command_tx
            .send(RuntimeCommand::Execute(operation))
            .is_err()
        {
            *state = RuntimeState::Failed;
            return Err(format!(
                "runtime worker for session {} is unavailable",
                self.session_id
            ));
        }
        Ok(())
    }

    /// Inject guidance into the live run. The current provider stream is
    /// cooperatively interrupted, while the run itself remains active.
    pub fn steer(&self, text: &str, source: Option<Value>) -> Result<(), String> {
        let state = self.state.lock().unwrap();
        if !matches!(
            *state,
            RuntimeState::Running | RuntimeState::WaitingApproval | RuntimeState::WaitingUser
        ) {
            return Err(format!(
                "session {} cannot be steered while {state:?}",
                self.session_id
            ));
        }
        self.steering
            .lock()
            .unwrap()
            .push((text.to_string(), source));
        self.provider_cancel.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Queue a distinct run for the same session. The worker scheduler starts
    /// it only after the current run reaches a terminal boundary.
    pub fn follow_up(&self, text: &str, source: Option<Value>) -> Result<String, String> {
        let state = self.state.lock().unwrap();
        if !matches!(
            *state,
            RuntimeState::Running | RuntimeState::WaitingApproval | RuntimeState::WaitingUser
        ) {
            return Err(format!(
                "session {} has no active run to follow",
                self.session_id
            ));
        }
        let run_id = uuid_v4();
        self.follow_ups.lock().unwrap().push_back(QueuedRun {
            input: text.to_string(),
            attachments: Vec::new(),
            source,
            run_id: run_id.clone(),
        });
        Ok(run_id)
    }

    /// Cancel the active run immediately. Pending follow-ups are discarded so
    /// cancellation cannot unexpectedly start more work.
    pub fn cancel(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if !state.is_active() {
            return false;
        }
        *state = RuntimeState::Cancelling;
        self.follow_ups.lock().unwrap().clear();
        self.cancel.store(true, Ordering::SeqCst);
        self.provider_cancel.store(true, Ordering::SeqCst);
        true
    }

    pub fn switch_model(&self, model: &str) -> Result<Option<String>, String> {
        let state = self.state.lock().unwrap();
        if state.is_active() {
            return Err(format!(
                "cannot switch model: session {} has an active run ({state:?})",
                self.session_id
            ));
        }
        let (reply, response) = mpsc::channel();
        self.command_tx
            .send(RuntimeCommand::SwitchModel {
                change: ModelChange::LegacyId(model.to_string()),
                reply,
            })
            .map_err(|_| "runtime worker is unavailable".to_string())?;
        response
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "runtime worker did not acknowledge model switch".to_string())?
    }

    pub fn switch_runtime_config(&self, config: RuntimeConfig) -> Result<Option<String>, String> {
        let state = self.state.lock().unwrap();
        if state.is_active() {
            return Err(format!(
                "cannot switch model: session {} has an active run ({state:?})",
                self.session_id
            ));
        }
        let (reply, response) = mpsc::channel();
        self.command_tx
            .send(RuntimeCommand::SwitchModel {
                change: ModelChange::Resolved(Box::new(config)),
                reply,
            })
            .map_err(|_| "runtime worker is unavailable".to_string())?;
        response
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "runtime worker did not acknowledge model switch".to_string())?
    }

    pub fn refresh_runtime(
        &self,
        config: RuntimeConfig,
        tools: Value,
    ) -> Result<Option<String>, String> {
        let state = self.state.lock().unwrap();
        if state.is_active() {
            return Err(format!(
                "cannot refresh runtime: session {} has an active run ({state:?})",
                self.session_id
            ));
        }
        let (reply, response) = mpsc::channel();
        self.command_tx
            .send(RuntimeCommand::RefreshRuntime {
                config: Box::new(config),
                tools,
                reply,
            })
            .map_err(|_| "runtime worker is unavailable".to_string())?;
        response
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "runtime worker did not acknowledge runtime refresh".to_string())?
    }

    pub fn truncate_messages(&self, index: usize) -> Result<usize, String> {
        let state = self.state.lock().unwrap();
        if state.is_active() {
            return Err(format!(
                "cannot truncate messages: session {} has an active run ({state:?})",
                self.session_id
            ));
        }
        let (reply, response) = mpsc::channel();
        self.command_tx
            .send(RuntimeCommand::Truncate { index, reply })
            .map_err(|_| "runtime worker is unavailable".to_string())?;
        response
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "runtime worker did not acknowledge message truncation".to_string())?
    }
}

fn runtime_worker(
    mut host: RuntimeHost,
    command_rx: mpsc::Receiver<RuntimeCommand>,
    state: Arc<Mutex<RuntimeState>>,
    follow_ups: Arc<Mutex<VecDeque<QueuedRun>>>,
    messages: Arc<RwLock<Vec<Value>>>,
    cancel: Arc<AtomicBool>,
    provider_cancel: Arc<AtomicBool>,
) {
    while let Ok(command) = command_rx.recv() {
        match command {
            RuntimeCommand::Execute(operation) => execute_operation_chain(
                &mut host,
                operation,
                &state,
                &follow_ups,
                &messages,
                &cancel,
                &provider_cancel,
            ),
            RuntimeCommand::SwitchModel { change, reply } => {
                host.clear_event_error();
                let notice = match change {
                    ModelChange::LegacyId(model) => host.switch_model(&model),
                    ModelChange::Resolved(config) => host.switch_runtime_config(*config),
                };
                sync_message_snapshot(&host, &messages);
                let result = host.ensure_event_delivery().map(|_| notice);
                let _ = reply.send(result);
            }
            RuntimeCommand::RefreshRuntime {
                config,
                tools,
                reply,
            } => {
                host.clear_event_error();
                let notice = host.switch_runtime_config(*config);
                host.replace_tools(tools);
                sync_message_snapshot(&host, &messages);
                let result = host.ensure_event_delivery().map(|_| notice);
                let _ = reply.send(result);
            }
            RuntimeCommand::Truncate { index, reply } => {
                host.truncate_messages(index);
                sync_message_snapshot(&host, &messages);
                let _ = reply.send(Ok(host.messages().len()));
            }
        }
    }
}

fn execute_operation_chain(
    host: &mut RuntimeHost,
    mut operation: RuntimeOperation,
    state: &Arc<Mutex<RuntimeState>>,
    follow_ups: &Arc<Mutex<VecDeque<QueuedRun>>>,
    messages: &Arc<RwLock<Vec<Value>>>,
    cancel: &Arc<AtomicBool>,
    provider_cancel: &Arc<AtomicBool>,
) {
    loop {
        let result = match operation {
            RuntimeOperation::Run(run) => {
                host.set_run_id(run.run_id);
                host.run_with_attachments(&run.input, &run.attachments, run.source)
            }
            RuntimeOperation::Resume { run_id } => {
                host.set_run_id(run_id);
                host.resume()
            }
            RuntimeOperation::Retry { run_id } => {
                host.set_run_id(run_id);
                host.retry()
            }
        };
        sync_message_snapshot(host, messages);
        let terminal_state = state_from_result(&result);

        // Holding state before the queue gives follow_up() an atomic choice:
        // either it queues while the run is active, or it observes terminal
        // state and is rejected. No accepted follow-up can be lost here.
        let mut current_state = state.lock().unwrap();
        let mut queued = follow_ups.lock().unwrap();
        if cancel.load(Ordering::SeqCst) {
            queued.clear();
            *current_state = RuntimeState::Interrupted;
            break;
        }
        if let Some(next) = queued.pop_front() {
            cancel.store(false, Ordering::SeqCst);
            provider_cancel.store(false, Ordering::SeqCst);
            *current_state = RuntimeState::Running;
            operation = RuntimeOperation::Run(next);
            drop(queued);
            drop(current_state);
            continue;
        }
        *current_state = terminal_state;
        break;
    }
}

fn sync_message_snapshot(host: &RuntimeHost, messages: &Arc<RwLock<Vec<Value>>>) {
    *messages.write().unwrap() = host.messages().to_vec();
}

fn state_from_result(result: &Result<Value, String>) -> RuntimeState {
    match result {
        Err(_) => RuntimeState::Failed,
        Ok(value) if value.get("status").and_then(Value::as_str) == Some("interrupted") => {
            RuntimeState::Interrupted
        }
        Ok(value)
            if value.get("status").and_then(Value::as_str) == Some("max_iterations_exceeded") =>
        {
            RuntimeState::Failed
        }
        Ok(_) => RuntimeState::Completed,
    }
}
