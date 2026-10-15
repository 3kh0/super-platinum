//! Durable-CAS controller model. The admission constructor is intentionally absent.
use super::{
    api::{Frozen, Reply},
    catalog::Catalog,
    *,
};
use crate::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient, Transport},
};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[path = "managed_terminal.rs"]
mod managed_terminal;
#[cfg(test)]
pub(crate) use managed_terminal::isolated_terminal;
pub(crate) use managed_terminal::{ManagedEvidence, ManagedTerminal};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Prepared,
    DispatchPossible,
    NotSent,
    Completed(Reply),
    Unknown,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    id: String,
    version: u64,
    frozen: Frozen,
    phase: Phase,
    #[serde(deserialize_with = "required_option")]
    retry_after_secs: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    http_status: Option<u16>,
    retired: bool,
}
fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
impl Record {
    pub(super) fn version(&self) -> u64 {
        self.version
    }
    pub(super) fn same_input(&self, other: &Self) -> bool {
        self.frozen == other.frozen
    }
    /// Use before restore; ordinary Value decoding may collapse duplicate keys.
    pub fn decode(bytes: &[u8]) -> Result<Self, Failure> {
        let value = codec::decode(bytes, 1024 * 1024).map_err(|_| Failure::Invalid)?;
        serde_json::from_value(value).map_err(|_| Failure::Invalid)
    }
    // Immutable storage comparisons only; neither returns execution authority.
    pub(crate) fn protected_input_revision(&self) -> u64 {
        self.frozen.revision
    }
    pub(crate) fn protected_descends_from(&self, prior: &Self) -> bool {
        self.id == prior.id
            && self.frozen == prior.frozen
            && self.version >= prior.version
            && (self.version != prior.version || self == prior)
            && !matches!(self.phase, Phase::Prepared)
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn phase(&self) -> &Phase {
        &self.phase
    }
    pub fn original(&self) -> &Value {
        &self.frozen.original
    }
    pub fn owner(&self) -> &Owner {
        &self.frozen.owner
    }
    pub fn retry_after_secs(&self) -> Option<u64> {
        self.retry_after_secs
    }
    pub fn http_status(&self) -> Option<u16> {
        self.http_status
    }
    pub fn retired(&self) -> bool {
        self.retired
    }
}
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SlashRecord(..)")
    }
}

/// Trusted host storage boundary: atomically compare the full prior record,
/// commit the replacement durably, and return only after persistence. Never
/// implement this with an optimistic UI acknowledgement. No real adapter staged.
/// A DispatchPossible transition must atomically enforce the durable owner-wide
/// cooldown and absence of unresolved dispatch for that owner. A 429 outcome
/// records a durable Retry-After deadline before permitting another reservation;
/// absent/invalid Retry-After requires explicit recovery, never a guessed delay.
/// On startup, restore the remaining deadline conservatively into monotonic time.
/// These are REQUIRED adapter semantics, not implemented by this trait alone.
pub trait Journal {
    fn compare_exchange(
        &mut self,
        old: Option<&Record>,
        next: &Record,
    ) -> impl Future<Output = Result<(), Failure>>;
}

/// Sealed until native chooser/Editor/Binding/Review/writer/visit plus protected
/// SlashIntent ownership can be proved. `alive` must revoke on *any* owner,
/// snapshot, session, transport, credentials, catalog or mount replacement.
/// Revocation is permanent: never store true again on an existing alive cell.
pub struct Admission {
    pub(super) intent: String,
    pub(super) frozen: Frozen,
    pub(super) catalog: Catalog,
    pub(super) transport: Arc<Transport>,
    pub(super) session: WorkspaceSession,
    pub(super) client: SlackClient,
    pub(super) alive: Arc<AtomicBool>,
}
pub fn managed_admission_unavailable() -> Result<Admission, Failure> {
    Err(Failure::AdmissionUnavailable)
}

/// One-use Rust value; deliberately neither Clone nor Deserialize.
pub struct Permit {
    id: String,
    version: u64,
    request: PreparedRequest,
    transport: Arc<Transport>,
    alive: Arc<AtomicBool>,
}
impl Permit {
    pub(crate) fn request(&self) -> &PreparedRequest {
        &self.request
    }
    pub(crate) fn current(&self, transport: &Transport) -> bool {
        std::ptr::eq(self.transport.as_ref(), transport) && self.alive.load(Ordering::Acquire)
    }
    pub(crate) fn report(self, delivery: Delivery, retry_after_secs: Option<u64>) -> Report {
        Report {
            id: self.id,
            version: self.version,
            delivery,
            retry_after_secs,
        }
    }
}
pub(crate) enum Delivery {
    NotSent,
    Reply(Value),
    HttpFailure(u16),
    Unknown,
}
pub struct Report {
    id: String,
    version: u64,
    delivery: Delivery,
    retry_after_secs: Option<u64>,
}

