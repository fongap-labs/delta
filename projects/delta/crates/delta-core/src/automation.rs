//! Rust Application Control Plane authority for automations, scheduler state,
//! and run identity.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::Local;
use cron::Schedule;
use rusqlite::{params, Connection, TransactionBehavior};
use serde_json::{json, Map, Value};

use crate::{ShadowReadError, TaskStore};

/// The most runs an hour one folder-triggered automation starts.
const FOLDER_RUNS_PER_HOUR: usize = 6;
/// Consecutive denials of one rule after which an automation is paused.
pub const DEFAULT_DENIAL_THRESHOLD: u32 = 3;
/// The most denials a user may allow before the pause (A3).
const MAX_DENIAL_THRESHOLD: u32 = 20;

/// What the runtime needs to know about the automation a finished run belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    pub run_id: String,
    pub title: String,
    /// The automation's `notify_on_completion` setting.
    pub wants_notice: bool,
}

pub struct AutomationStore {
    tasks: TaskStore,
    db_path: PathBuf,
    state_dir: PathBuf,
}

impl AutomationStore {
    pub fn open(state_dir: impl AsRef<Path>) -> Result<Self, ShadowReadError> {
        let state_dir = state_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&state_dir)?;
        let db_path = state_dir.join("automation.db");
        Ok(Self {
            tasks: TaskStore::open(&db_path)?,
            db_path,
            state_dir,
        })
    }

    pub fn list(&self) -> Result<Vec<Value>, ShadowReadError> {
        self.tasks
            .list_tasks()?
            .into_iter()
            .map(|entry| {
                let mut data = object_or_empty(entry.data);
                data.remove("agent");
                data.insert("id".to_string(), Value::String(entry.id));
                data.insert("enabled".to_string(), Value::Bool(entry.enabled));
                data.insert(
                    "next_run".to_string(),
                    entry.next_run.map(Value::from).unwrap_or(Value::Null),
                );
                Ok(Value::Object(data))
            })
            .collect()
    }

    pub fn create(&self, payload: &Value) -> Result<Value, ShadowReadError> {
        let title = payload
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let instructions = payload
            .get("instructions")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if title.is_empty() || instructions.is_empty() {
            return Ok(json!({"ok": false, "error": "title and instructions are required"}));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let cron = payload.get("cron").and_then(Value::as_str);
        let fire_at = payload.get("fire_at").and_then(Value::as_str);
        // A folder trigger starts a run when a new file arrives in a folder of the workspace.
        let watch_folder = payload
            .get("watch_folder")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|folder| !folder.is_empty());
        let watch_pattern = payload
            .get("watch_pattern")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty());
        let workspace_text = payload
            .get("workspace")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut folder_label = None;
        if let Some(folder) = watch_folder {
            if cron.is_some_and(|value| !value.trim().is_empty())
                || fire_at.is_some_and(|value| !value.trim().is_empty())
            {
                return Ok(
                    json!({"ok": false, "error": "choose one trigger: a schedule or a folder"}),
                );
            }
            match crate::folder_trigger::resolve_folder(workspace_text, folder) {
                Ok((_, relative)) => folder_label = Some(relative),
                Err(error) => return Ok(json!({"ok": false, "error": error})),
            }
        }
        let next_run = if folder_label.is_some() {
            None
        } else {
            next_run(cron, fire_at)
        };
        let schedule_raw = if let Some(folder) = &folder_label {
            json!({"kind": "folder", "cron": null, "fire_at": null, "folder": folder, "pattern": watch_pattern, "timezone": "local"})
        } else if let Some(cron) = cron {
            json!({"kind": "cron", "cron": cron, "fire_at": null, "timezone": payload.get("timezone").and_then(Value::as_str).unwrap_or("local")})
        } else {
            json!({"kind": "once", "cron": null, "fire_at": fire_at, "timezone": payload.get("timezone").and_then(Value::as_str).unwrap_or("local")})
        };
        let task = json!({
            "id": id,
            "title": title,
            "instructions": instructions,
            "schedule": match &folder_label {
                Some(folder) => format!("folder:{folder}"),
                None => cron.or(fire_at).unwrap_or("manual").to_owned(),
            },
            "schedule_raw": schedule_raw,
            "workspace": payload.get("workspace").and_then(Value::as_str).unwrap_or_default(),
            "enabled": true,
            "next_run": next_run,
            "last_run": null,
            "last_status": null,
            "run_count": 0,
            "notify_on_completion": payload.get("notify_on_completion").and_then(Value::as_bool).unwrap_or(true),
            "seen_runs_at": 0,
            "unseen_runs": 0,
            "unseen_failed": false,
            "always_allowed": payload.get("permissions").cloned().unwrap_or_else(|| json!([])),
        });
        self.tasks
            .save_task(&id, true, next_run, &serde_json::to_string(&task)?)?;
        Ok(json!({"ok": true, "task": task}))
    }

    pub fn get(&self, id: &str) -> Result<Value, ShadowReadError> {
        let Some(entry) = self.tasks.get_task(id)? else {
            return Ok(json!({"task": null, "runs": []}));
        };
        let mut task = object_or_empty(entry.data);
        task.remove("agent");
        task.insert("id".to_string(), Value::String(entry.id));
        task.insert("enabled".to_string(), Value::Bool(entry.enabled));
        task.insert(
            "next_run".to_string(),
            entry.next_run.map(Value::from).unwrap_or(Value::Null),
        );
        let runs = self
            .tasks
            .runs(id, 100)?
            .into_iter()
            .map(|run| run.data)
            .collect::<Vec<_>>();
        Ok(json!({"task": task, "runs": runs}))
    }

    pub fn update(&self, id: &str, changes: &Value) -> Result<Value, ShadowReadError> {
        let Some(entry) = self.tasks.get_task(id)? else {
            return Ok(json!({"ok": false, "error": "automation not found"}));
        };
        let mut task = object_or_empty(entry.data);
        if let Some(changes) = changes.as_object() {
            for (key, value) in changes {
                task.insert(key.clone(), value.clone());
            }
        }
        task.insert("id".to_string(), Value::String(id.to_string()));
        task.remove("agent");
        // Turning an automation back on ends the pause the breaker put it in.
        if changes.get("enabled").and_then(Value::as_bool) == Some(true) {
            task.remove("paused_reason");
            task.remove("paused_rule");
        }
        if let Some(cron) = changes.get("cron").and_then(Value::as_str) {
            task.insert("schedule".to_string(), Value::String(cron.to_string()));
            task.insert(
                "schedule_raw".to_string(),
                json!({"kind": "cron", "cron": cron, "fire_at": null, "timezone": "local"}),
            );
        }
        let enabled = task
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(entry.enabled);
        let next = if enabled {
            next_run(
                task.get("schedule_raw")
                    .and_then(|value| value.get("cron"))
                    .and_then(Value::as_str),
                task.get("schedule_raw")
                    .and_then(|value| value.get("fire_at"))
                    .and_then(Value::as_str),
            )
            .or(entry.next_run)
        } else {
            None
        };
        self.tasks.save_task(
            id,
            enabled,
            next,
            &serde_json::to_string(&Value::Object(task.clone()))?,
        )?;
        Ok(json!({"ok": true, "task": task}))
    }

    pub fn delete(&self, id: &str) -> Result<Value, ShadowReadError> {
        // What the folder trigger remembered about this automation goes with it.
        if let Ok(conn) = self.folder_connection() {
            let _ = conn.execute("DELETE FROM folder_files WHERE task_id = ?", params![id]);
            let _ = conn.execute("DELETE FROM folder_tasks WHERE task_id = ?", params![id]);
        }
        Ok(json!({"ok": self.tasks.delete_task(id)?}))
    }

    pub fn mark_seen(&self, id: &str) -> Result<Value, ShadowReadError> {
        self.update(
            id,
            &json!({"seen_runs_at": now(), "unseen_runs": 0, "unseen_failed": false}),
        )
    }

    pub fn prepare_run(&self, id: &str) -> Result<Value, ShadowReadError> {
        self.prepare_trigger_run(id, "manual")
    }

    fn prepare_trigger_run(&self, id: &str, trigger: &str) -> Result<Value, ShadowReadError> {
        let Some(entry) = self.tasks.get_task(id)? else {
            return Ok(json!({"ok": false, "error": "automation not found"}));
        };
        let run_id = uuid::Uuid::new_v4().to_string();
        let session_id = format!("__run__{run_id}");
        let workspace = entry
            .data
            .get("workspace")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let prompt = entry
            .data
            .get("instructions")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let run = json!({
            "run_id": run_id, "task_id": id, "session_id": session_id,
            "started_at": now(), "finished_at": null, "status": "running",
            "result_text": null, "artifacts": [], "error": null, "trigger": trigger,
        });
        self.tasks
            .add_run(&run_id, id, now(), &serde_json::to_string(&run)?, workspace)?;
        Ok(json!({
            "ok": true, "run_id": run_id, "session_id": session_id,
            "workspace": workspace, "prompt": prompt,
            "task_id": id,
            "task_title": entry.data.get("title").and_then(Value::as_str).unwrap_or("Automation"),
        }))
    }

    pub fn due(&self) -> Result<Vec<Value>, ShadowReadError> {
        Ok(self
            .tasks
            .due_tasks(now())?
            .into_iter()
            .map(|entry| {
                let mut data = object_or_empty(entry.data);
                data.remove("agent");
                data.insert("id".to_string(), Value::String(entry.id));
                Value::Object(data)
            })
            .collect())
    }

    /// Atomically claim every task due at this scheduler tick.
    ///
    /// The previous implementation updated `scheduled_tasks` first and inserted
    /// the run later through a separate TaskStore call. A process crash between
    /// those writes could advance/disable the task without a corresponding run.
    /// This path takes an IMMEDIATE SQLite transaction before reading due work
    /// and commits the schedule advance and run insertion together.
    pub fn claim_due_runs(&self) -> Result<Vec<Value>, ShadowReadError> {
        let current = now();
        let mut conn = Connection::open(&self.db_path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let due = {
            let mut stmt = tx.prepare(
                "SELECT id, data FROM scheduled_tasks \
                 WHERE enabled = 1 AND next_run IS NOT NULL AND next_run <= ? \
                 ORDER BY next_run",
            )?;
            let rows = stmt.query_map(params![current], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut due = Vec::new();
            for row in rows {
                let (id, data) = row?;
                due.push((id, serde_json::from_str::<Value>(&data)?));
            }
            due
        };

        let mut prepared = Vec::with_capacity(due.len());
        for (task_id, data) in due {
            let next = next_task_run(&data, true);
            let enabled = next.is_some();
            let mut task = object_or_empty(data.clone());
            task.insert("enabled".to_string(), Value::Bool(enabled));
            task.insert(
                "next_run".to_string(),
                next.map(Value::from).unwrap_or(Value::Null),
            );
            let task_data = serde_json::to_string(&Value::Object(task))?;

            let run_id = uuid::Uuid::new_v4().to_string();
            let session_id = format!("__run__{run_id}");
            let workspace = data
                .get("workspace")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let prompt = data
                .get("instructions")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let task_title = data
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Automation")
                .to_string();
            let started_at = now();
            let run = json!({
                "run_id": run_id, "task_id": task_id, "session_id": session_id,
                "started_at": started_at, "finished_at": null, "status": "running",
                "result_text": null, "artifacts": [], "error": null, "trigger": "scheduled",
            });
            let run_data = serde_json::to_string(&run)?;

            tx.execute(
                "UPDATE scheduled_tasks SET enabled = ?, next_run = ?, data = ? WHERE id = ?",
                params![enabled as i64, next, task_data, task_id],
            )?;
            tx.execute(
                "INSERT INTO task_runs (run_id, task_id, started_at, data, workspace) \
                 VALUES (?, ?, ?, ?, ?)",
                params![run_id, task_id, started_at, run_data, workspace],
            )?;

            prepared.push(json!({
                "ok": true, "run_id": run_id, "session_id": session_id,
                "workspace": workspace, "prompt": prompt,
                "task_id": task_id, "task_title": task_title,
            }));
        }
        tx.commit()?;
        Ok(prepared)
    }

    fn folder_connection(&self) -> Result<Connection, ShadowReadError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS folder_tasks (
                task_id TEXT PRIMARY KEY,
                baselined INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS folder_files (
                task_id TEXT NOT NULL,
                name TEXT NOT NULL,
                size INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                state TEXT NOT NULL,
                PRIMARY KEY (task_id, name)
            );",
        )?;
        Ok(conn)
    }

    /// Claim a run for every folder-triggered automation whose folder has new files.
    ///
    /// The first look at a folder only records what is already there: files that existed before the
    /// automation was made never start it. A new file starts a run once it has stayed the same size
    /// and time across two looks (so one still being copied is waited for). Files that appear while
    /// a run of the same automation is going are taken to be its own output and never start
    /// another, and at most [`FOLDER_RUNS_PER_HOUR`] runs an hour are started, so a run that
    /// writes into its own folder cannot start itself again and again.
    pub fn claim_folder_runs(&self) -> Result<Vec<Value>, ShadowReadError> {
        let conn = self.folder_connection()?;
        let mut prepared = Vec::new();
        for entry in self.tasks.list_tasks()? {
            let raw = entry
                .data
                .get("schedule_raw")
                .cloned()
                .unwrap_or(Value::Null);
            if !entry.enabled || raw.get("kind").and_then(Value::as_str) != Some("folder") {
                continue;
            }
            let workspace = entry
                .data
                .get("workspace")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let folder = raw.get("folder").and_then(Value::as_str).unwrap_or(".");
            let pattern = raw.get("pattern").and_then(Value::as_str);
            // A folder that is gone (or moved out of the workspace) is skipped, not an error: it
            // may be back by the next look.
            let Ok((directory, relative)) =
                crate::folder_trigger::resolve_folder(workspace, folder)
            else {
                continue;
            };
            let observed = crate::folder_trigger::scan(&directory, pattern);
            let task_id = entry.id.as_str();

            let baselined: bool = conn
                .query_row(
                    "SELECT baselined FROM folder_tasks WHERE task_id = ?",
                    params![task_id],
                    |row| row.get::<_, i64>(0),
                )
                .map(|value| value != 0)
                .unwrap_or(false);
            let remember = |file: &crate::folder_trigger::Observation, state: &str| {
                conn.execute(
                    "INSERT INTO folder_files (task_id, name, size, mtime, state) VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT(task_id, name) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, state = excluded.state",
                    params![task_id, file.name, file.size as i64, file.mtime, state],
                )
            };
            // A file that left the folder is forgotten, so one that comes back is new again.
            let mut known: std::collections::HashMap<String, (i64, i64, String)> = {
                let mut statement = conn.prepare(
                    "SELECT name, size, mtime, state FROM folder_files WHERE task_id = ?",
                )?;
                let rows = statement.query_map(params![task_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        (
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, String>(3)?,
                        ),
                    ))
                })?;
                rows.collect::<Result<_, _>>()?
            };
            for name in known.keys().cloned().collect::<Vec<_>>() {
                if !observed.iter().any(|file| file.name == name) {
                    conn.execute(
                        "DELETE FROM folder_files WHERE task_id = ? AND name = ?",
                        params![task_id, name],
                    )?;
                    known.remove(&name);
                }
            }

            if !baselined {
                for file in &observed {
                    remember(file, "seen")?;
                }
                conn.execute(
                    "INSERT OR REPLACE INTO folder_tasks (task_id, baselined) VALUES (?, 1)",
                    params![task_id],
                )?;
                continue;
            }
            let is_running = self
                .tasks
                .runs(task_id, 5)?
                .iter()
                .any(|run| run.data.get("status").and_then(Value::as_str) == Some("running"));
            let mut ready = Vec::new();
            for file in &observed {
                match known.get(&file.name) {
                    Some((_, _, state)) if state == "seen" => {}
                    // While this automation runs, new files are its own work.
                    _ if is_running => {
                        remember(file, "seen")?;
                    }
                    Some((size, mtime, _)) if *size == file.size as i64 && *mtime == file.mtime => {
                        ready.push(file.clone());
                    }
                    _ => {
                        remember(file, "pending")?;
                    }
                }
            }
            if ready.is_empty() {
                continue;
            }
            let hour_ago = now() - 3600.0;
            let recent = self
                .tasks
                .runs(task_id, FOLDER_RUNS_PER_HOUR * 2)?
                .iter()
                .filter(|run| {
                    run.data.get("trigger").and_then(Value::as_str) == Some("folder")
                        && run
                            .data
                            .get("started_at")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0)
                            > hour_ago
                })
                .count();
            if recent >= FOLDER_RUNS_PER_HOUR {
                // The files stay ready and are taken up when the hour has room again.
                continue;
            }
            let mut run = self.prepare_trigger_run(task_id, "folder")?;
            if run.get("ok").and_then(Value::as_bool) != Some(true) {
                continue;
            }
            let instructions = run
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or_default();
            run["prompt"] = Value::String(format!(
                "{instructions}\n\n{}",
                crate::folder_trigger::describe_arrival(&relative, &ready)
            ));
            for file in &ready {
                remember(file, "seen")?;
            }
            prepared.push(run);
        }
        Ok(prepared)
    }

    fn breaker_connection(&self) -> Result<Connection, ShadowReadError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS approval_denials (
                task_id TEXT NOT NULL,
                rule TEXT NOT NULL,
                count INTEGER NOT NULL,
                PRIMARY KEY (task_id, rule)
            );
            CREATE TABLE IF NOT EXISTS breaker_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(conn)
    }

    /// How many denials in a row of one rule pause an automation.
    pub fn denial_threshold(&self) -> Result<u32, ShadowReadError> {
        let stored: Option<String> = self
            .breaker_connection()?
            .query_row(
                "SELECT value FROM breaker_settings WHERE key = 'denial_threshold'",
                [],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(stored
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| (1..=MAX_DENIAL_THRESHOLD).contains(value))
            .unwrap_or(DEFAULT_DENIAL_THRESHOLD))
    }

    pub fn set_denial_threshold(&self, threshold: u32) -> Result<Value, ShadowReadError> {
        if !(1..=MAX_DENIAL_THRESHOLD).contains(&threshold) {
            return Err(ShadowReadError::Parse(format!(
                "the denial threshold must be between 1 and {MAX_DENIAL_THRESHOLD}"
            )));
        }
        self.breaker_connection()?.execute(
            "INSERT INTO breaker_settings (key, value) VALUES ('denial_threshold', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![threshold.to_string()],
        )?;
        Ok(json!({"ok": true, "denial_threshold": threshold}))
    }

    /// Which automation a session belongs to, and whether it wants a notice when a run finishes.
    /// `None` for a session that is not an automation run.
    pub fn completion_context(
        &self,
        session_id: &str,
    ) -> Result<Option<CompletionContext>, ShadowReadError> {
        let Some(run_id) = session_id.strip_prefix("__run__") else {
            return Ok(None);
        };
        let Some(run) = self.tasks.find_run(run_id)? else {
            return Ok(None);
        };
        let Some(task) = self.tasks.get_task(&run.task_id)? else {
            return Ok(None);
        };
        Ok(Some(CompletionContext {
            run_id: run_id.to_owned(),
            title: task
                .data
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Automation")
                .to_owned(),
            wants_notice: task
                .data
                .get("notify_on_completion")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }))
    }

    /// Feed the user's answer to an approval into the circuit breaker (A3). A denial counts against
    /// the automation and the rule (the tool); an approval of that rule clears the count. When the
    /// count reaches the threshold the automation is paused and the result says so, so the caller
    /// can leave a notice. A session that is not an automation run is not counted.
    pub fn note_approval_outcome(
        &self,
        session_id: &str,
        rule: &str,
        is_approved: bool,
    ) -> Result<Option<Value>, ShadowReadError> {
        let Some(run_id) = session_id.strip_prefix("__run__") else {
            return Ok(None);
        };
        let Some(run) = self.tasks.find_run(run_id)? else {
            return Ok(None);
        };
        let task_id = run.task_id;
        let conn = self.breaker_connection()?;
        if is_approved {
            conn.execute(
                "DELETE FROM approval_denials WHERE task_id = ?1 AND rule = ?2",
                params![task_id, rule],
            )?;
            return Ok(None);
        }
        conn.execute(
            "INSERT INTO approval_denials (task_id, rule, count) VALUES (?1, ?2, 1)
             ON CONFLICT(task_id, rule) DO UPDATE SET count = count + 1",
            params![task_id, rule],
        )?;
        let count: u32 = conn.query_row(
            "SELECT count FROM approval_denials WHERE task_id = ?1 AND rule = ?2",
            params![task_id, rule],
            |row| row.get(0),
        )?;
        let threshold = self.denial_threshold()?;
        if count < threshold {
            return Ok(None);
        }
        let title = self
            .tasks
            .get_task(&task_id)?
            .and_then(|entry| {
                entry
                    .data
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Automation".to_string());
        self.update(
            &task_id,
            &json!({"enabled": false, "paused_reason": "approval_denied", "paused_rule": rule}),
        )?;
        // A fresh start after the user turns it back on.
        conn.execute(
            "DELETE FROM approval_denials WHERE task_id = ?1",
            params![task_id],
        )?;
        Ok(Some(json!({
            "task_id": task_id, "title": title, "rule": rule,
            "count": count, "threshold": threshold,
        })))
    }

    pub fn finalize_run(&mut self, id: &str, run_id: &str) -> Result<Value, ShadowReadError> {
        let Some(run) = self.tasks.find_run(run_id)? else {
            return Ok(json!({"ok": false, "error": "automation run not found"}));
        };
        if run.task_id != id {
            return Ok(json!({"ok": false, "error": "automation run identity mismatch"}));
        }
        let mut data = object_or_empty(run.data);
        data.insert("finished_at".to_string(), Value::from(now()));
        data.insert("status".to_string(), Value::String("ok".to_string()));
        let next = self.tasks.get_task(id)?.and_then(|task| task.next_run);
        self.tasks.complete_run(
            run_id,
            id,
            run.started_at,
            &serde_json::to_string(&Value::Object(data))?,
            &run.workspace,
            now(),
            next,
        )?;
        Ok(json!({"ok": true}))
    }
    pub fn finalize_recovery(
        &mut self,
        session_id: &str,
        recovery: &Value,
    ) -> Result<Value, ShadowReadError> {
        let Some(status) = terminal_status(recovery) else {
            return Ok(json!({"ok": false, "error": "runtime recovery is not terminal"}));
        };
        self.finalize_session(session_id, &status)
    }

    pub fn reconcile_terminal_sessions(&mut self) -> Result<Value, ShadowReadError> {
        let runs = self.tasks.unfinished_runs()?;
        let mut reconciled = 0usize;
        let mut pending = 0usize;
        for run in runs {
            let session_id = run
                .data
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("__run__{}", run.run_id));
            let Some(recovery) =
                crate::control_plane::get_session_recovery(&self.state_dir, &session_id)?
            else {
                pending += 1;
                continue;
            };
            if terminal_status(&recovery).is_none() {
                pending += 1;
                continue;
            }
            let result = self.finalize_recovery(&session_id, &recovery)?;
            if result.get("ok").and_then(Value::as_bool) == Some(true) {
                reconciled += 1;
            } else {
                pending += 1;
            }
        }
        Ok(json!({"ok": true, "reconciled": reconciled, "pending": pending}))
    }

    pub fn finalize_session(
        &mut self,
        session_id: &str,
        status: &str,
    ) -> Result<Value, ShadowReadError> {
        let Some(run_id) = session_id.strip_prefix("__run__") else {
            return Ok(json!({"ok": false, "error": "not an automation session"}));
        };
        let Some(run) = self.tasks.find_run(run_id)? else {
            return Ok(json!({"ok": false, "error": "automation run not found"}));
        };
        let task_id = run.task_id.clone();
        let mut data = object_or_empty(run.data);
        data.insert("finished_at".to_string(), Value::from(now()));
        data.insert("status".to_string(), Value::String(status.to_string()));
        let next = self
            .tasks
            .get_task(&task_id)?
            .and_then(|task| task.next_run);
        self.tasks.complete_run(
            run_id,
            &task_id,
            run.started_at,
            &serde_json::to_string(&Value::Object(data))?,
            &run.workspace,
            now(),
            next,
        )?;
        Ok(json!({"ok": true, "task_id": task_id, "run_id": run_id}))
    }
}

