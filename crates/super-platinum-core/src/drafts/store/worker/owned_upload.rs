//! Typed facade over the existing upload holder, worker, and exact child edge.
//! No network, ordinary queue fallback, arbitrary command, or replay API.
use super::*;
use crate::{
    drafts::store::uploads::{Draft, Reservation, Stored},
    error::AppError,
};

pub struct UploadJob<I, T> {
    queue: Arc<Queue>,
    held: a::Operation<I, Result<T, AppError>>,
}
impl<I, T> Clone for UploadJob<I, T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            held: self.held.clone(),
        }
    }
}
impl<I: Send + Sync + 'static, T: Send + Sync + 'static> UploadJob<I, T> {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.held.key())
    }
    pub fn input(&self) -> Arc<I> {
        self.held.input()
    }
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_held(&self.held)
            .map_err(Into::into)
    }
    /// Blocking/off-renderer; waits for the matching real accounting terminal.
    pub fn wait(&self) -> Result<Arc<Result<T, AppError>>, AdmissionError> {
        self.queue
            .worker
            .wait_retained(&self.held)
            .map_err(Into::into)
    }
}
pub struct UploadFinalization {
    queue: Arc<Queue>,
    parent: a::Upload,
    child: a::UploadCompletion,
}
impl Clone for UploadFinalization {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            parent: self.parent.clone(),
            child: self.child.clone(),
        }
    }
}
impl UploadFinalization {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.child.key())
    }
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_held(&self.child)
            .map_err(Into::into)
    }
    pub fn wait(&self) -> Result<Arc<Result<Option<Stored>, AppError>>, AdmissionError> {
        self.queue
            .worker
            .wait_retained(&self.child)
            .map_err(Into::into)
    }
    pub fn observe_finalized(&self) -> Result<(Stored, UploadAcknowledgment), AdmissionError> {
        let value = self
            .queue
            .worker
            .observe_upload_finalized(&self.parent, &self.child, false)?;
        Ok((
            value,
            UploadAcknowledgment(Receipt::Finalized(self.clone())),
        ))
    }
}
impl Operations {
    pub fn reserve_upload(
        &self,
        input: Draft,
    ) -> Result<UploadJob<Draft, Reservation>, AdmissionRefused<Draft>> {
        self.queue
            .worker
            .hold_upload(input)
            .map(|held| UploadJob {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
    pub fn upload_inventory(
        &self,
    ) -> Result<UploadJob<String, Vec<Stored>>, AdmissionRefused<String>> {
        self.queue
            .worker
            .hold_upload_inventory(self.account().into())
            .map(|held| UploadJob {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
    pub fn retire_upload(
        &self,
        input: Stored,
    ) -> Result<UploadJob<Stored, bool>, AdmissionRefused<Stored>> {
        self.queue
            .worker
            .hold_upload_retirement(input)
            .map(|held| UploadJob {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
    pub fn resume_upload_reservation(
        &self,
        key: &OperationKey,
    ) -> Result<UploadJob<Draft, Reservation>, AdmissionError> {
        self.queue
            .worker
            .reattach(&key.0)
            .map(|held| UploadJob {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
}
impl UploadJob<Draft, Reservation> {
    /// Files are bound to the actual created reservation, never a caller ticket.
    pub fn finalize(
        &self,
        files: Vec<String>,
    ) -> Result<UploadFinalization, AdmissionRefused<Vec<String>>> {
        self.queue
            .worker
            .hold_upload_completion(&self.held, files)
            .map(|child| UploadFinalization {
                queue: self.queue.clone(),
                parent: self.held.clone(),
                child,
            })
            .map_err(Into::into)
    }
    pub fn observe_existing(&self) -> Result<(Reservation, UploadAcknowledgment), AdmissionError> {
        let value = self
            .queue
            .worker
            .observe_upload_existing(&self.held, false)?;
        Ok((value, UploadAcknowledgment(Receipt::Existing(self.clone()))))
    }
}
impl UploadJob<String, Vec<Stored>> {
    pub fn observe_loaded(&self) -> Result<(Vec<Stored>, UploadAcknowledgment), AdmissionError> {
        let value = self.queue.worker.observe_upload_loaded(&self.held, false)?;
        Ok((value, UploadAcknowledgment(Receipt::Loaded(self.clone()))))
    }
}
impl UploadJob<Stored, bool> {
    pub fn observe_retired(&self) -> Result<UploadAcknowledgment, AdmissionError> {
        self.queue
            .worker
            .observe_upload_retired(&self.held, false)?;
        Ok(UploadAcknowledgment(Receipt::Retired(self.clone())))
    }
}
enum Receipt {
    Canceled(UploadCancellation),
    Existing(UploadJob<Draft, Reservation>),
    Loaded(UploadJob<String, Vec<Stored>>),
    Retired(UploadJob<Stored, bool>),
    Finalized(UploadFinalization),
}
/// Successful observation only. Drop retains work; no Clone/constructor/replay.
pub struct UploadAcknowledgment(Receipt);
impl UploadAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        let result = match &self.0 {
            Receipt::Canceled(job) => {
                job.queue
                    .worker
                    .observe_upload_canceled(&job.parent, &job.child, true)
            }
            Receipt::Existing(job) => job
                .queue
                .worker
                .observe_upload_existing(&job.held, true)
                .map(|_| ()),
            Receipt::Loaded(job) => job
                .queue
                .worker
                .observe_upload_loaded(&job.held, true)
                .map(|_| ()),
            Receipt::Retired(job) => job.queue.worker.observe_upload_retired(&job.held, true),
            Receipt::Finalized(job) => job
                .queue
                .worker
                .observe_upload_finalized(&job.parent, &job.child, true)
                .map(|_| ()),
        };
        result.map_err(|e| (e.into(), self))
    }
}

#[cfg(test)]
#[path = "owned_upload_tests.rs"]
mod tests;

impl Operations {
    pub fn resume_upload_inventory(
        &self,
        key: &OperationKey,
    ) -> Result<UploadJob<String, Vec<Stored>>, AdmissionError> {
        self.queue
            .worker
            .reattach(&key.0)
            .map(|held| UploadJob {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
    pub fn resume_upload_retirement(
        &self,
        key: &OperationKey,
    ) -> Result<UploadJob<Stored, bool>, AdmissionError> {
        self.queue
            .worker
            .reattach(&key.0)
            .map(|held| UploadJob {
                queue: self.queue.clone(),
                held,
            })
            .map_err(Into::into)
    }
    pub fn resume_upload_finalization(
        &self,
        parent: &UploadJob<Draft, Reservation>,
        key: &OperationKey,
    ) -> Result<UploadFinalization, AdmissionError> {
        if !Arc::ptr_eq(&self.queue, &parent.queue) {
            return Err(AdmissionError::Stale);
        }
        self.queue
            .worker
            .reattach_upload_completion(&parent.held, &key.0)
            .map(|child| UploadFinalization {
                queue: self.queue.clone(),
                parent: parent.held.clone(),
                child,
            })
            .map_err(Into::into)
    }
}

#[path = "owned_upload_cancel.rs"]
mod cancel;
pub use cancel::UploadCancellation;
