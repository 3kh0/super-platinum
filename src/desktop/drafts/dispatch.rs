//! The only dispatch path crosses SQLite first; callbacks never choose an account.
use super::*;
use super_platinum_core::drafts::sync::ConsumeKind;

#[derive(Clone, Debug)]
pub(crate) enum Outcome {
    Draft(RemoteDraft),
    Deleted {
        draft_id: String,
    },
    Sent {
        client_msg_id: String,
        draft_id: Option<String>,
        ts: String,
    },
    Scheduled {
        draft: RemoteDraft,
        post_at: i64,
    },
    /// Only an explicit server rejection, never absence, transport failure or timeout.
    Rejected,
    Uncertain,
}
#[derive(Debug)]
pub(crate) struct DispatchResult {
    pub operation: Operation,
    pub stored: Stored,
    /// Whether this receipt changed the matching operation; not proof of delivery.
    pub applied: bool,
}
impl Owner {
    /// The caller clears its visible composer only after this paired commit.
    /// Later typing is a separate desired snapshot while this intent is frozen.
    pub async fn consume(&self, expected: u64, kind: ConsumeKind) -> Result<Stored, Fault> {
        let _guard = self.gate.lock().await;
        let view = self.current().await?;
        let mut stored = view.ready(expected)?;
        if local_snapshot(&self.scope.key, &view.local)?.as_ref() != stored.state.desired() {
            return Err(Fault::Stale);
        }
        stored
            .state
            .request_consume(&self.scope, stored.state.local_revision(), kind)
            .map_err(|_| Fault::Pending)?;
        self.save(Some(expected), LocalDraft::default(), stored.state)
            .await
    }
    /// One explicit attempt. The lock is released during network I/O so newer
    /// edits can commit. Neither a dropped future nor a lost response retries it.
    pub async fn dispatch<F, Fut>(&self, expected: u64, send: F) -> Result<DispatchResult, Fault>
    where
        F: FnOnce(Operation) -> Fut,
        Fut: Future<Output = Outcome>,
    {
        let operation = {
            let _guard = self.gate.lock().await;
            self.current().await?.ready(expected)?;
            match self
                .account
                .backend
                .prepare(self.scope.clone(), expected)
                .await?
            {
                Prepare::Prepared { operation, .. } => operation,
                Prepare::Blocked(_) => return Err(Fault::Pending),
                Prepare::Stale(_) => return Err(Fault::Stale),
            }
        };
        let result = send(operation.clone()).await;
        self.record(operation, result).await
    }
    /// This same immutable receipt entry point also supports delayed readback.
    /// CAS retries only replay a pure state transition, never a Slack request.
    pub async fn record(
        &self,
        operation: Operation,
        outcome: Outcome,
    ) -> Result<DispatchResult, Fault> {
        if operation.scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        let _guard = self.gate.lock().await;
        for _ in 0..4 {
            let view = self.current().await?;
            let mut stored = view.stored.ok_or(Fault::Unmanaged)?;
            if stored.state.pending() != Some(&operation) {
                return Ok(DispatchResult {
                    operation,
                    stored,
                    applied: false,
                });
            }
            let applied = apply(&mut stored.state, &operation, &outcome);
            if !applied {
                stored.state.mark_uncertain(&self.scope, &operation.id);
            }
            match self
                .save(Some(stored.revision), view.local, stored.state)
                .await
            {
                Ok(stored) => {
                    return Ok(DispatchResult {
                        operation,
                        stored,
                        applied,
                    });
                }
                Err(Fault::Stale) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Fault::Stale)
    }
    /// A remote edit remains scoped to its owning account even in a background
    /// workspace. Unsupported content becomes an explicit conflict, not an edit
    /// through the lossy display parser and not permission to overwrite Slack.
    pub async fn observe(&self, remote: RemoteDraft) -> Result<Stored, Fault> {
        if remote.scope != self.scope {
            return Err(Fault::InvalidReceipt);
        }
        let _guard = self.gate.lock().await;
        for _ in 0..4 {
            let view = self.current().await?;
            let mut stored = view.stored.ok_or(Fault::Unmanaged)?;
            let old = stored.state.desired().cloned();
            let projection = codec::decode(&self.scope.key, &remote.snapshot);
            let changed = if projection.is_ok() {
                stored.state.observe(remote.clone())
            } else {
                stored.state.observe_conflict(remote.clone())
            };
            if !changed {
                return Ok(stored);
            }
            let local = if old.as_ref() != stored.state.desired() {
                let desired = stored.state.desired().ok_or(Fault::Unsupported)?;
                let projection =
                    codec::decode(&self.scope.key, desired).map_err(|_| Fault::Unsupported)?;
                LocalDraft {
                    composer: projection.composer().clone(),
                    attachments: vec![],
                }
            } else {
                view.local
            };
            match self.save(Some(stored.revision), local, stored.state).await {
                Ok(stored) => return Ok(stored),
                Err(Fault::Stale) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Fault::Stale)
    }
}
fn apply(state: &mut DraftSync, operation: &Operation, outcome: &Outcome) -> bool {
    match outcome {
        Outcome::Draft(remote) => {
            let accepted = state.confirm(&operation.id, remote.clone());
            if !accepted {
                state.observe_conflict(remote.clone());
            }
            accepted
        }
        Outcome::Deleted { draft_id } => {
            state.confirm_deleted(&operation.scope, &operation.id, draft_id)
        }
        Outcome::Sent {
            client_msg_id,
            draft_id,
            ts,
        } => state.confirm_sent(
            &operation.scope,
            &operation.id,
            client_msg_id,
            draft_id.as_deref(),
            ts,
        ),
        Outcome::Scheduled { draft, post_at } => {
            let accepted = state.confirm_scheduled(&operation.id, draft.clone(), *post_at);
            if !accepted {
                state.observe_conflict(draft.clone());
            }
            accepted
        }
        Outcome::Rejected => state.reject(&operation.scope, &operation.id),
        Outcome::Uncertain => state.mark_uncertain(&operation.scope, &operation.id),
    }
}
