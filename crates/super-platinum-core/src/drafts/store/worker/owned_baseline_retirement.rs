//! Closed observed cooldown and storage-repair receipts, never generic eviction.
use super::*;
type Reports = Vec<(u64, crate::drafts::store::terminal::Report)>;

#[derive(Clone)]
pub struct BaselineCooldown {
    key: OperationKey,
    submission: u64,
    until: u64,
}
impl BaselineCooldown {
    pub fn key(&self) -> &OperationKey {
        &self.key
    }
    pub fn submission(&self) -> u64 {
        self.submission
    }
    pub fn until_ms(&self) -> u64 {
        self.until
    }
}
pub struct BaselineCooldownAcknowledgment {
    reserved: BaselineReserved,
    observed: BaselineCooldown,
}
impl BaselineReserved {
    /// Observe the actual failed Reserve deadline. Its two unmaterialized family
    /// placeholders are retained until the separate acknowledgment is consumed.
    pub fn observe_cooldown(
        &self,
    ) -> Result<(BaselineCooldown, BaselineCooldownAcknowledgment), AdmissionError> {
        let (submission, until) = self
            .queue
            .worker
            .observe_baseline_cooldown(&self.step, None, false)?;
        let observed = BaselineCooldown {
            key: self.key(),
            submission,
            until,
        };
        Ok((
            observed.clone(),
            BaselineCooldownAcknowledgment {
                reserved: self.clone(),
                observed,
            },
        ))
    }
}
impl BaselineCooldownAcknowledgment {
    /// No durable state changes; release only the accounted failed family.
    /// A later explicit action needs a new actual input/time/Source/Review.
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.reserved.queue.worker.observe_baseline_cooldown(
            &self.reserved.step,
            Some((self.observed.submission, self.observed.until)),
            true,
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
#[derive(Clone)]
pub struct BaselineRepairReceipt {
    reports: Reports,
}
impl BaselineRepairReceipt {
    pub fn failed_submissions(&self) -> Vec<u64> {
        self.reports
            .iter()
            .filter_map(|(id, r)| {
                (r.execution == crate::drafts::store::terminal::Execution::StorageUnresolved)
                    .then_some(*id)
            })
            .collect()
    }
    pub fn successful_submission(&self) -> u64 {
        self.reports.last().unwrap().0
    }
}
pub struct BaselineRepairAcknowledgment {
    completion: BaselineCompletion,
    outcome: BaselineOutcome,
    observed: BaselineRepairReceipt,
}
impl BaselineCompletion {
    /// Only actual Storage failures followed by the exact retained successful
    /// result can mint this distinct receipt. Raw read and evidence are unchanged.
    /// The clean observe_success contract remains strict.
    pub fn observe_repaired_success(
        &self,
    ) -> Result<
        (
            BaselineOutcome,
            BaselineRepairReceipt,
            BaselineRepairAcknowledgment,
        ),
        AdmissionError,
    > {
        let (value, reports) = self
            .queue
            .worker
            .observe_baseline_retirement(&self.held, None, None, true, false)?;
        let outcome = BaselineOutcome { value };
        let observed = BaselineRepairReceipt { reports };
        Ok((
            outcome.clone(),
            observed.clone(),
            BaselineRepairAcknowledgment {
                completion: self.clone(),
                outcome,
                observed,
            },
        ))
    }
}
impl BaselineRepairAcknowledgment {
    pub fn matches(&self, outcome: &BaselineOutcome) -> bool {
        Arc::ptr_eq(&self.outcome.value, &outcome.value)
    }
    /// Retires observed in-memory attempts and family slots only. Actual baseline
    /// evidence remains usable by explicit finish_baseline after memory retirement.
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.completion.queue.worker.observe_baseline_retirement(
            &self.completion.held,
            Some(&self.outcome.value),
            Some(&self.observed.reports),
            true,
            true,
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
pub struct BaselineFinishRepairAcknowledgment {
    job: StorageJob<BaselineFinishInput, p::Review>,
    value: p::Review,
    observed: BaselineRepairReceipt,
}
impl StorageJob<BaselineFinishInput, p::Review> {
    /// Historical settlement plus every observed local Storage failure. This
    /// does not authorize a new choice or substitute for native projection checks.
    pub fn observe_repaired_success(
        &self,
    ) -> Result<
        (
            p::Review,
            BaselineRepairReceipt,
            BaselineFinishRepairAcknowledgment,
        ),
        AdmissionError,
    > {
        let (value, reports) = self
            .queue
            .worker
            .observe_baseline_finish_repair(&self.step, None, false)?;
        let observed = BaselineRepairReceipt { reports };
        Ok((
            value.clone(),
            observed.clone(),
            BaselineFinishRepairAcknowledgment {
                job: self.clone(),
                value,
                observed,
            },
        ))
    }
}
impl BaselineFinishRepairAcknowledgment {
    pub fn matches(&self, value: &p::Review) -> bool {
        self.value.same(value)
    }
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.job.queue.worker.observe_baseline_finish_repair(
            &self.job.step,
            Some((&self.value, &self.observed.reports)),
            true,
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
#[cfg(test)]
#[path = "owned_baseline_retirement_tests.rs"]
mod tests;
