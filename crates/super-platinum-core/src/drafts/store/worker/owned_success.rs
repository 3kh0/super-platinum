//! Acknowledgment is explicit after result observation, never Drop or eviction.
use super::*;
#[path = "owned_writer_settlement.rs"]
mod writer_settlement;
pub use writer_settlement::{WriterSettlement, WriterSettlementAcknowledgment};

root!(
    renew_local,
    resume_renew_local,
    hold_renew_local,
    p::LocalInventory,
    p::LocalWriterLease
);

/// One observed successful single-step result. No public constructor, Clone,
/// serialization, replay, or whole-account drain meaning. Dropping retains work.
pub struct SuccessAcknowledgment<I, T> {
    job: StorageJob<I, T>,
    id: u64,
    report: crate::drafts::store::terminal::Report,
}
impl<I: Send + Sync + 'static, T: Send + 'static> SuccessAcknowledgment<I, T> {
    /// Releases only this exact successful in-memory slot and accounting row.
    /// On refusal return the receipt so the consumer retains its custody.
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
impl<I: Send + Sync + 'static, T: Send + 'static> StorageJob<I, T> {
    fn success_ack(&self) -> Result<SuccessAcknowledgment<I, T>, AdmissionError> {
        let (id, report) = self.queue.worker.observe_simple_success(&self.step)?;
        Ok(SuccessAcknowledgment {
            job: self.clone(),
            id,
            report,
        })
    }
}
impl<I: Send + Sync + 'static, T: Clone + Send + 'static> StorageJob<I, T> {
    /// Return the actual successful result alongside its opaque receipt. Other
    /// families, errors, held replies, and unaccounted replies cannot mint it.
    pub fn observe_success(&self) -> Result<(T, SuccessAcknowledgment<I, T>), AdmissionError> {
        let value = self
            .result()
            .ok_or(AdmissionError::Incomplete)?
            .map_err(|_| AdmissionError::Incomplete)?;
        Ok((value, self.success_ack()?))
    }
}
impl StorageJob<CheckpointInput, p::WriteResult> {
    pub fn observe_checkpoint(
        &self,
    ) -> Result<
        (
            p::Review,
            SuccessAcknowledgment<CheckpointInput, p::WriteResult>,
        ),
        AdmissionError,
    > {
        let Some(Ok(WriteOutcome::Applied(value))) = self.write_result() else {
            return Err(AdmissionError::Incomplete);
        };
        Ok((value, self.success_ack()?))
    }
}
impl StorageJob<LocalWriteInput, p::LocalWriteResult> {
    pub fn observe_local_write(
        &self,
    ) -> Result<
        (
            p::LocalInventory,
            SuccessAcknowledgment<LocalWriteInput, p::LocalWriteResult>,
        ),
        AdmissionError,
    > {
        let Some(Ok(LocalOutcome::Applied(value))) = self.local_result() else {
            return Err(AdmissionError::Incomplete);
        };
        Ok((value, self.success_ack()?))
    }
}
