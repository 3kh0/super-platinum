//! Explicit local archival after acknowledged Slash and an accepted checkpoint.
//! No admission minting, HTTP, automatic clearing, or historical adoption.
use super::*;
type ArchiveStep = a::Step<Arc<p::slash_runtime::ArchiveInput>, p::slash_runtime::Archived>;
#[derive(Clone)]
pub struct SlashArchiveJob {
    queue: Arc<Queue>,
    step: ArchiveStep,
}
#[derive(Clone)]
pub struct SlashArchived {
    inner: p::slash_runtime::Archived,
}
impl SlashArchived {
    pub fn original_intent(&self) -> &str {
        &self.inner.intent
    }
    /// Historical successor only: callers must obtain fresh Editor adoption.
    pub fn historical_review(&self) -> &p::Review {
        &self.inner.review
    }
}
pub struct SlashArchiveAcknowledgment {
    job: SlashArchiveJob,
    id: u64,
    report: crate::drafts::store::terminal::Report,
}
impl SlashCompletion {
    pub fn archive_after_checkpoint(
        &self,
        checkpoint: &StorageJob<CheckpointInput, p::WriteResult>,
    ) -> Result<SlashArchiveJob, AdmissionError> {
        if !Arc::ptr_eq(&self.queue, &checkpoint.queue) {
            return Err(AdmissionError::Stale);
        }
        // Requires the actual acknowledged family and consumed native clear,
        // where applicable. Memory acknowledgment alone never archives storage.
        let (terminal, _) = self.observe_acknowledged()?;
        let (expected, _) = checkpoint.observe_checkpoint()?;
        let accepted = checkpoint.input();
        let original = self.held.input();
        if accepted.writer.id() != original.writer.id()
            || accepted.writer.enrollment() != original.writer.enrollment()
            || accepted.writer.scope() != original.expected.scope()
            || expected.scope() != original.expected.scope()
            || accepted.serial <= original.serial
        {
            return Err(AdmissionError::Stale);
        }
        let input = Arc::new(p::slash_runtime::ArchiveInput {
            stream: original.stream.clone(),
            expected,
            writer: accepted.writer.clone(),
            serial: accepted.serial,
            intent: terminal.id().to_owned(),
        });
        let step = self
            .queue
            .worker
            .hold_slash_archive(&self.held, input)
            .map_err(AdmissionError::from)?;
        Ok(SlashArchiveJob {
            queue: self.queue.clone(),
            step,
        })
    }
}
impl Operations {
    pub fn resume_slash_archive(
        &self,
        key: &OperationKey,
    ) -> Result<SlashArchiveJob, AdmissionError> {
        let step = self
            .queue
            .worker
            .reattach_step(&key.0)
            .map_err(AdmissionError::from)?;
        Ok(SlashArchiveJob {
            queue: self.queue.clone(),
            step,
        })
    }
}
/// Exact observed local storage failures followed by the successful archive.
/// Diagnostic submission IDs are scoped to this worker, not durable identities.
#[derive(Clone)]
pub struct SlashArchiveRepair {
    reports: Vec<(u64, crate::drafts::store::terminal::Report)>,
}
impl SlashArchiveRepair {
    pub fn failed_submissions(&self) -> Vec<u64> {
        self.reports[..self.reports.len() - 1]
            .iter()
            .map(|(id, _)| *id)
            .collect()
    }
    pub fn successful_submission(&self) -> u64 {
        self.reports.last().unwrap().0
    }
}
pub struct SlashArchiveRepairAcknowledgment {
    job: SlashArchiveJob,
    observed: SlashArchiveRepair,
}
impl SlashArchiveRepairAcknowledgment {
    /// Retire only these observed local attempts and this archive slot. Durable
    /// draft/delivery/archive history is unchanged; the parent family is separate.
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self.job.queue.worker.observe_slash_archive_repair(
            &self.job.step,
            Some(&self.observed.reports),
            true,
        ) {
            Ok(_) => Ok(()),
            Err(error) => Err((error.into(), self)),
        }
    }
}
impl SlashArchiveJob {
    /// Explicit same-input local storage retry; never re-executes the command.
    pub fn retry_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_slash_archive(&self.step)
            .map_err(Into::into)
    }
    /// Historical result only, without any retirement or fresh adoption authority.
    pub fn result(&self) -> Option<Result<SlashArchived, p::Error>> {
        match self.step.state() {
            a::StepState::Ready => self
                .step
                .value()
                .ok()
                .map(|inner| Ok(SlashArchived { inner })),
            a::StepState::Failed(error) => Some(Err(error)),
            _ => None,
        }
    }
    /// Copy every failed Storage attempt and the final success before issuing a
    /// distinct repair acknowledgment. Clean observe_success remains unchanged.
    pub fn observe_repaired_success(
        &self,
    ) -> Result<
        (
            SlashArchived,
            SlashArchiveRepair,
            SlashArchiveRepairAcknowledgment,
        ),
        AdmissionError,
    > {
        let inner = self.step.value().map_err(|_| AdmissionError::Incomplete)?;
        let observed = SlashArchiveRepair {
            reports: self
                .queue
                .worker
                .observe_slash_archive_repair(&self.step, None, false)?,
        };
        Ok((
            SlashArchived { inner },
            observed.clone(),
            SlashArchiveRepairAcknowledgment {
                job: self.clone(),
                observed,
            },
        ))
    }

    pub fn key(&self) -> OperationKey {
        OperationKey(self.step.key())
    }
    pub fn state(&self) -> JobState {
        self.step.state().into()
    }
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_step(&self.step)
            .map_err(AdmissionError::from)
    }
    /// Blocking; never call from the renderer thread.
    pub fn wait(&self) -> Result<JobState, AdmissionError> {
        self.queue
            .worker
            .wait_step(&self.step)
            .map(Into::into)
            .map_err(AdmissionError::from)
    }
    pub fn observe_success(
        &self,
    ) -> Result<(SlashArchived, SlashArchiveAcknowledgment), AdmissionError> {
        let inner = self.step.value().map_err(|_| AdmissionError::Incomplete)?;
        let (id, report) = self.queue.worker.observe_simple_success(&self.step)?;
        Ok((
            SlashArchived { inner },
            SlashArchiveAcknowledgment {
                job: self.clone(),
                id,
                report,
            },
        ))
    }
}
impl SlashArchiveAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self
            .job
            .queue
            .worker
            .acknowledge_simple_success(&self.job.step, self.id, self.report)
        {
            Ok(()) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}
