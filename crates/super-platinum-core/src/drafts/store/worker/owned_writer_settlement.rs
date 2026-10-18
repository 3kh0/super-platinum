//! Applied checkpoint/local-write history, distinct from clean success.
use super::*;
type Reports = Vec<(u64, crate::drafts::store::terminal::Report)>;

/// Historical local storage proof only. It does not settle a pending Send.
#[derive(Clone)]
pub struct WriterSettlement {
    reports: Reports,
}
impl WriterSettlement {
    pub fn failed_submissions(&self) -> Vec<u64> {
        self.reports[..self.reports.len() - 1]
            .iter()
            .map(|(id, _)| *id)
            .collect()
    }
    pub fn successful_submission(&self) -> u64 {
        self.reports.last().unwrap().0
    }
    pub fn operation_pending(&self) -> bool {
        self.reports.last().unwrap().1.inventory
            == crate::drafts::store::terminal::Inventory::OperationPending
    }
}
/// Nonclone, observed exact-job receipt. Dropping it retains all slots/history.
pub struct WriterSettlementAcknowledgment<I, T> {
    job: StorageJob<I, T>,
    family: &'static str,
    observed: WriterSettlement,
}
impl<I: Send + Sync + 'static, T: Send + 'static> WriterSettlementAcknowledgment<I, T> {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        let report = self.observed.reports.last().unwrap().1;
        match self.job.queue.worker.observe_writer_settlement(
            &self.job.step,
            self.family,
            report,
            Some(&self.observed.reports),
            true,
        ) {
            Ok(_) => Ok(()),
            Err(error) => Err((error.into(), self)),
        }
    }
}
impl<I: Send + Sync + 'static, T: Send + 'static> StorageJob<I, T> {
    fn writer_settlement(
        &self,
        family: &'static str,
        report: crate::drafts::store::terminal::Report,
    ) -> Result<(WriterSettlement, WriterSettlementAcknowledgment<I, T>), AdmissionError> {
        let observed = WriterSettlement {
            reports: self
                .queue
                .worker
                .observe_writer_settlement(&self.step, family, report, None, false)?,
        };
        Ok((
            observed.clone(),
            WriterSettlementAcknowledgment {
                job: self.clone(),
                family,
                observed,
            },
        ))
    }
}
impl StorageJob<CheckpointInput, p::WriteResult> {
    /// A repaired checkpoint, or an applied checkpoint retaining a pending Send.
    /// Clean observe_checkpoint remains strict; this issues no remote authority.
    pub fn observe_checkpoint_settlement(
        &self,
    ) -> Result<
        (
            p::Review,
            WriterSettlement,
            WriterSettlementAcknowledgment<CheckpointInput, p::WriteResult>,
        ),
        AdmissionError,
    > {
        let Some(Ok(WriteOutcome::Applied(value))) = self.write_result() else {
            return Err(AdmissionError::Incomplete);
        };
        let (observed, acknowledgment) = self.writer_settlement(
            "protected-checkpoint",
            crate::drafts::store::terminal::review(&value),
        )?;
        Ok((value, observed, acknowledgment))
    }
}
impl StorageJob<LocalWriteInput, p::LocalWriteResult> {
    /// Exact immutable local-write retry history followed by an applied result.
    pub fn observe_local_write_settlement(
        &self,
    ) -> Result<
        (
            p::LocalInventory,
            WriterSettlement,
            WriterSettlementAcknowledgment<LocalWriteInput, p::LocalWriteResult>,
        ),
        AdmissionError,
    > {
        let Some(Ok(LocalOutcome::Applied(value))) = self.local_result() else {
            return Err(AdmissionError::Incomplete);
        };
        let (observed, acknowledgment) = self.writer_settlement(
            "protected-write-local",
            crate::drafts::store::terminal::accepted(&value),
        )?;
        Ok((value, observed, acknowledgment))
    }
}