fn terminal_status(recovery: &Value) -> Option<String> {
    match recovery.get("event").and_then(Value::as_str) {
        Some("turn_end") => recovery
            .get("payload")
            .and_then(|payload| payload.get("status"))
            .and_then(Value::as_str)
            .map(str::to_string),
        Some("interrupted") => Some("interrupted".to_string()),
        Some("error")
            if recovery
                .get("payload")
                .and_then(|payload| payload.get("terminal"))
                .and_then(Value::as_bool)
                == Some(true) =>
        {
            Some("failed".to_string())
        }
        _ => None,
    }
}

fn object_or_empty(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

fn next_run(cron: Option<&str>, fire_at: Option<&str>) -> Option<f64> {
    if let Some(fire_at) = fire_at {
        if let Ok(parsed) =
            time::OffsetDateTime::parse(fire_at, &time::format_description::well_known::Rfc3339)
        {
            return Some(parsed.unix_timestamp() as f64);
        }
    }
    cron.filter(|value| !value.trim().is_empty())
        .and_then(|value| {
            let expression = if value.split_whitespace().count() == 5 {
                format!("0 {value}")
            } else {
                value.to_string()
            };
            Schedule::from_str(&expression).ok()
        })
        .and_then(|schedule| schedule.upcoming(Local).next())
        .map(|date| date.timestamp_millis() as f64 / 1000.0)
}

fn next_task_run(task: &Value, is_scheduled_tick: bool) -> Option<f64> {
    let scheduled_tick = is_scheduled_tick;
    let raw = task.get("schedule_raw").unwrap_or(&Value::Null);
    let kind = raw.get("kind").and_then(Value::as_str).unwrap_or_default();
    if kind == "once" && scheduled_tick {
        return None;
    }
    next_run(
        raw.get("cron").and_then(Value::as_str),
        raw.get("fire_at").and_then(Value::as_str),
    )
}

/// The model-facing tool that files a new automation: one approved call creates
/// the task in the Rust authority, with its schedule validated before anything
/// is stored. Like `save_skill`, the write itself is the approval's outcome —
/// the tool carries no file or exec grants of its own.
pub fn create_registration(
    store: std::sync::Arc<std::sync::Mutex<AutomationStore>>,
) -> crate::capability::CapabilityRegistration {
    use crate::capability::{CapabilityControl, CapabilityGrants, CapabilityResult};

    crate::capability::CapabilityRegistration {
        capability_id: "automation.create".to_owned(),
        tool_name: "create_scheduled_task".to_owned(),
        description: "Create a scheduled automation from the user's request: a title, the \
            instructions it should run with, and what starts it: a schedule (a five-field cron \
            expression, or one fire_at time in RFC 3339) or a folder to watch (`watch_folder`, \
            inside the workspace: a run starts when a new file arrives there). The automation \
            runs unattended later; name the permissions it needs only if the user asked for them."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["title", "instructions"],
            "properties": {
                "title": {"type": "string", "description": "Short task name shown in the task center."},
                "instructions": {"type": "string", "description": "What to do on each run, written for an agent running without the user present."},
                "cron": {"type": "string", "description": "Five-field cron expression in local time (for example 0 9 * * 1 for Mondays at 09:00). Either cron or fire_at is required."},
                "fire_at": {"type": "string", "description": "One-off run time in RFC 3339 (for example 2026-11-01T09:00:00+08:00)."},
                "timezone": {"type": "string", "description": "IANA time zone for the schedule; defaults to local time."},
                "watch_folder": {"type": "string", "description": "Instead of a schedule: a folder inside the workspace to watch (relative to it, or . for the workspace itself). A run starts when a new file arrives; files already there do not start it. Needs a workspace."},
                "watch_pattern": {"type": "string", "description": "With watch_folder: only files whose names match, such as *.pdf or *.xlsx, *.csv."},
                "workspace": {"type": "string", "description": "Workspace the automation runs in; defaults to none."},
                "permissions": {
                    "type": "array",
                    "description": "Standing permissions for unattended runs (for example [{\"tool\": \"send_message\", \"target\": \"slack:T1/C1\", \"access\": \"write\"}]).",
                    "items": {"type": "object"}
                }
            }
        }),
        metadata: json!({
            "risk_level": "medium", "requires_approval": true,
            "category": "automation", "capabilities": ["automation.create"]
        }),
        // Files a task record in the Rust automation authority: a reversible
        // local write the user approves in the approval card.
        side_effect: crate::skills::SideEffectClass::LocalWrite,
        grants: CapabilityGrants::default(),
        workspace_write: false,
        runner: std::sync::Arc::new(crate::capability::NativeCapabilityRunner::new(
            move |job, control: &CapabilityControl| {
                if control.is_cancelled() {
                    return CapabilityResult::cancelled(&job.job_id);
                }
                let text = |key: &str| {
                    job.arguments
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                };
                let cron = text("cron");
                let fire_at = text("fire_at");
                let watch_folder = text("watch_folder");
                if cron.is_empty() && fire_at.is_empty() && watch_folder.is_empty() {
                    return CapabilityResult::failed(
                        &job.job_id,
                        "a trigger is required: give a cron expression, a fire_at time or a watch_folder",
                        Some("schedule_required"),
                    );
                }
                // Reuse the store's own parser: a schedule that would never fire
                // is refused here instead of being filed as a manual task.
                if watch_folder.is_empty()
                    && next_run(
                        (!cron.is_empty()).then_some(cron.as_str()),
                        (!fire_at.is_empty()).then_some(fire_at.as_str()),
                    )
                    .is_none()
                {
                    return CapabilityResult::failed(
                        &job.job_id,
                        "the schedule could not be parsed; use a five-field cron expression or an RFC 3339 time",
                        Some("invalid_schedule"),
                    );
                }
                let payload = json!({
                    "title": text("title"),
                    "instructions": text("instructions"),
                    "cron": cron,
                    "fire_at": fire_at,
                    "timezone": text("timezone"),
                    "watch_folder": watch_folder,
                    "watch_pattern": text("watch_pattern"),
                    "workspace": text("workspace"),
                    "permissions": job.arguments.get("permissions").cloned().unwrap_or_else(|| json!([])),
                });
                let guard = store
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match guard.create(&payload) {
                    Ok(result) if result.get("ok").and_then(Value::as_bool) == Some(true) => {
                        CapabilityResult::completed(&job.job_id, result)
                    }
                    Ok(result) => {
                        let reason = result
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("could not create the automation");
                        CapabilityResult::failed(&job.job_id, reason, Some("automation_rejected"))
                    }
                    Err(error) => CapabilityResult::failed(
                        &job.job_id,
                        &error.to_string(),
                        Some("automation_rejected"),
                    ),
                }
            },
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn automation_session(store: &AutomationStore, title: &str) -> (String, String) {
        let created = store
            .create(&json!({"title": title, "instructions": "Run", "cron": "0 9 * * *"}))
            .unwrap();
        let task_id = created["task"]["id"].as_str().unwrap().to_string();
        let prepared = store.prepare_run(&task_id).unwrap();
        (
            task_id,
            prepared["session_id"].as_str().unwrap().to_string(),
        )
    }

    #[test]
    fn repeated_denials_of_one_rule_pause_the_automation() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path()).unwrap();
        let (task_id, session) = automation_session(&store, "Digest");
        assert!(store
            .note_approval_outcome(&session, "write_file", false)
            .unwrap()
            .is_none());
        assert!(store
            .note_approval_outcome(&session, "write_file", false)
            .unwrap()
            .is_none());
        let tripped = store
            .note_approval_outcome(&session, "write_file", false)
            .unwrap()
            .expect("the third denial in a row trips the breaker");
        assert_eq!(tripped["task_id"], task_id.as_str());
        assert_eq!(tripped["title"], "Digest");
        assert_eq!(tripped["rule"], "write_file");
        assert_eq!(tripped["count"], 3);
        let state = store.get(&task_id).unwrap();
        assert_eq!(state["task"]["enabled"], false);
        assert_eq!(state["task"]["paused_reason"], "approval_denied");
        assert_eq!(state["task"]["paused_rule"], "write_file");
    }

    #[test]
    fn an_approval_clears_the_count_and_rules_count_separately() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path()).unwrap();
        let (task_id, session) = automation_session(&store, "Digest");
        for _ in 0..2 {
            assert!(store
                .note_approval_outcome(&session, "write_file", false)
                .unwrap()
                .is_none());
        }
        assert!(store
            .note_approval_outcome(&session, "write_file", true)
            .unwrap()
            .is_none());
        for _ in 0..2 {
            assert!(store
                .note_approval_outcome(&session, "write_file", false)
                .unwrap()
                .is_none());
        }
        for _ in 0..2 {
            assert!(store
                .note_approval_outcome(&session, "web_fetch", false)
                .unwrap()
                .is_none());
        }
        assert_eq!(store.get(&task_id).unwrap()["task"]["enabled"], true);
    }

    #[test]
    fn a_session_that_is_not_an_automation_run_is_not_counted() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path()).unwrap();
        let (task_id, _) = automation_session(&store, "Digest");
        for _ in 0..5 {
            assert!(store
                .note_approval_outcome("chat-session", "write_file", false)
                .unwrap()
                .is_none());
            assert!(store
                .note_approval_outcome("__run__missing", "write_file", false)
                .unwrap()
                .is_none());
        }
        assert_eq!(store.get(&task_id).unwrap()["task"]["enabled"], true);
    }

    #[test]
    fn the_denial_threshold_is_configurable_within_bounds() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path()).unwrap();
        assert_eq!(store.denial_threshold().unwrap(), DEFAULT_DENIAL_THRESHOLD);
        assert!(store.set_denial_threshold(0).is_err());
        assert!(store.set_denial_threshold(21).is_err());
        assert_eq!(store.denial_threshold().unwrap(), DEFAULT_DENIAL_THRESHOLD);
        store.set_denial_threshold(1).unwrap();
        assert_eq!(
            AutomationStore::open(temp.path())
                .unwrap()
                .denial_threshold()
                .unwrap(),
            1
        );
        let (task_id, session) = automation_session(&store, "Digest");
        assert!(store
            .note_approval_outcome(&session, "write_file", false)
            .unwrap()
            .is_some());
        assert_eq!(store.get(&task_id).unwrap()["task"]["enabled"], false);
    }

    #[test]
    fn turning_a_paused_automation_back_on_clears_the_pause_and_the_count() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path()).unwrap();
        let (task_id, session) = automation_session(&store, "Digest");
        for _ in 0..3 {
            store
                .note_approval_outcome(&session, "write_file", false)
                .unwrap();
        }
        store.update(&task_id, &json!({"enabled": true})).unwrap();
        let task = store.get(&task_id).unwrap()["task"].clone();
        assert_eq!(task["enabled"], true);
        assert!(task.get("paused_reason").is_none());
        // The count started over: two more denials do not trip it again.
        for _ in 0..2 {
            assert!(store
                .note_approval_outcome(&session, "write_file", false)
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn automation_crud_and_manual_run_use_rust_taskstore() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path()).unwrap();
        let created = store
            .create(&json!({"title": "Digest", "instructions": "Summarize", "cron": "0 9 * * *"}))
            .unwrap();
        let id = created["task"]["id"].as_str().unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        let prepared = store.prepare_run(id).unwrap();
        assert_eq!(prepared["ok"], true);
        assert_ne!(prepared["run_id"], prepared["session_id"]);
        assert_eq!(store.delete(id).unwrap()["ok"], true);
    }
    #[test]
    fn terminal_reconciliation_is_restart_safe_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = AutomationStore::open(temp.path()).unwrap();
        let created = store
            .create(&json!({"title": "Recover", "instructions": "Run"}))
            .unwrap();
        let task_id = created["task"]["id"].as_str().unwrap().to_string();
        let prepared = store.prepare_run(&task_id).unwrap();
        let session_id = prepared["session_id"].as_str().unwrap().to_string();
        crate::control_plane::ensure_session(temp.path(), &session_id, None, "model").unwrap();
        crate::control_plane::record_runtime_event(
            temp.path(),
            &json!({
                "type": "turn_end", "version": 1, "sessionId": session_id,
                "sequence": 1, "payload": {"status": "completed", "iterations": 1}
            }),
        )
        .unwrap();

        let first = store.reconcile_terminal_sessions().unwrap();
        assert_eq!(first["reconciled"], 1);
        let after_first = store.get(&task_id).unwrap();
        assert_eq!(after_first["task"]["run_count"], 1);
        assert_eq!(after_first["runs"][0]["status"], "completed");

        let second = store.reconcile_terminal_sessions().unwrap();
        assert_eq!(second["reconciled"], 0);
        let after_second = store.get(&task_id).unwrap();
        assert_eq!(after_second["task"]["run_count"], 1);
    }

    #[test]
    fn transient_error_recovery_does_not_finalize_automation() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = AutomationStore::open(temp.path()).unwrap();
        let created = store
            .create(&json!({"title": "Retry", "instructions": "Run"}))
            .unwrap();
        let task_id = created["task"]["id"].as_str().unwrap().to_string();
        let prepared = store.prepare_run(&task_id).unwrap();
        let session_id = prepared["session_id"].as_str().unwrap().to_string();
        crate::control_plane::ensure_session(temp.path(), &session_id, None, "model").unwrap();
        crate::control_plane::record_runtime_event(
            temp.path(),
            &json!({
                "type": "error", "version": 1, "sessionId": session_id,
                "sequence": 1,
                "payload": {"error": "retrying", "error_type": "transient_5xx", "terminal": false}
            }),
        )
        .unwrap();

        let result = store.reconcile_terminal_sessions().unwrap();
        assert_eq!(result["reconciled"], 0);
        assert_eq!(result["pending"], 1);
        let state = store.get(&task_id).unwrap();
        assert_eq!(state["runs"][0]["status"], "running");
        assert_eq!(state["task"]["run_count"], 0);
    }

    #[test]
    fn scheduled_claim_is_single_shot_for_due_one_time_task() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path()).unwrap();
        let created = store
            .create(&json!({
                "title": "Once", "instructions": "Run",
                "fire_at": "2020-01-01T00:00:00Z"
            }))
            .unwrap();
        let id = created["task"]["id"].as_str().unwrap().to_string();
        assert_eq!(store.claim_due_runs().unwrap().len(), 1);
        assert!(store.claim_due_runs().unwrap().is_empty());
        assert_eq!(store.get(&id).unwrap()["runs"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn scheduled_claim_is_single_across_independent_store_connections() {
        let temp = tempfile::tempdir().unwrap();
        let first = AutomationStore::open(temp.path()).unwrap();
        let created = first
            .create(&json!({
                "title": "Once", "instructions": "Run",
                "fire_at": "2020-01-01T00:00:00Z"
            }))
            .unwrap();
        let id = created["task"]["id"].as_str().unwrap().to_string();
        let second = AutomationStore::open(temp.path()).unwrap();

        assert_eq!(first.claim_due_runs().unwrap().len(), 1);
        assert!(second.claim_due_runs().unwrap().is_empty());
        assert_eq!(
            second.get(&id).unwrap()["runs"].as_array().unwrap().len(),
            1
        );
    }

    fn tool_control() -> crate::capability::CapabilityControl {
        use std::sync::atomic::AtomicBool;
        let progress: std::sync::Arc<dyn Fn(crate::capability::CapabilityProgress) + Send + Sync> =
            std::sync::Arc::new(|_| {});
        crate::capability::CapabilityControl::new(
            std::sync::Arc::new(AtomicBool::new(false)),
            progress,
        )
    }

    fn tool_job(arguments: Value) -> crate::capability::CapabilityJob {
        crate::capability::CapabilityJob::new("automation.create", "job-automation-test", arguments)
    }

    fn run_tool(arguments: Value) -> crate::capability::CapabilityResult {
        let store = std::sync::Arc::new(std::sync::Mutex::new(
            AutomationStore::open(std::env::temp_dir().join("automation-tool-tests"))
                .expect("open automation store"),
        ));
        let runner = create_registration(store);
        runner.runner.run(&tool_job(arguments), &tool_control())
    }

    #[test]
    fn the_create_tool_files_a_valid_cron_automation() {
        let temp = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(std::sync::Mutex::new(
            AutomationStore::open(temp.path()).unwrap(),
        ));
        let runner = create_registration(store.clone());
        let result = runner.runner.run(
            &tool_job(json!({
                "title": "Weekly digest",
                "instructions": "Summarize the week.",
                "cron": "0 9 * * 1",
            })),
            &tool_control(),
        );
        assert_eq!(
            result.state,
            crate::capability::CapabilityExitState::Completed
        );
        let task = result.result.unwrap()["task"].clone();
        assert_eq!(task["title"], "Weekly digest");
        assert!(
            task["next_run"].as_f64().is_some(),
            "cron yields a next run"
        );
        let listed = store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .list()
            .unwrap();
        assert_eq!(listed.len(), 1);
    }

    // -- folder trigger (ADR-0090) ----------------------------------------------------------

    fn folder_automation(
        store: &AutomationStore,
        workspace: &Path,
        folder: &str,
        pattern: &str,
    ) -> String {
        let created = store
            .create(&json!({
                "title": "Inbox", "instructions": "Summarize each new file.",
                "workspace": workspace.to_string_lossy(), "watch_folder": folder,
                "watch_pattern": pattern,
            }))
            .unwrap();
        assert_eq!(created["ok"], true, "{created}");
        created["task"]["id"].as_str().unwrap().to_owned()
    }

    fn finish(store: &mut AutomationStore, run: &Value) {
        let finalized = store
            .finalize_run(
                run["task_id"].as_str().unwrap(),
                run["run_id"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(finalized["ok"], true);
    }

    #[test]
    fn a_folder_automation_is_filed_with_no_time_and_a_folder_inside_the_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path().join("state")).unwrap();
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(ws.join("inbox")).unwrap();
        let id = folder_automation(&store, &ws, "inbox", "*.pdf");
        let task = store.get(&id).unwrap()["task"].clone();
        assert_eq!(task["schedule"], "folder:inbox");
        assert_eq!(task["schedule_raw"]["kind"], "folder");
        assert_eq!(task["schedule_raw"]["pattern"], "*.pdf");
        assert!(task["next_run"].is_null());
        // It is never due on a clock.
        assert!(store.claim_due_runs().unwrap().is_empty());

        let workspace = ws.to_string_lossy().to_string();
        for (payload, expected) in [
            (
                json!({"workspace": workspace, "watch_folder": "../elsewhere"}),
                "does not exist",
            ),
            (
                json!({"workspace": workspace, "watch_folder": "inbox", "cron": "0 9 * * *"}),
                "choose one trigger",
            ),
            (
                json!({"watch_folder": "inbox"}),
                "needs the automation to have a workspace",
            ),
            (
                json!({"workspace": workspace, "watch_folder": ".delta"}),
                "does not exist",
            ),
        ] {
            let mut payload = payload;
            payload["title"] = json!("Bad");
            payload["instructions"] = json!("Run");
            let refused = store.create(&payload).unwrap();
            assert_eq!(refused["ok"], false, "{payload}");
            assert!(
                refused["error"].as_str().unwrap().contains(expected),
                "{refused}"
            );
        }
    }

    #[test]
    fn files_already_there_do_not_start_it_and_a_new_one_starts_it_once_it_has_settled() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = AutomationStore::open(temp.path().join("state")).unwrap();
        let ws = temp.path().join("ws");
        let inbox = ws.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(inbox.join("old.pdf"), "old").unwrap();
        folder_automation(&store, &ws, "inbox", "*.pdf");

        // First look: what is there is only remembered.
        assert!(store.claim_folder_runs().unwrap().is_empty());
        assert!(store.claim_folder_runs().unwrap().is_empty());

        // A new file, and one that does not match the pattern.
        std::fs::write(inbox.join("new.pdf"), "new!").unwrap();
        std::fs::write(inbox.join("notes.txt"), "ignored").unwrap();
        // First sighting: not yet, it may still be being written.
        assert!(store.claim_folder_runs().unwrap().is_empty());
        // Unchanged since the last look: it starts a run, told which file arrived.
        let runs = store.claim_folder_runs().unwrap();
        assert_eq!(runs.len(), 1);
        let prompt = runs[0]["prompt"].as_str().unwrap();
        assert!(prompt.starts_with("Summarize each new file."), "{prompt}");
        assert!(prompt.contains("- inbox/new.pdf (4 bytes)"), "{prompt}");
        assert!(
            !prompt.contains("old.pdf") && !prompt.contains("notes.txt"),
            "{prompt}"
        );
        assert_eq!(runs[0]["workspace"], ws.to_string_lossy().to_string());
        assert!(runs[0]["session_id"]
            .as_str()
            .unwrap()
            .starts_with("__run__"));
        let recorded = store
            .tasks
            .runs(runs[0]["task_id"].as_str().unwrap(), 5)
            .unwrap();
        assert_eq!(recorded[0].data["trigger"], "folder");
        finish(&mut store, &runs[0]);

        // Each file starts a run once.
        assert!(store.claim_folder_runs().unwrap().is_empty());
        assert!(store.claim_folder_runs().unwrap().is_empty());
    }

    #[test]
    fn a_file_still_being_written_waits_until_it_stops_changing() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path().join("state")).unwrap();
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        folder_automation(&store, &ws, ".", "");
        assert!(store.claim_folder_runs().unwrap().is_empty());

        std::fs::write(ws.join("big.bin"), "12").unwrap();
        assert!(store.claim_folder_runs().unwrap().is_empty());
        std::fs::write(ws.join("big.bin"), "123456").unwrap(); // grew since the last look
        assert!(store.claim_folder_runs().unwrap().is_empty());
        let runs = store.claim_folder_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert!(runs[0]["prompt"]
            .as_str()
            .unwrap()
            .contains("- big.bin (6 bytes)"));
    }

    #[test]
    fn files_that_appear_while_the_run_is_going_are_its_own_output() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = AutomationStore::open(temp.path().join("state")).unwrap();
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(ws.join("in")).unwrap();
        folder_automation(&store, &ws, "in", "");
        store.claim_folder_runs().unwrap();
        std::fs::write(ws.join("in/a.txt"), "a").unwrap();
        store.claim_folder_runs().unwrap();
        let runs = store.claim_folder_runs().unwrap();
        assert_eq!(runs.len(), 1);

        // The run writes its result into the folder it is watching, while it is still running.
        std::fs::write(ws.join("in/summary.md"), "result").unwrap();
        for _ in 0..3 {
            assert!(store.claim_folder_runs().unwrap().is_empty());
        }
        finish(&mut store, &runs[0]);
        for _ in 0..3 {
            assert!(
                store.claim_folder_runs().unwrap().is_empty(),
                "the output must not start the next run"
            );
        }
        // A genuinely new file afterwards does.
        std::fs::write(ws.join("in/b.txt"), "b").unwrap();
        store.claim_folder_runs().unwrap();
        assert_eq!(store.claim_folder_runs().unwrap().len(), 1);
    }

    #[test]
    fn a_paused_automation_or_a_missing_folder_starts_nothing_and_a_deleted_one_is_forgotten() {
        let temp = tempfile::tempdir().unwrap();
        let store = AutomationStore::open(temp.path().join("state")).unwrap();
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(ws.join("in")).unwrap();
        let id = folder_automation(&store, &ws, "in", "");
        store.claim_folder_runs().unwrap();
        std::fs::write(ws.join("in/a.txt"), "a").unwrap();
        store.update(&id, &json!({"enabled": false})).unwrap();
        for _ in 0..3 {
            assert!(store.claim_folder_runs().unwrap().is_empty());
        }
        store.update(&id, &json!({"enabled": true})).unwrap();
        std::fs::rename(ws.join("in"), ws.join("moved")).unwrap();
        assert!(store.claim_folder_runs().unwrap().is_empty());
        std::fs::rename(ws.join("moved"), ws.join("in")).unwrap();
        store.claim_folder_runs().unwrap();
        assert_eq!(store.claim_folder_runs().unwrap().len(), 1);

        store.delete(&id).unwrap();
        let conn = store.folder_connection().unwrap();
        let left: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM folder_files WHERE task_id = ?",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn no_more_than_the_hourly_limit_of_runs_is_started() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = AutomationStore::open(temp.path().join("state")).unwrap();
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(ws.join("in")).unwrap();
        folder_automation(&store, &ws, "in", "");
        store.claim_folder_runs().unwrap();
        let mut started = 0;
        for n in 0..(FOLDER_RUNS_PER_HOUR + 3) {
            std::fs::write(ws.join(format!("in/f{n}.txt")), "x").unwrap();
            store.claim_folder_runs().unwrap();
            for run in store.claim_folder_runs().unwrap() {
                started += 1;
                finish(&mut store, &run);
            }
        }
        assert_eq!(started, FOLDER_RUNS_PER_HOUR);
    }

    #[test]
    fn the_create_tool_takes_a_folder_in_place_of_a_schedule() {
        let temp = tempfile::tempdir().unwrap();
        let ws = temp.path().join("ws");
        std::fs::create_dir_all(ws.join("drop")).unwrap();
        let result = run_tool(json!({
            "title": "Drop box", "instructions": "File each document.",
            "workspace": ws.to_string_lossy(), "watch_folder": "drop", "watch_pattern": "*.pdf"
        }));
        assert_eq!(
            result.state,
            crate::capability::CapabilityExitState::Completed
        );
        let none = run_tool(json!({"title": "x", "instructions": "y", "watch_folder": "drop"}));
        assert_eq!(none.state, crate::capability::CapabilityExitState::Failed);
    }

    #[test]
    fn the_create_tool_refuses_an_automation_without_a_schedule() {
        let result = run_tool(json!({
            "title": "No schedule",
            "instructions": "Run",
        }));
        assert_eq!(result.state, crate::capability::CapabilityExitState::Failed);
        let diagnostics = result.diagnostics.expect("failure carries diagnostics");
        assert_eq!(diagnostics.error_code.as_deref(), Some("schedule_required"));
    }

    #[test]
    fn the_create_tool_refuses_an_unparsable_schedule() {
        for arguments in [
            json!({"title": "Bad cron", "instructions": "Run", "cron": "not a cron"}),
            json!({"title": "Bad time", "instructions": "Run", "fire_at": "tomorrow morning"}),
        ] {
            let result = run_tool(arguments);
            assert_eq!(result.state, crate::capability::CapabilityExitState::Failed);
            let diagnostics = result.diagnostics.expect("failure carries diagnostics");
            assert_eq!(
                diagnostics.error_code.as_deref(),
                Some("invalid_schedule"),
                "{:?}",
                diagnostics.error_message
            );
        }
    }

    #[test]
    fn the_create_tool_surfaces_the_store_rejection() {
        let result = run_tool(json!({
            "title": "  ",
            "instructions": "Run",
            "cron": "0 9 * * 1",
        }));
        assert_eq!(result.state, crate::capability::CapabilityExitState::Failed);
        let message = result
            .diagnostics
            .expect("failure carries diagnostics")
            .error_message
            .unwrap_or_default();
        assert!(message.contains("title and instructions are required"));
    }

    #[test]
    fn the_create_registration_declares_an_approvable_automation_tool() {
        let store = std::sync::Arc::new(std::sync::Mutex::new(
            AutomationStore::open(std::env::temp_dir().join("automation-tool-tests")).unwrap(),
        ));
        let registration = create_registration(store);
        assert_eq!(registration.tool_name, "create_scheduled_task");
        assert_eq!(registration.metadata["category"], "automation");
        assert_eq!(registration.metadata["requires_approval"], json!(true));
        assert!(!registration.grants.exec);
        assert!(!registration.workspace_write);
    }
}