pub struct Controller {
    record: Record,
    catalog: Option<Catalog>,
    pending: Option<Record>,
    recovered: bool,
}
impl Controller {
    pub async fn reserve(
        frozen: Frozen,
        catalog: Catalog,
        journal: &mut impl Journal,
    ) -> Result<Self, Failure> {
        frozen.validate()?;
        if catalog.owner() != &frozen.owner || !catalog.commands().contains(&frozen.command) {
            return Err(Failure::Stale);
        }
        let record = Record {
            id: uuid::Uuid::new_v4().to_string(),
            version: 1,
            frozen,
            phase: Phase::Prepared,
            retry_after_secs: None,
            http_status: None,
            retired: false,
        };
        journal.compare_exchange(None, &record).await?;
        Ok(Self {
            record,
            catalog: Some(catalog),
            pending: None,
            recovered: false,
        })
    }
    pub fn restore(record: Record) -> Result<Self, Failure> {
        record.frozen.validate()?;
        if record.version == 0 || !uuid::Uuid::parse_str(&record.id).is_ok_and(|v| !v.is_nil()) {
            return Err(Failure::Invalid);
        }
        if record
            .http_status
            .is_some_and(|status| !(100..600).contains(&status))
            || matches!(record.phase, Phase::Prepared | Phase::NotSent)
                && (record.http_status.is_some() || record.retry_after_secs.is_some())
            || matches!(record.phase, Phase::DispatchPossible)
                && (record.http_status.is_some() || record.retry_after_secs.is_some())
            || matches!(record.phase, Phase::Completed(_))
                && record
                    .http_status
                    .is_some_and(|status| !(200..300).contains(&status))
        {
            return Err(Failure::Invalid);
        }
        if let Phase::Completed(reply) = &record.phase {
            let raw = match reply {
                Reply::Acknowledged { raw, .. }
                | Reply::Rejected { raw }
                | Reply::Unknown { raw: Some(raw) } => Some(raw),
                Reply::Unknown { raw: None } => None,
            };
            if raw.is_some_and(|raw| &api::decode(raw.clone()) != reply) {
                return Err(Failure::Invalid);
            }
        }
        if record.retired
            && matches!(
                record.phase,
                Phase::DispatchPossible | Phase::Unknown | Phase::Completed(Reply::Unknown { .. })
            )
        {
            return Err(Failure::Invalid);
        }
        Ok(Self {
            record,
            catalog: None,
            pending: None,
            recovered: true,
        })
    }
    pub fn record(&self) -> &Record {
        &self.record
    }
    /// Includes Prepared on restart: no recovered intent is automatically sent.
    pub fn recovery_required(&self) -> bool {
        self.recovered
            || matches!(
                self.record.phase,
                Phase::DispatchPossible | Phase::Unknown | Phase::Completed(Reply::Unknown { .. })
            )
    }
    pub async fn arm(
        &mut self,
        admission: Admission,
        token: &str,
        journal: &mut impl Journal,
    ) -> Result<Permit, Failure> {
        if self.recovered
            || self.record.retired
            || self.pending.is_some()
            || self.record.phase != Phase::Prepared
        {
            return Err(Failure::NoReplay);
        }
        if admission.intent != self.record.id
            || !admission.alive.load(Ordering::Acquire)
            || admission.frozen != self.record.frozen
            || self
                .catalog
                .as_ref()
                .is_none_or(|c| !c.same_generation(&admission.catalog))
        {
            return Err(Failure::Stale);
        }
        let request = api::prepare(
            &admission.client,
            &admission.session,
            &self.record.frozen,
            token,
        )?;
        let mut next = self.record.clone();
        next.version = next.version.checked_add(1).ok_or(Failure::Invalid)?;
        next.phase = Phase::DispatchPossible;
        journal.compare_exchange(Some(&self.record), &next).await?;
        self.record = next;
        // An invalidation during durable admission never returns a permit.
        if !admission.alive.load(Ordering::Acquire) {
            return Err(Failure::Stale);
        }
        Ok(Permit {
            id: self.record.id.clone(),
            version: self.record.version,
            request,
            transport: admission.transport.clone(),
            alive: admission.alive.clone(),
        })
    }
    /// Stage outcome before persistence; a failed journal write keeps it for
    /// flush-only retry. That retry never dispatches or exposes a new permit.
    pub fn finish(&mut self, report: Report) -> Result<(), Failure> {
        if self.pending.is_some()
            || report.id != self.record.id
            || report.version != self.record.version
            || self.record.phase != Phase::DispatchPossible
        {
            return Err(Failure::Stale);
        }
        let mut next = self.record.clone();
        next.version = next.version.checked_add(1).ok_or(Failure::Invalid)?;
        next.retry_after_secs = report.retry_after_secs;
        next.phase = match report.delivery {
            Delivery::NotSent => Phase::NotSent,
            Delivery::Reply(raw) => Phase::Completed(api::decode(raw)),
            Delivery::HttpFailure(status) => {
                next.http_status = Some(status);
                Phase::Unknown
            }
            Delivery::Unknown => Phase::Unknown,
        };
        self.pending = Some(next);
        Ok(())
    }
    pub async fn flush(&mut self, journal: &mut impl Journal) -> Result<(), Failure> {
        let Some(next) = self.pending.as_ref() else {
            return Ok(());
        };
        journal.compare_exchange(Some(&self.record), next).await?;
        self.record = self.pending.take().expect("retained during await");
        Ok(())
    }
    /// Retire only the command receipt; never clears/releases a protected draft.
    /// Unknown dispatch cannot be hidden while its outcome is unresolved.
    pub async fn retire(&mut self, journal: &mut impl Journal) -> Result<(), Failure> {
        if self.pending.is_some()
            || matches!(
                self.record.phase,
                Phase::DispatchPossible | Phase::Unknown | Phase::Completed(Reply::Unknown { .. })
            )
        {
            return Err(Failure::NoReplay);
        }
        let mut next = self.record.clone();
        next.version = next.version.checked_add(1).ok_or(Failure::Invalid)?;
        next.retired = true;
        journal.compare_exchange(Some(&self.record), &next).await?;
        self.record = next;
        Ok(())
    }
}

#[path = "claimed.rs"]
mod claimed;
#[cfg(test)]
pub(crate) use claimed::fixture_plan;
pub(crate) use claimed::{ArmRefused, ClaimPlan};
