//! What happens when a turn finishes (roadmap B5): a notice in the Inbox for a finished
//! automation run, and the optional webhook.

use super::*;

use crate::automation::AutomationStore;
use crate::webhook::{self, Completion};

impl RuntimeHost {
    /// Announce a turn that ended with `final_text`. Never fails the turn: a notice or post that
    /// can not be made goes to the ledger and nowhere else.
    pub(super) fn announce_completion(&self, final_text: &str) {
        let Some(authorities) = &self.authorities else {
            return;
        };
        let state_dir = authorities.state_dir().to_path_buf();
        let artifacts = self.turn_artifacts.lock().unwrap().clone();
        let summary = webhook::summarize(final_text, webhook::SUMMARY_CHARS);
        let context = AutomationStore::open(&state_dir)
            .and_then(|store| store.completion_context(&self.session_id));
        let (source, title) = match context {
            // An automation that asked for no notice gets neither the notice nor the post.
            Ok(Some(context)) if !context.wants_notice => return,
            Ok(Some(context)) => {
                let files: Vec<Value> = artifacts
                    .iter()
                    .map(|(path, name)| json!({"path": path, "name": name}))
                    .collect();
                let body = if summary.is_empty() {
                    "The run finished.".to_owned()
                } else {
                    summary.clone()
                };
                let added = authorities.inbox.add_notice_with_data(
                    &self.session_id,
                    &format!("done:{}", context.run_id),
                    &format!("Finished: {}", context.title),
                    &body,
                    json!({"completion": true, "run_id": context.run_id, "artifacts": files}),
                );
                if let Err(error) = added {
                    let _ = self.ledger_append(
                        "notification.failed",
                        "runtime",
                        json!({"stage": "inbox", "error": error.to_string()}),
                    );
                }
                ("automation", Some(context.title))
            }
            Ok(None) => ("chat", None),
            Err(error) => {
                let _ = self.ledger_append(
                    "notification.failed",
                    "runtime",
                    json!({"stage": "context", "error": error.to_string()}),
                );
                ("chat", None)
            }
        };
        webhook::deliver_in_background(
            state_dir,
            self.config.workspace.clone().unwrap_or_default(),
            Completion {
                session_id: self.session_id.clone(),
                source,
                title,
                summary,
                artifacts,
                finished_at: now_ts() as u64,
            },
        );
    }
}
