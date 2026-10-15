//! DISCONNECTED CANDIDATE: private child of store::sync::resolution.
//! No production activation, socket wiring, or network calls.
use super::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[path = "continuity_inventory.rs"]
mod inventory;
#[path = "continuity_read.rs"]
mod readback;
#[path = "continuity_storage.rs"]
mod storage;
#[cfg(test)]
#[path = "continuity_tests.rs"]
mod tests;
#[path = "continuity_codec.rs"]
mod wire;

mod adapter_atomic;
mod adapter_schema;
pub mod api;
mod archive;
mod facade;
mod migration;
mod recovery;
const PROTECTED_FORMAT: i64 = 7;
const READ_FLOOR_MS: u64 = 30_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ownership {
    enrollment_id: String,
    verified_epoch: u64,
    // Explicit enum prevents absent metadata from granting remote authority.
    basis: Basis,
    origin: Origin,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Basis {
    Unverified,
    Remote,
    Local,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Origin {
    Imported,
    LocalIssued { client_msg_id: String },
}

/// Never convert this into a bare Stored as a protected admission proof.
#[derive(Clone, Debug)]
struct ProtectedRow {
    stored: Stored,
    ownership: Ownership,
}
impl ProtectedRow {
    fn matches(&self, other: &Self) -> bool {
        self.stored.revision == other.stored.revision
            && self.stored.state == other.stored.state
            && self.ownership == other.ownership
    }
}

#[derive(Clone, Debug)]
struct ProtectedReview {
    review: LocalReview,
    row: ProtectedRow,
}
impl ProtectedReview {
    fn matches(&self, other: &Self) -> bool {
        self.row.matches(&other.row) && self.review.matches(&other.review)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Mode {
    Open,
    Gap,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum PendingKind {
    Observation,
    Reconcile,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    id: String,
    kind: PendingKind,
    enrollment_id: String,
    sequence: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Team {
    epoch: u64,
    lease_id: Option<String>,
    mode: Mode,
    sequence: u64,
    pending: Option<Pending>,
    last_interrupted_id: Option<String>,
    gap_reason: Option<String>,
    read_retry_at_ms: u64,
}
impl Team {
    fn gap() -> Self {
        Self {
            epoch: 1,
            lease_id: None,
            mode: Mode::Gap,
            sequence: 0,
            pending: None,
            last_interrupted_id: None,
            gap_reason: Some("activation".into()),
            read_retry_at_ms: 0,
        }
    }
}

// Deliberately not Serialize/Deserialize. A loaded UUID is not a live permit.
#[derive(Clone)]
struct LiveLease {
    account: String,
    team: String,
    id: String,
    epoch: u64,
    live: Arc<Mutex<bool>>,
    revoked: Arc<std::sync::atomic::AtomicBool>,
}
impl LiveLease {
    fn live_guard(&self) -> Result<std::sync::MutexGuard<'_, bool>, AppError> {
        let guard = self
            .live
            .lock()
            .map_err(|_| invalid("poisoned continuity lease"))?;
        if !*guard || !self.is_live() {
            return Err(invalid("continuity lease was revoked"));
        }
        Ok(guard)
    }
    fn matches(&self, team: &Team) -> bool {
        team.mode == Mode::Open
            && team.epoch == self.epoch
            && team.lease_id.as_ref() == Some(&self.id)
    }
    fn is_live(&self) -> bool {
        !self.live.is_poisoned() && !self.revoked.load(std::sync::atomic::Ordering::Acquire)
    }
    fn revoke(&self) -> Result<(), AppError> {
        self.revoked
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

struct Admission {
    lease: LiveLease,
    pending: Pending,
    expected: ProtectedReview,
    observation: Option<crate::drafts::sync::DraftObservation>,
}
struct ReadTicket {
    admission: Admission,
    floor_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ReadState {
    Unresolved,
    Finished,
    SupersededUnknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum KnownReadFailure {
    Missing,
    Unsupported,
    ApiError,
    RateLimited,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ReadCompletion {
    Positive,
    SupersededUnknown,
    Nonpositive(KnownReadFailure),
    RetainedSlashRemote {
        version: u32,
    },
    RetainedPositive {
        version: u32,
    },
    Executed {
        version: u32,
        disposition: api::KnownReadFailure,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRecord {
    team: String,
    lease_id: String,
    epoch: u64,
    enrollment_id: String,
    sequence: u64,
    floor_ms: u64,
    state: ReadState,
    retry_at_ms: u64,
    completion: Option<ReadCompletion>,
}

fn increment(value: u64) -> Result<u64, AppError> {
    value
        .checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or_else(|| invalid("continuity counter exhausted"))
}
fn sql_number(value: u64) -> Result<i64, AppError> {
    i64::try_from(value).map_err(|_| invalid("continuity counter out of range"))
}
fn valid_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil())
}
fn next_pending(
    team: &mut Team,
    kind: PendingKind,
    row: &ProtectedRow,
) -> Result<Pending, AppError> {
    if team.pending.is_some() {
        return Err(invalid("unresolved continuity admission"));
    }
    team.sequence = increment(team.sequence)?;
    let pending = Pending {
        id: uuid::Uuid::new_v4().to_string(),
        kind,
        enrollment_id: row.ownership.enrollment_id.clone(),
        sequence: team.sequence,
    };
    team.pending = Some(pending.clone());
    Ok(pending)
}
fn check_finish(team: &Team, admission: &Admission) -> Result<(), AppError> {
    if !admission.lease.matches(team) || team.pending.as_ref() != Some(&admission.pending) {
        return Err(invalid("stale continuity admission"));
    }
    Ok(())
}

// Store-owned derivation: a caller supplies semantic state, never metadata.
fn successor(previous: &ProtectedRow, state: DraftSync) -> Result<ProtectedRow, AppError> {
    let mut ownership = previous.ownership.clone();
    let identity = |s: &DraftSync| {
        s.acknowledged()
            .map(|r| (r.id.clone(), r.client_msg_id.clone()))
    };
    if previous.stored.state.client_msg_id() != state.client_msg_id()
        || identity(&previous.stored.state) != identity(&state)
    {
        ownership.verified_epoch = 0;
        ownership.basis = Basis::Unverified;
        if previous.stored.state.client_msg_id() != state.client_msg_id() {
            ownership.origin = Origin::LocalIssued {
                client_msg_id: state.client_msg_id().into(),
            };
        }
    }
    Ok(ProtectedRow {
        stored: Stored {
            revision: next_revision(Some(&previous.stored))?,
            state,
        },
        ownership,
    })
}
