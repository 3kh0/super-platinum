//! Feature-only fixed fixture authority. No path, credential, SQL, or send callback.
use super::*;
use crate::drafts::{DraftKey, sync::Scope};
use crate::slash_commands::{
    Owner,
    catalog::{Catalog, Choice},
    controller::{Delivery, FixtureContext},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashFixtureTarget {
    Channel,
    Thread,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashFixtureOutcome {
    Clear,
    /// Final fixed response waits for an explicit fixture release after dispatch.
    HeldClear,
    KeepInput,
    Rejected,
    Unknown,
    NotSent,
}

#[derive(Default)]
struct ResultGate {
    entered: bool,
    released: bool,
}

pub struct SlashFixture {
    queue: Arc<Queue>,
    scope: Scope,
    context: FixtureContext,
    outcome: SlashFixtureOutcome,
    prepared: Mutex<Option<SlashInput>>,
    requests: Mutex<Vec<Vec<(String, String)>>>,
    result_gate: Mutex<ResultGate>,
    result_changed: tokio::sync::Notify,
}
impl SlashFixture {
    pub(in crate::drafts::store) fn create(
        queue: Arc<Queue>,
        target: SlashFixtureTarget,
        outcome: SlashFixtureOutcome,
    ) -> Result<Self, p::Error> {
        // No queued/opened caller exists yet. This fresh private owner is seeded
        // completely before its first inspect/open/cold; never a configured store.
        let mut store = DraftStore::open(queue.owner.path()).map_err(|_| p::Error::Storage)?;
        p::slash_runtime::seed_fresh_fixture(&mut store, queue.account())?;
        drop(store);
        let scope = Scope {
            account_id: queue.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: match target {
                    SlashFixtureTarget::Channel => None,
                    SlashFixtureTarget::Thread => Some("10.000001".into()),
                },
            },
        };
        let context = FixtureContext::new(Owner {
            account: scope.account_id.clone(),
            team: scope.key.team.clone(),
            user: "U1".into(),
            channel: scope.key.channel.clone(),
            thread: scope.key.thread.clone(),
        })
        .map_err(|_| p::Error::Unsupported)?;
        Ok(Self {
            queue,
            scope,
            context,
            outcome,
            prepared: Mutex::new(None),
            requests: Mutex::new(Vec::new()),
            result_gate: Mutex::default(),
            result_changed: tokio::sync::Notify::new(),
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn catalog(&self) -> &Catalog {
        self.context.catalog()
    }
    /// Actual disconnected identity carrier; cannot acquire an HTTP client.
    pub fn transport(&self) -> Arc<crate::slack::Transport> {
        self.context.transport().clone()
    }
    /// Read-only revocation observation, never a way to renew authority.
    pub fn is_live(&self) -> bool {
        self.context.is_live()
    }
    pub fn revoke(&self) {
        self.context.revoke();
    }
    /// Actual transport-result entry, not a timer or a durable completion signal.
    /// Only HeldClear can wait; the fixed response is released separately once.
    pub async fn wait_dispatched(&self) -> Result<(), AdmissionError> {
        if self.outcome != SlashFixtureOutcome::HeldClear {
            return Err(AdmissionError::Unsupported);
        }
        loop {
            let changed = self.result_changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self
                .result_gate
                .lock()
                .map_err(|_| AdmissionError::Dead)?
                .entered
            {
                return Ok(());
            }
            changed.await;
        }
    }
    pub fn release_result(&self) -> Result<(), AdmissionError> {
        let mut gate = self.result_gate.lock().map_err(|_| AdmissionError::Dead)?;
        if self.outcome != SlashFixtureOutcome::HeldClear || !gate.entered || gate.released {
            return Err(AdmissionError::Stale);
        }
        gate.released = true;
        drop(gate);
        self.result_changed.notify_waiters();
        Ok(())
    }
    async fn held_result(&self) {
        self.result_gate.lock().unwrap().entered = true;
        self.result_changed.notify_waiters();
        loop {
            let changed = self.result_changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.result_gate.lock().unwrap().released {
                return;
            }
            changed.await;
        }
    }
    /// Exactly one prepared input for this fresh context. Storage still compares
    /// the full review/writer/serial and stream; this method mints no native proof.
    pub fn prepare(
        &self,
        ops: &Operations,
        stream: p::Stream,
        review: p::Review,
        writer: p::Writer,
        serial: u64,
        choice: &Choice,
        client_token: &str,
    ) -> Result<SlashInput, AdmissionError> {
        if !Arc::ptr_eq(&self.queue, &ops.queue)
            || review.scope() != &self.scope
            || stream.account() != self.scope.account_id
            || stream.team() != self.scope.key.team
            || !stream.is_live()
            || writer.scope() != &self.scope
            || writer.enrollment() != review.row().lineage().enrollment()
        {
            return Err(AdmissionError::Stale);
        }
        let mut slot = self.prepared.lock().map_err(|_| AdmissionError::Dead)?;
        if slot.is_some() {
            return Err(AdmissionError::Stale);
        }
        let plan = self
            .context
            .plan(&review, choice, client_token)
            .map_err(|e| match e {
                crate::slash_commands::Failure::Unsupported => AdmissionError::Unsupported,
                _ => AdmissionError::Stale,
            })?;
        let input = SlashInput::from_prepared(stream, review, writer, serial, Arc::new(plan));
        *slot = Some(input.clone());
        Ok(input)
    }
    /// Sanitized forms only; fixed synthetic token is removed even in fixtures.
    pub fn requests(&self) -> Vec<Vec<(String, String)>> {
        self.requests.lock().unwrap().clone()
    }
    pub async fn execute(&self, completion: &SlashCompletion) -> Result<(), SlashExecuteError> {
        let same = Arc::ptr_eq(&self.queue, &completion.queue)
            && self
                .prepared
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|input| Arc::ptr_eq(&input.inner, &completion.held.input()));
        if !same {
            return Err(SlashExecuteError::BeforeDispatch(AdmissionError::Stale));
        }
        self.queue
            .worker
            .fixture_execute_slash(&completion.held, |permit| async move {
                // The exact fixed context owns this prepared request. Recheck permanent
                // revocation immediately at the final seam, just as the real transport.
                if !permit.current(self.context.transport()) {
                    return permit.report(Delivery::NotSent, None);
                }
                let crate::slack::client::RequestBody::Form(fields) = &permit.request().body else {
                    return permit.report(Delivery::NotSent, None);
                };
                self.requests.lock().unwrap().push(
                    fields
                        .iter()
                        .filter(|(key, _)| key != "token")
                        .cloned()
                        .collect(),
                );
                if self.outcome == SlashFixtureOutcome::HeldClear {
                    // Handoff already occurred. Revocation cannot rewrite the observed
                    // response into NotSent; storage still checks the original writer.
                    self.held_result().await;
                }
                let delivery = match self.outcome {
                    SlashFixtureOutcome::Clear | SlashFixtureOutcome::HeldClear => {
                        Delivery::Reply(serde_json::json!({"ok":true}))
                    }
                    SlashFixtureOutcome::KeepInput => {
                        Delivery::Reply(serde_json::json!({"ok":true,"keep_input":true}))
                    }
                    SlashFixtureOutcome::Rejected => {
                        Delivery::Reply(serde_json::json!({"ok":false,"error":"invalid_command"}))
                    }
                    SlashFixtureOutcome::Unknown => Delivery::Unknown,
                    SlashFixtureOutcome::NotSent => Delivery::NotSent,
                };
                permit.report(delivery, None)
            })
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "owned_slash_fixture_tests.rs"]
mod tests;
