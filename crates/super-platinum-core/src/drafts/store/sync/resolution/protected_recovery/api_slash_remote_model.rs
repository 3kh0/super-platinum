//! Strict retained data is not live dispatch or native choice authority.
use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Fence {
    pub view: views::InventoryRecord,
    pub writer: String,
    pub serial: u64,
    pub receipt: String,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(in super::super) enum Stage {
    ReadReserved,
    ReadDispatched,
    ReadDone,
    CleanupReserved,
    CleanupDispatched,
    CleanupDone,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(in super::super) enum ReadKind {
    Response,
    NotRequested,
    Unknown,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct ReadEvidence {
    pub kind: ReadKind,
    pub user: String,
    #[serde(deserialize_with = "required_option")]
    pub body: Option<String>,
    pub until: u64,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(in super::super) enum Action {
    Preserve,
    PreserveRejected,
    Cleanup,
    CleanupRejected,
}
impl Action {
    pub fn cleanup(self) -> bool {
        matches!(self, Self::Cleanup | Self::CleanupRejected)
    }
    pub fn rejected(self) -> bool {
        matches!(self, Self::PreserveRejected | Self::CleanupRejected)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Decision {
    pub id: String,
    pub action: Action,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Cleanup {
    pub operation: Operation,
    pub before: views::InventoryRecord,
    pub after: views::InventoryRecord,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(in super::super) enum CleanupKind {
    Response,
    NotRequested,
    Unknown,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct CleanupEvidence {
    pub kind: CleanupKind,
    #[serde(deserialize_with = "required_option")]
    pub body: Option<String>,
    pub until: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Job {
    pub version: u32,
    pub revision: u64,
    pub id: String,
    pub scope: Scope,
    pub intent: String,
    pub enrollment: String,
    pub binding: String,
    pub settlement: String,
    pub attestation: String,
    pub user: String,
    pub base: RemoteDraft,
    pub original: Fence,
    #[serde(deserialize_with = "required_team")]
    pub team: Team,
    pub pending: Pending,
    pub floor: u64,
    pub known_deadline: u64,
    pub stage: Stage,
    #[serde(deserialize_with = "required_option")]
    pub read: Option<ReadEvidence>,
    #[serde(deserialize_with = "required_option")]
    pub decision: Option<Decision>,
    #[serde(deserialize_with = "required_option")]
    pub cleanup: Option<Cleanup>,
    #[serde(deserialize_with = "required_option")]
    pub result: Option<CleanupEvidence>,
    #[serde(deserialize_with = "required_option")]
    pub reduced_before: Option<views::InventoryRecord>,
    #[serde(deserialize_with = "required_option")]
    pub reduced_after: Option<views::InventoryRecord>,
}
// All process handles are private, non-Clone/non-Deserialize. Loading a Job never mints one.
pub(in super::super) struct ReadTicket {
    pub(super) raw: String,
    pub(super) stream: Stream,
}
pub(in super::super) struct ReadClaim {
    pub(super) raw: String,
    pub(super) stream: Stream,
}
pub(in super::super) struct Choice {
    pub(super) raw: String,
    pub(super) stream: Stream,
    pub(super) decision: Decision,
}
pub(in super::super) struct CleanupTicket {
    pub(super) raw: String,
    pub(super) stream: Stream,
}
pub(in super::super) struct CleanupClaim {
    pub(super) raw: String,
    pub(super) stream: Stream,
}
/// No production constructor: awaiting sealed native/network adapter review.
pub(in super::super) struct ReadReport {
    pub(super) id: String,
    pub(super) evidence: ReadEvidence,
}
pub(in super::super) struct CleanupReport {
    pub(super) id: String,
    pub(super) operation: String,
    pub(super) evidence: CleanupEvidence,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Archive {
    pub version: u32,
    pub scope: Scope,
    pub intent: String,
    pub enrollment: String,
    pub binding: String,
    pub job: String,
    pub request: String,
    pub current: Fence,
    #[serde(deserialize_with = "required_team")]
    pub team: Team,
    pub after: views::InventoryRecord,
    pub retired: SlashRecord,
}
pub(in super::super) struct Finished {
    pub(super) raw: String,
}
fn required_team<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Team, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    for key in ["lease_id", "pending", "last_interrupted_id", "gap_reason"] {
        if value.get(key).is_none() {
            return Err(serde::de::Error::custom("missing continuity guard field"));
        }
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}
impl Finished {
    pub(in super::super) fn historical(&self) -> Result<Review, AppError> {
        let a: Archive = wire::strict(&self.raw)?;
        restore(&a.after)
    }
}
#[cfg(test)]
pub(in super::super) fn read_report(
    claim: &ReadClaim,
    kind: ReadKind,
    body: Option<String>,
    until: u64,
) -> ReadReport {
    let job: Job = wire::strict(&claim.raw).unwrap();
    ReadReport {
        id: job.id,
        evidence: ReadEvidence {
            kind,
            user: job.user,
            body,
            until,
        },
    }
}
#[cfg(test)]
pub(in super::super) fn cleanup_report(
    claim: &CleanupClaim,
    kind: CleanupKind,
    body: Option<String>,
    until: u64,
) -> CleanupReport {
    let job: Job = wire::strict(&claim.raw).unwrap();
    CleanupReport {
        id: job.id,
        operation: job.cleanup.unwrap().operation.id,
        evidence: CleanupEvidence { kind, body, until },
    }
}
#[cfg(test)]
pub(in super::super) fn choice(raw: String, stream: Stream, action: Action) -> Choice {
    Choice {
        raw,
        stream,
        decision: Decision {
            id: uuid::Uuid::new_v4().to_string(),
            action,
        },
    }
}
