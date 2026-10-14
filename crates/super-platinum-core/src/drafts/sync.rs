//! Pure ordinary-draft synchronization state. This module sends no requests.
//!
//! Persist the complete state returned by `begin_operation` successfully BEFORE
//! dispatching its returned operation. `restore_after_restart` must run on load:
//! a persisted in-flight request cannot prove whether Slack accepted the write.
//! Neither an operation UUID nor `client_msg_id` promises server deduplication.
use super::DraftKey;
use crate::state::ts_key;
use serde::{Deserialize, Serialize};
use serde_json::Value;
mod consume;
mod direct_local;
mod observation;
pub mod owned_index;
pub(crate) mod reconciliation;
mod recovery;
pub use consume::{ConsumeError, ConsumeIntent, ConsumeKind};
pub use observation::{DraftObservation, ObservationError, ObservationKind};
pub use recovery::{CheckBlock, CheckTicket};

/// Account identity and the verified destination, never the currently visible
/// composer. Adapters must derive a remote scope from its own destination.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub account_id: String,
    pub key: DraftKey,
}

/// Preserve authoritative Slack blocks and attachment IDs without a text codec.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub blocks: Vec<Value>,
    pub file_ids: Vec<String>,
    pub destinations: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteDraft {
    pub scope: Scope,
    pub id: String,
    pub client_msg_id: String,
    pub last_updated_ts: String,
    pub snapshot: Snapshot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdoptError {
    #[error("the remote draft does not belong to the expected account and destination")]
    Scope,
    #[error("the remote draft identity is missing or invalid")]
    Identity,
    #[error("the remote draft revision is missing or invalid")]
    Revision,
    #[error("the remote draft must have exactly the expected destination")]
    Destination,
    #[error("the remote draft contains malformed blocks or file identifiers")]
    Content,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}
fn nonnil_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil())
}
/// Account identities used by persisted draft owners are non-nil UUIDs.
pub fn valid_account_id(value: &str) -> bool {
    nonnil_uuid(value)
}
fn revision(value: &str) -> bool {
    let (seconds, fraction) = value.split_once('.').unwrap_or((value, "0"));
    !seconds.is_empty()
        && seconds.bytes().all(|c| c.is_ascii_digit())
        && seconds.parse::<u64>().is_ok_and(|v| v > 0)
        && !fraction.is_empty()
        && fraction.bytes().all(|c| c.is_ascii_digit())
        && fraction.parse::<u64>().is_ok()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    Create,
    Update,
    Delete,
    Send,
    Schedule { post_at: i64 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    pub scope: Scope,
    pub client_msg_id: String,
    pub local_revision: u64,
    pub kind: OperationKind,
    pub snapshot: Option<Snapshot>,
    pub base: Option<RemoteDraft>,
    #[serde(default)]
    pub consume_id: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Workflow {
    #[default]
    Idle,
    InFlight(Operation),
    Uncertain(Operation),
    /// A definite consumption failure requires an explicit retry or recovery.
    Rejected(Operation),
    /// Keep both versions and the outstanding receipt; conflict is not failure.
    Conflict {
        remote: Option<RemoteDraft>,
        pending: Option<Operation>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSync {
    scope: Scope,
    client_msg_id: String,
    desired: Option<Snapshot>,
    local_revision: u64,
    acknowledged_local_revision: u64,
    acknowledged: Option<RemoteDraft>,
    workflow: Workflow,
    /// Survives an empty visible composer and an uncertain create/delete.
    tombstone: bool,
    /// Prevent delayed events from resurrecting the previously retired identity.
    retired_ids: Vec<String>,
    #[serde(default)]
    consumption: Option<ConsumeIntent>,
    #[serde(default)]
    check_deadline: Option<recovery::CheckDeadline>,
    #[serde(default)]
    check_ticket: Option<CheckTicket>,
    #[serde(default)]
    observed_events: std::collections::BTreeMap<String, DraftObservation>,
    #[serde(default)]
    blocked_observation: Option<DraftObservation>,
}
impl DraftSync {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            client_msg_id: uuid::Uuid::new_v4().to_string(),
            desired: None,
            local_revision: 0,
            acknowledged_local_revision: 0,
            acknowledged: None,
            workflow: Workflow::Idle,
            tombstone: false,
            retired_ids: vec![],
            consumption: None,
            check_deadline: None,
            check_ticket: None,
            observed_events: Default::default(),
            blocked_observation: None,
        }
    }
    /// Explicitly bind a positively identified foreign remote draft. The caller
    /// must verify authenticated ownership and that the source draft is active
    /// and unscheduled: those lifecycle flags are not part of `RemoteDraft`.
    ///
    /// This validates identity/routing, not composer support. Unknown content and
    /// destination metadata remain verbatim; a separate codec must authorize
    /// editing. No operation is pending, and a later edit updates this same ID.
    pub fn adopt_remote(expected: &Scope, remote: RemoteDraft) -> Result<Self, AdoptError> {
        if &remote.scope != expected
            || !nonnil_uuid(&expected.account_id)
            || !identifier(&expected.key.team)
            || !identifier(&expected.key.channel)
            || expected
                .key
                .thread
                .as_deref()
                .is_some_and(|thread| !revision(thread))
        {
            return Err(AdoptError::Scope);
        }
        if !identifier(&remote.id) || !nonnil_uuid(&remote.client_msg_id) {
            return Err(AdoptError::Identity);
        }
        if !revision(&remote.last_updated_ts) {
            return Err(AdoptError::Revision);
        }
        let [destination] = remote.snapshot.destinations.as_slice() else {
            return Err(AdoptError::Destination);
        };
        let destination = destination.as_object().ok_or(AdoptError::Destination)?;
        let valid_thread = match &expected.key.thread {
            Some(thread) => {
                destination.get("thread_ts").and_then(Value::as_str) == Some(thread.as_str())
            }
            None => !destination.contains_key("thread_ts"),
        };
        if destination.get("channel_id").and_then(Value::as_str)
            != Some(expected.key.channel.as_str())
            || !valid_thread
            || destination
                .get("broadcast")
                .is_some_and(|value| !value.is_boolean())
        {
            return Err(AdoptError::Destination);
        }
        if remote.snapshot.file_ids.iter().any(|id| !identifier(id))
            || remote.snapshot.blocks.iter().any(|block| {
                !block.is_object()
                    || !block
                        .get("type")
                        .and_then(Value::as_str)
                        .is_some_and(identifier)
            })
        {
            return Err(AdoptError::Content);
        }
        Ok(Self {
            scope: expected.clone(),
            client_msg_id: remote.client_msg_id.clone(),
            desired: Some(remote.snapshot.clone()),
            local_revision: 0,
            acknowledged_local_revision: 0,
            acknowledged: Some(remote),
            workflow: Workflow::Idle,
            tombstone: false,
            retired_ids: vec![],
            consumption: None,
            check_deadline: None,
            check_ticket: None,
            observed_events: Default::default(),
            blocked_observation: None,
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn client_msg_id(&self) -> &str {
        &self.client_msg_id
    }
    pub fn desired(&self) -> Option<&Snapshot> {
        self.desired.as_ref()
    }
    pub fn local_revision(&self) -> u64 {
        self.local_revision
    }
    pub fn acknowledged_local_revision(&self) -> u64 {
        self.acknowledged_local_revision
    }
    pub fn acknowledged(&self) -> Option<&RemoteDraft> {
        self.acknowledged.as_ref()
    }
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }
    pub fn tombstone(&self) -> bool {
        self.tombstone
    }
    pub fn pending(&self) -> Option<&Operation> {
        match &self.workflow {
            Workflow::InFlight(op) | Workflow::Uncertain(op) | Workflow::Rejected(op) => Some(op),
            Workflow::Conflict { pending, .. } => pending.as_ref(),
            Workflow::Idle => None,
        }
    }
    pub fn restore_after_restart(mut self) -> Self {
        if let Workflow::InFlight(op) = self.workflow {
            self.workflow = Workflow::Uncertain(op);
        }
        self
    }
    /// The caller supplies a complete content snapshot. `None` means explicit
    /// clearing, never proof that a pending server create failed.
    pub fn edit(&mut self, scope: &Scope, desired: Option<Snapshot>) -> bool {
        if scope != &self.scope || self.desired == desired {
            return false;
        }
        self.local_revision = self.local_revision.saturating_add(1);
        self.tombstone = desired.is_none();
        self.desired = desired;
        true
    }
    /// A returned operation authorizes one dispatch only AFTER durable commit.
    /// Coalesced edits remain in `desired`; outstanding/uncertain work blocks it.
    pub fn begin_operation(&mut self, scope: &Scope) -> Option<Operation> {
        if scope != &self.scope
            || self.workflow != Workflow::Idle
            || self.blocked_observation.is_some()
        {
            return None;
        }
        if self.consumption.is_some() {
            return self.begin_consumption();
        }
        let kind = match (&self.desired, &self.acknowledged) {
            (Some(_), None) => OperationKind::Create,
            (Some(want), Some(base)) if want != &base.snapshot => OperationKind::Update,
            (None, Some(_)) => OperationKind::Delete,
            _ => {
                self.acknowledged_local_revision = self.local_revision;
                return None;
            }
        };
        let op = Operation {
            id: uuid::Uuid::new_v4().to_string(),
            scope: self.scope.clone(),
            client_msg_id: self.client_msg_id.clone(),
            local_revision: self.local_revision,
            kind,
            snapshot: self.desired.clone(),
            base: self.acknowledged.clone(),
            consume_id: None,
        };
        self.workflow = Workflow::InFlight(op.clone());
        Some(op)
    }
    fn operation_matches(&self, scope: &Scope, id: &str) -> bool {
        scope == &self.scope && self.pending().is_some_and(|op| op.id == id)
    }
    pub fn mark_uncertain(&mut self, scope: &Scope, operation_id: &str) -> bool {
        if !self.operation_matches(scope, operation_id) {
            return false;
        }
        if let Workflow::InFlight(op) = &self.workflow {
            self.workflow = Workflow::Uncertain(op.clone());
        }
        true
    }
    /// Only a definite server rejection permits another attempt. Callers must
    /// not classify missing list entries, timeouts, or lost replies as rejection.
    pub fn reject(&mut self, scope: &Scope, operation_id: &str) -> bool {
        if !self.operation_matches(scope, operation_id) {
            return false;
        }
        if matches!(self.workflow, Workflow::Conflict { .. }) {
            return false;
        }
        self.workflow = if self.consumption.is_some() {
            Workflow::Rejected(self.pending().unwrap().clone())
        } else {
            Workflow::Idle
        };
        true
    }
    fn valid_remote(&self, remote: &RemoteDraft) -> bool {
        remote.scope == self.scope
            && !remote.id.is_empty()
            && ts_key(&remote.last_updated_ts).0 > 0
            && !self.retired_ids.contains(&remote.id)
    }
    fn obsolete(&self, remote: &RemoteDraft) -> bool {
        self.acknowledged.as_ref().is_some_and(|base| {
            base.id == remote.id && ts_key(&remote.last_updated_ts) < ts_key(&base.last_updated_ts)
        })
    }
    fn equal_revision_conflict(&self, remote: &RemoteDraft) -> bool {
        self.acknowledged.as_ref().is_some_and(|base| {
            base.id == remote.id
                && ts_key(&remote.last_updated_ts) == ts_key(&base.last_updated_ts)
                && remote.snapshot != base.snapshot
        })
    }
    fn conflict(&mut self, remote: RemoteDraft) {
        self.workflow = Workflow::Conflict {
            remote: Some(remote),
            pending: self.pending().cloned(),
        };
    }
    /// Preserve an authoritative shape the active composer cannot represent.
    /// Adapters must use this rather than acknowledging content through a lossy
    /// projection. Local input and any outstanding operation remain intact.
    pub fn observe_conflict(&mut self, remote: RemoteDraft) -> bool {
        if !self.valid_remote(&remote)
            || self.obsolete(&remote)
            || matches!(self.workflow, Workflow::Conflict { .. })
        {
            return false;
        }
        let belongs = if let Some(base) = &self.acknowledged {
            base.id == remote.id
        } else if let Some(op) = self.pending() {
            Self::belongs_to_operation(op, &remote)
        } else {
            remote.client_msg_id == self.client_msg_id
        };
        if !belongs {
            return false;
        }
        self.conflict(remote);
        true
    }
    fn belongs_to_operation(op: &Operation, remote: &RemoteDraft) -> bool {
        match op.kind {
            OperationKind::Create => remote.client_msg_id == op.client_msg_id,
            OperationKind::Update | OperationKind::Delete => {
                op.base.as_ref().is_some_and(|base| base.id == remote.id)
            }
            OperationKind::Send | OperationKind::Schedule { .. } => op.base.as_ref().map_or_else(
                || remote.client_msg_id == op.client_msg_id,
                |base| base.id == remote.id,
            ),
        }
    }
    fn positive_content(op: &Operation, remote: &RemoteDraft) -> bool {
        matches!(op.kind, OperationKind::Create | OperationKind::Update)
            && Self::belongs_to_operation(op, remote)
            && op.snapshot.as_ref() == Some(&remote.snapshot)
            && op
                .base
                .as_ref()
                .is_none_or(|base| ts_key(&remote.last_updated_ts) > ts_key(&base.last_updated_ts))
    }
    fn accept_operation(&mut self, op: Operation, remote: RemoteDraft) {
        self.acknowledged_local_revision = op.local_revision;
        self.acknowledged = Some(remote);
        // Preserve newer local edits and clearing that arrived during this write.
        self.workflow = Workflow::Idle;
    }
    /// A scoped write receipt plus a matching authoritative snapshot confirms
    /// only that operation's content, never newer local edits.
    pub fn confirm(&mut self, operation_id: &str, remote: RemoteDraft) -> bool {
        if matches!(
            self.workflow,
            Workflow::Conflict { .. } | Workflow::Rejected(_)
        ) || !self.operation_matches(&remote.scope, operation_id)
            || !self.valid_remote(&remote)
            || self.obsolete(&remote)
        {
            return false;
        }
        if self.equal_revision_conflict(&remote) {
            self.conflict(remote);
            return false;
        }
        let op = self.pending().unwrap().clone();
        if !Self::positive_content(&op, &remote) {
            return false;
        }
        if matches!(self.workflow, Workflow::Conflict { .. }) {
            return false;
        }
        self.accept_operation(op, remote);
        true
    }
    /// List/realtime observations are positive evidence only when identity,
    /// content, and revision agree. Absence is deliberately not an input here.
    pub fn observe(&mut self, remote: RemoteDraft) -> bool {
        if !self.valid_remote(&remote) || self.obsolete(&remote) {
            return false;
        }
        if matches!(self.workflow, Workflow::Conflict { .. }) {
            return false;
        }
        if self.consumption.is_some() && self.observe_consumption(&remote) {
            return true;
        }
        if self.equal_revision_conflict(&remote) {
            self.conflict(remote);
            return true;
        }
        if let Some(op) = self.pending().cloned() {
            if !Self::belongs_to_operation(&op, &remote) {
                return false;
            }
            if Self::positive_content(&op, &remote) {
                self.accept_operation(op, remote);
                return true;
            }
            if let Some(base) = &op.base
                && base.id == remote.id
            {
                if ts_key(&remote.last_updated_ts) <= ts_key(&base.last_updated_ts) {
                    return false;
                }
                if remote.snapshot == base.snapshot {
                    // Even unchanged content can be a newer authoritative
                    // revision. Retain it so a delayed older write receipt
                    // cannot replace the server state already observed.
                    self.acknowledged = Some(remote);
                    return true;
                }
            }
            self.workflow = Workflow::Conflict {
                remote: Some(remote),
                pending: Some(op),
            };
            return true;
        }
        let known = self
            .acknowledged
            .as_ref()
            .is_some_and(|base| base.id == remote.id);
        // Once bound, even another object repeating the client UUID cannot
        // replace the remote identity this composer owns.
        if self.acknowledged.is_some() && !known {
            return false;
        }
        if !known && remote.client_msg_id != self.client_msg_id {
            return false;
        }
        let clean = self.desired.as_ref() == self.acknowledged.as_ref().map(|base| &base.snapshot);
        if self.desired.as_ref() == Some(&remote.snapshot) || (known && clean) {
            if self.desired.as_ref() != Some(&remote.snapshot) {
                self.local_revision = self.local_revision.saturating_add(1);
                self.desired = Some(remote.snapshot.clone());
                self.tombstone = false;
            }
            self.acknowledged_local_revision = self.local_revision;
            self.acknowledged = Some(remote);
        } else if self
            .acknowledged
            .as_ref()
            .is_some_and(|base| base.snapshot == remote.snapshot)
        {
            // A repeat of the base is not a conflict with unsaved local edits.
            self.acknowledged = Some(remote);
        } else {
            self.workflow = Workflow::Conflict {
                remote: Some(remote),
                pending: None,
            };
        }
        true
    }
    /// Positive deletion receipt for this exact operation; a missing list row
    /// is not sufficient. Newer local content survives and gets a fresh UUID.
    pub fn confirm_deleted(&mut self, scope: &Scope, operation_id: &str, remote_id: &str) -> bool {
        if !self.operation_matches(scope, operation_id)
            || matches!(self.workflow, Workflow::Conflict { .. })
        {
            return false;
        }
        let op = self.pending().unwrap();
        if op.kind != OperationKind::Delete
            || op.base.as_ref().is_none_or(|base| base.id != remote_id)
        {
            return false;
        }
        self.acknowledged_local_revision = op.local_revision;
        self.retired_ids.push(remote_id.into());
        self.acknowledged = None;
        self.client_msg_id = uuid::Uuid::new_v4().to_string();
        self.workflow = Workflow::Idle;
        true
    }
    /// A definite remote retirement never silently recreates dirty local text.
    /// Event ordering must be checked by the caller before supplying this fact.
    pub fn observe_deleted(&mut self, scope: &Scope, remote_id: &str) -> bool {
        if scope != &self.scope || remote_id.is_empty() {
            return false;
        }
        // A scoped delete event can beat the response/echo that first assigns
        // this composer its remote ID. Keep that positive fact independently.
        if self
            .acknowledged
            .as_ref()
            .is_none_or(|base| base.id != remote_id)
        {
            if self.retired_ids.iter().any(|id| id == remote_id) {
                return false;
            }
            self.retired_ids.push(remote_id.into());
            return true;
        }
        if matches!(self.workflow, Workflow::Conflict { .. }) {
            return false;
        }
        if let Some(op) = self.pending()
            && op.kind == OperationKind::Delete
        {
            let operation_id = op.id.clone();
            return self.confirm_deleted(scope, &operation_id, remote_id);
        }
        if let Some(op) = self.pending().cloned()
            && op.kind == OperationKind::Send
            && !matches!(self.workflow, Workflow::Rejected(_))
        {
            // Draft retirement may be a send echo or another client's delete.
            // Only a positive message receipt can confirm the send.
            self.retired_ids.push(remote_id.into());
            self.workflow = Workflow::Uncertain(op);
            return true;
        }
        let clean = self.consumption.is_none()
            && self.pending().is_none()
            && self.desired.as_ref() == self.acknowledged.as_ref().map(|base| &base.snapshot);
        if clean {
            self.retired_ids.push(remote_id.into());
            self.desired = None;
            self.acknowledged = None;
            self.local_revision = self.local_revision.saturating_add(1);
            self.acknowledged_local_revision = self.local_revision;
            self.tombstone = true;
            self.client_msg_id = uuid::Uuid::new_v4().to_string();
        } else {
            self.workflow = Workflow::Conflict {
                remote: None,
                pending: self.pending().cloned(),
            };
        }
        true
    }
}

#[cfg(test)]
mod tests;
