//! Opaque exact reservation retirement, never proof that remote work was canceled.
use super::*;

#[derive(Clone)]
pub struct UploadCancellation {
    pub(super) queue: Arc<Queue>,
    pub(super) parent: a::Upload,
    pub(super) child: a::Operation<Stored, Result<bool, AppError>>,
}
impl UploadJob<Draft, Reservation> {
    /// Retire the exact unchanged row from this actual created reservation.
    /// The caller must separately retain any network outcome; this grants no retry.
    pub fn cancel_reservation(&self) -> Result<UploadCancellation, AdmissionError> {
        self.queue
            .worker
            .hold_upload_cancellation(&self.held)
            .map(|child| UploadCancellation {
                queue: self.queue.clone(),
                parent: self.held.clone(),
                child,
            })
            .map_err(Into::into)
    }
}
impl UploadCancellation {
    pub fn key(&self) -> OperationKey {
        OperationKey(self.child.key())
    }
    pub fn submit(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .submit_held(&self.child)
            .map_err(Into::into)
    }
    pub fn wait(&self) -> Result<Arc<Result<bool, AppError>>, AdmissionError> {
        self.queue
            .worker
            .wait_retained(&self.child)
            .map_err(Into::into)
    }
    pub fn observe_canceled(&self) -> Result<UploadAcknowledgment, AdmissionError> {
        self.queue
            .worker
            .observe_upload_canceled(&self.parent, &self.child, false)?;
        Ok(UploadAcknowledgment(Receipt::Canceled(self.clone())))
    }
}
impl Operations {
    pub fn resume_upload_cancellation(
        &self,
        parent: &UploadJob<Draft, Reservation>,
        key: &OperationKey,
    ) -> Result<UploadCancellation, AdmissionError> {
        if !Arc::ptr_eq(&self.queue, &parent.queue) {
            return Err(AdmissionError::Stale);
        }
        self.queue
            .worker
            .reattach_upload_cancellation(&parent.held, &key.0)
            .map(|child| UploadCancellation {
                queue: self.queue.clone(),
                parent: parent.held.clone(),
                child,
            })
            .map_err(Into::into)
    }
}
