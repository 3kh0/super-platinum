//! Immutable send/schedule ownership. Persist every transition with local state.
use super::*;

fn message_timestamp(value: &str) -> bool {
    value.split_once('.').is_some_and(|(seconds, fraction)| {
        !seconds.is_empty()
            && fraction.len() == 6
            && seconds
                .bytes()
                .chain(fraction.bytes())
                .all(|c| c.is_ascii_digit())
            && seconds.parse::<u64>().is_ok_and(|seconds| seconds > 0)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsumeKind {
    Send,
    Schedule { post_at: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumeIntent {
    pub id: String,
    pub scope: Scope,
    pub client_msg_id: String,
    /// Revision of the content consumed, before detaching the visible composer.
    pub local_revision: u64,
    pub kind: ConsumeKind,
    pub snapshot: Snapshot,
    /// Identity known at intent capture. An already pending create can bind its
    /// returned ID later, in Operation.base, without rewriting this intent.
    pub base: Option<RemoteDraft>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConsumeError {
    #[error("the composer account or destination changed")]
    Scope,
    #[error("the composer changed before this action could be saved")]
    Stale,
    #[error("another draft action must finish or be recovered first")]
    Busy,
    #[error("there is no content to send or schedule")]
    Empty,
    #[error("the scheduled time must be positive")]
    Schedule,
    #[error("newer typing must be preserved separately before restoring this draft")]
    NewerEdits,
    #[error("the local draft revision is exhausted")]
    RevisionExhausted,
}

impl DraftSync {
    pub fn consumption(&self) -> Option<&ConsumeIntent> {
        self.consumption.as_ref()
    }

    /// Freeze exact content, then detach it from the visible composer. Commit
    /// this state and an empty LocalDraft atomically before clearing native UI.
    /// New typing belongs to `desired`, while this intent remains immutable.
    pub fn request_consume(
        &mut self,
        scope: &Scope,
        expected_local_revision: u64,
        kind: ConsumeKind,
    ) -> Result<ConsumeIntent, ConsumeError> {
        if scope != &self.scope {
            return Err(ConsumeError::Scope);
        }
        if expected_local_revision != self.local_revision {
            return Err(ConsumeError::Stale);
        }
        if self.consumption.is_some()
            || matches!(
                self.workflow,
                Workflow::Conflict { .. } | Workflow::Rejected(_)
            )
            || self
                .pending()
                .is_some_and(|op| !matches!(op.kind, OperationKind::Create | OperationKind::Update))
        {
            return Err(ConsumeError::Busy);
        }
        if matches!(kind, ConsumeKind::Schedule {post_at} if post_at <= 0) {
            return Err(ConsumeError::Schedule);
        }
        let snapshot = self.desired.clone().ok_or(ConsumeError::Empty)?;
        let detached_revision = self
            .local_revision
            .checked_add(1)
            .ok_or(ConsumeError::RevisionExhausted)?;
        let intent = ConsumeIntent {
            id: uuid::Uuid::new_v4().to_string(),
            scope: self.scope.clone(),
            client_msg_id: self.client_msg_id.clone(),
            local_revision: self.local_revision,
            kind,
            snapshot,
            base: self.acknowledged.clone(),
        };
        self.desired = None;
        self.local_revision = detached_revision;
        self.tombstone = false;
        self.consumption = Some(intent.clone());
        Ok(intent)
    }

    /// A definite rejection parks the entire consume sequence, including any
    /// create/update that preceded it. Only an explicit user retry unlocks it.
    pub fn retry_consumption(&mut self, scope: &Scope, intent_id: &str) -> bool {
        if scope != &self.scope
            || self
                .consumption
                .as_ref()
                .is_none_or(|intent| intent.id != intent_id)
            || !matches!(self.workflow, Workflow::Rejected(_))
        {
            return false;
        }
        self.workflow = Workflow::Idle;
        true
    }

    /// Restore unsent content only when doing so cannot replace newer typing.
    /// Otherwise keep the frozen intent for an explicit separate-copy workflow.
    /// In-flight/uncertain requests can never be canceled as if they failed.
    pub fn cancel_consumption(
        &mut self,
        scope: &Scope,
        intent_id: &str,
    ) -> Result<Snapshot, ConsumeError> {
        if scope != &self.scope {
            return Err(ConsumeError::Scope);
        }
        let intent = self.consumption.as_ref().ok_or(ConsumeError::Stale)?;
        if intent.id != intent_id {
            return Err(ConsumeError::Stale);
        }
        if !matches!(self.workflow, Workflow::Idle | Workflow::Rejected(_)) {
            return Err(ConsumeError::Busy);
        }
        if self.desired.is_some()
            || intent.local_revision.checked_add(1) != Some(self.local_revision)
        {
            return Err(ConsumeError::NewerEdits);
        }
        let revision = self
            .local_revision
            .checked_add(1)
            .ok_or(ConsumeError::RevisionExhausted)?;
        let snapshot = intent.snapshot.clone();
        self.desired = Some(snapshot.clone());
        self.local_revision = revision;
        self.tombstone = false;
        self.consumption = None;
        self.workflow = Workflow::Idle;
        Ok(snapshot)
    }

    pub(super) fn begin_consumption(&mut self) -> Option<Operation> {
        let intent = self.consumption.as_ref()?;
        let op = Operation {
            id: uuid::Uuid::new_v4().to_string(),
            scope: intent.scope.clone(),
            client_msg_id: intent.client_msg_id.clone(),
            local_revision: intent.local_revision,
            kind: match intent.kind {
                ConsumeKind::Send => OperationKind::Send,
                ConsumeKind::Schedule { post_at } => OperationKind::Schedule { post_at },
            },
            snapshot: Some(intent.snapshot.clone()),
            base: self.acknowledged.clone(),
            consume_id: Some(intent.id.clone()),
        };
        self.workflow = Workflow::InFlight(op.clone());
        Some(op)
    }

    fn consumption_operation(&self, scope: &Scope, operation_id: &str) -> Option<&Operation> {
        if scope != &self.scope
            || !matches!(
                self.workflow,
                Workflow::InFlight(_) | Workflow::Uncertain(_)
            )
        {
            return None;
        }
        let op = self.pending()?;
        let intent = self.consumption.as_ref()?;
        (op.id == operation_id
            && op.consume_id.as_deref() == Some(intent.id.as_str())
            && op.client_msg_id == intent.client_msg_id
            && op.snapshot.as_ref() == Some(&intent.snapshot))
        .then_some(op)
    }

    /// Supply only a positively confirmed message receipt from the captured
    /// dispatch (or exact sender/channel/client-ID history evidence). A missing
    /// draft or a draft_delete event is not such a receipt.
    pub fn confirm_sent(
        &mut self,
        scope: &Scope,
        operation_id: &str,
        client_msg_id: &str,
        remote_id: Option<&str>,
        message_ts: &str,
    ) -> bool {
        let Some(op) = self.consumption_operation(scope, operation_id) else {
            return false;
        };
        if op.kind != OperationKind::Send
            || op.client_msg_id != client_msg_id
            || op.base.as_ref().map(|base| base.id.as_str()) != remote_id
            || !message_timestamp(message_ts)
        {
            return false;
        }
        self.finish_consumption(remote_id.map(str::to_owned));
        true
    }

    /// A scheduling receipt must positively report the requested schedule and
    /// exact content/identity. Ordinary active-draft observations never suffice.
    pub fn confirm_scheduled(
        &mut self,
        operation_id: &str,
        remote: RemoteDraft,
        post_at: i64,
    ) -> bool {
        let Some(op) = self.consumption_operation(&remote.scope, operation_id) else {
            return false;
        };
        if op.kind != (OperationKind::Schedule { post_at })
            || post_at <= 0
            || !self.valid_remote(&remote)
            || self.obsolete(&remote)
            || self.equal_revision_conflict(&remote)
            || remote.client_msg_id != op.client_msg_id
            || !Self::belongs_to_operation(op, &remote)
            || op.snapshot.as_ref() != Some(&remote.snapshot)
            || op.base.as_ref().is_some_and(|base| {
                ts_key(&remote.last_updated_ts) <= ts_key(&base.last_updated_ts)
            })
        {
            return false;
        }
        self.finish_consumption(Some(remote.id));
        true
    }

    fn finish_consumption(&mut self, remote_id: Option<String>) {
        let intent = self.consumption.take().unwrap();
        if let Some(id) = remote_id
            && !self.retired_ids.contains(&id)
        {
            self.retired_ids.push(id);
        }
        self.acknowledged = None;
        self.acknowledged_local_revision = intent.local_revision + 1;
        self.client_msg_id = uuid::Uuid::new_v4().to_string();
        self.workflow = Workflow::Idle;
        // desired/local_revision/tombstone describe newer input or clearing.
        // Never replace them with the older consumed snapshot.
    }

    /// Handle observations that must not turn a frozen consumption into a plain
    /// draft acknowledgment or overwrite the newly detached composer.
    pub(super) fn observe_consumption(&mut self, remote: &RemoteDraft) -> bool {
        let intent = self.consumption.as_ref().unwrap();
        if self
            .pending()
            .is_some_and(|op| matches!(op.kind, OperationKind::Create | OperationKind::Update))
            && !matches!(self.workflow, Workflow::Rejected(_))
        {
            return false;
        }
        let belongs = self.acknowledged.as_ref().map_or_else(
            || remote.client_msg_id == intent.client_msg_id,
            |base| base.id == remote.id,
        );
        if !belongs {
            return false;
        }
        if self.equal_revision_conflict(remote) {
            self.conflict(remote.clone());
        } else if remote.snapshot == intent.snapshot
            || self
                .acknowledged
                .as_ref()
                .is_some_and(|base| base.snapshot == remote.snapshot)
        {
            self.acknowledged = Some(remote.clone());
        } else {
            self.conflict(remote.clone());
        }
        true
    }

    /// Fail closed when durable state contains inconsistent consumption links.
    pub(crate) fn consumption_valid(&self) -> bool {
        let terminal = self.pending().is_some_and(|op| {
            matches!(
                op.kind,
                OperationKind::Send | OperationKind::Schedule { .. }
            )
        });
        let Some(intent) = &self.consumption else {
            return !terminal
                && !matches!(self.workflow, Workflow::Rejected(_))
                && self.pending().is_none_or(|op| op.consume_id.is_none());
        };
        if intent.scope != self.scope
            || intent.client_msg_id != self.client_msg_id
            || !nonnil_uuid(&intent.id)
            || intent.local_revision >= self.local_revision
            || matches!(intent.kind, ConsumeKind::Schedule {post_at} if post_at <= 0)
            || intent.base.as_ref().is_some_and(|base| {
                base.scope != self.scope
                    || self
                        .acknowledged
                        .as_ref()
                        .is_some_and(|current| current.id != base.id)
            })
        {
            return false;
        }
        self.pending().is_none_or(|op| {
            if terminal {
                op.consume_id.as_deref() == Some(intent.id.as_str())
                    && op.local_revision == intent.local_revision
                    && op.client_msg_id == intent.client_msg_id
                    && op.snapshot.as_ref() == Some(&intent.snapshot)
                    && op.kind
                        == match intent.kind {
                            ConsumeKind::Send => OperationKind::Send,
                            ConsumeKind::Schedule { post_at } => {
                                OperationKind::Schedule { post_at }
                            }
                        }
            } else {
                op.consume_id.is_none()
                    && matches!(op.kind, OperationKind::Create | OperationKind::Update)
            }
        })
    }
}

#[cfg(test)]
mod tests;
