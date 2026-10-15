//! Exact rich-dialog consumption; no renderer, transport, or enrollment policy.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReviewedScheduleError {
    #[error("the reviewed draft predecessor changed")]
    Stale,
    #[error("this draft is not an idle acknowledged draft without files")]
    Ineligible,
    #[error("the reviewed schedule has unsupported routing or content")]
    Snapshot,
    #[error(transparent)]
    Consume(#[from] ConsumeError),
}

impl DraftSync {
    /// Freeze dialog blocks verbatim against an exact durable predecessor.
    ///
    /// The caller must ALSO check its retained Editor instance, input serial,
    /// complete paired LocalDraft, storage CAS revision and dialog visit. Full
    /// value equality here cannot detect an identical row recreated later.
    /// Persist the returned state paired with a cleared local draft before any
    /// native clear or network dispatch. Input arriving during that transaction
    /// remains queued on the same Editor and uses terminal local preservation.
    ///
    /// Retained B (SHA256 1681ecc18bbfead52efb391b1ccfb3e15f1b4a1093a8475d847b221ce196c537)
    /// Id6n 6810246..6811835 freezes the edited draft, marks is_from_composer,
    /// sets date_scheduled and chooses update for an existing server identity.
    /// This entry point deliberately supports only that known-ID branch. It
    /// does not widen the existing request_consume path or authorize retries.
    pub fn request_reviewed_schedule(
        &mut self,
        predecessor: &DraftSync,
        reviewed: Snapshot,
        post_at: i64,
    ) -> Result<ConsumeIntent, ReviewedScheduleError> {
        if self != predecessor {
            return Err(ReviewedScheduleError::Stale);
        }
        let (Some(base), Some(desired)) = (&self.acknowledged, &self.desired) else {
            return Err(ReviewedScheduleError::Ineligible);
        };
        if self.workflow != Workflow::Idle
            || self.consumption.is_some()
            || self.blocked_observation.is_some()
            || !self.consumption_valid()
            || !self.recovery_state_valid()
            || base.client_msg_id != self.client_msg_id
            || Self::adopt_remote(&self.scope, base.clone()).is_err()
            || !base.snapshot.file_ids.is_empty()
            || !desired.file_ids.is_empty()
        {
            return Err(ReviewedScheduleError::Ineligible);
        }
        // The dialog edits blocks and time, not routing or attachment ownership.
        // Preserve destination metadata exactly, including explicit false.
        // Reuse the existing lossless raw-shape validator with the ACTUAL base
        // identity/revision. This validates structure, not editor semantics;
        // valid rich-editor blocks can be outside the reversible text subset.
        if !reviewed.file_ids.is_empty()
            || reviewed.destinations != desired.destinations
            || reviewed.destinations != base.snapshot.destinations
            || reviewed.blocks.is_empty()
            || Self::adopt_remote(
                &self.scope,
                RemoteDraft {
                    snapshot: reviewed.clone(),
                    ..base.clone()
                },
            )
            .is_err()
        {
            return Err(ReviewedScheduleError::Snapshot);
        }
        // Prepare on a clone so invalid time/revision exhaustion leaves every
        // original field intact. Do not create an intermediate ordinary Update
        // or convert the dialog through LocalDraft/codec::encode.
        let mut next = self.clone();
        next.desired = Some(reviewed);
        let intent = next.request_consume(
            &self.scope,
            self.local_revision,
            ConsumeKind::Schedule { post_at },
        )?;
        *self = next;
        Ok(intent)
    }
}

#[cfg(test)]
mod tests;
