use super::{Fault, Result, model::*};
use serde::{Deserialize, Serialize};

/// Permanent, content-free terminal metadata. Never a dispatch capability.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetiredAttempt {
    pub(crate) id: String,
    pub(crate) owner: Owner,
    pub(crate) draft_id: String,
    pub(crate) draft_revision: u64,
    pub(crate) result_revision: u64,
    pub(crate) delivery: Delivery,
    pub(crate) decision: Option<ArchiveReceipt>,
}
impl RetiredAttempt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }
    pub fn result_revision(&self) -> u64 {
        self.result_revision
    }
    pub fn draft_id(&self) -> &str {
        &self.draft_id
    }
    pub(crate) fn from_attempt(a: &Attempt, decision: Option<ArchiveReceipt>) -> Self {
        Self {
            id: a.id.clone(),
            owner: a.owner.clone(),
            draft_id: a.draft_id.clone(),
            draft_revision: a.draft_revision,
            result_revision: a.result_revision,
            delivery: a.delivery.clone(),
            decision,
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let a = Attempt {
            id: self.id.clone(),
            owner: self.owner.clone(),
            draft_id: self.draft_id.clone(),
            draft_revision: self.draft_revision,
            result_revision: self.result_revision,
            delivery: self.delivery.clone(),
            frozen: Content::new("metadata".into(), String::new())?,
        };
        a.validate()?;
        if !matches!(
            self.delivery,
            Delivery::CanceledBeforeAdmission
                | Delivery::RateLimited { .. }
                | Delivery::Created { .. }
        ) {
            return Err(Fault::Invalid);
        }
        if let Some(d) = &self.decision {
            d.validate()?;
            if d.owner != self.owner
                || d.old_draft != self.draft_id
                || d.old_attempt != self.id
                || d.expected_draft_revision != self.draft_revision
                || d.expected_result_revision > self.result_revision
            {
                return Err(Fault::Invalid);
            }
            match d.reason {
                ArchiveReason::CreatedReviewed
                    if d.expected_delivery != self.delivery
                        || d.expected_result_revision != self.result_revision =>
                {
                    return Err(Fault::Invalid);
                }
                ArchiveReason::AbandonedOutcomeUnknown
                    if matches!(self.delivery, Delivery::CanceledBeforeAdmission) =>
                {
                    return Err(Fault::Invalid);
                }
                _ => {}
            }
        }
        if matches!(self.delivery, Delivery::Created { .. }) && self.decision.is_none() {
            return Err(Fault::Invalid);
        }
        Ok(())
    }
    /// Compares result metadata only. It neither settles nor validates content for send.
    pub fn same_result(&self, original: &Attempt, outcome: Outcome) -> Result<bool> {
        self.validate()?;
        original.validate()?;
        if !matches!(original.delivery, Delivery::Started | Delivery::Uncertain)
            || original.id != self.id
            || original.owner != self.owner
            || original.draft_id != self.draft_id
            || original.draft_revision != self.draft_revision
            || original.result_revision >= self.result_revision
        {
            return Err(Fault::Stale);
        }
        Ok(outcome.delivery()? == self.delivery)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LateResultEvidence {
    pub(crate) id: String,
    pub(crate) owner: Owner,
    pub(crate) writer: String,
    pub(crate) original: Attempt,
    pub(crate) retired: RetiredAttempt,
    pub(crate) incoming: Delivery,
}
impl LateResultEvidence {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn original(&self) -> &Attempt {
        &self.original
    }
    pub fn retired(&self) -> &RetiredAttempt {
        &self.retired
    }
    pub fn incoming(&self) -> &Delivery {
        &self.incoming
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        uuid(&self.writer)?;
        self.owner.validate()?;
        self.original.validate()?;
        self.retired.validate()?;
        self.incoming.validate()?;
        if self.owner != self.original.owner
            || self.owner != self.retired.owner
            || self.original.id != self.retired.id
            || self.original.draft_id != self.retired.draft_id
            || self.original.draft_revision != self.retired.draft_revision
            || self.original.result_revision >= self.retired.result_revision
            || !matches!(
                self.original.delivery,
                Delivery::Started | Delivery::Uncertain
            )
            || !matches!(
                self.incoming,
                Delivery::Uncertain | Delivery::RateLimited { .. } | Delivery::Created { .. }
            )
            || self.incoming == self.retired.delivery
        {
            return Err(Fault::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvidenceDisposition {
    pub(crate) id: String,
    pub(crate) owner: Owner,
    pub(crate) writer: String,
    pub(crate) original_attempt: String,
    pub(crate) draft_id: String,
    pub(crate) draft_revision: u64,
}
impl EvidenceDisposition {
    pub(crate) fn from_evidence(e: &LateResultEvidence) -> Self {
        Self {
            id: e.id.clone(),
            owner: e.owner.clone(),
            writer: e.writer.clone(),
            original_attempt: e.original.id.clone(),
            draft_id: e.original.draft_id.clone(),
            draft_revision: e.original.draft_revision,
        }
    }
    pub(crate) fn validate(&self) -> Result<()> {
        uuid(&self.id)?;
        self.owner.validate()?;
        uuid(&self.writer)?;
        uuid(&self.original_attempt)?;
        uuid(&self.draft_id)?;
        revision(self.draft_revision)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RetirementSelection {
    pub(crate) attempt: Attempt,
    pub(crate) acknowledged_created: Option<String>,
}
impl RetirementSelection {
    /// Explicit choice; no selection is generated when a review is opened.
    pub fn new(attempt: &Attempt, acknowledge_created: Option<&str>) -> Result<Self> {
        attempt.validate()?;
        match (&attempt.delivery, acknowledge_created) {
            (Delivery::Created { file_id }, Some(given)) if file_id == given => {}
            (Delivery::CanceledBeforeAdmission | Delivery::RateLimited { .. }, None) => {}
            _ => return Err(Fault::Blocked),
        }
        Ok(Self {
            attempt: attempt.clone(),
            acknowledged_created: acknowledge_created.map(str::to_owned),
        })
    }
}
pub struct RetirementReceipt {
    pub(crate) newly_committed: bool,
    pub(crate) retired: Vec<RetiredAttempt>,
}
impl RetirementReceipt {
    pub fn newly_committed(&self) -> bool {
        self.newly_committed
    }
    pub fn retired(&self) -> &[RetiredAttempt] {
        &self.retired
    }
}
