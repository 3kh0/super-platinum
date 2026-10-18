//! A baseline choice is an applied full-inventory result, never a Consume.
//! Cleanup requires the actual acknowledged native projection of that exact
//! retained choice. Original terminal failure/native evidence is not rewritten.
use super::*;
use crate::drafts::protected::{actions::ProjectionBarrier, projection::Acknowledged};

impl Operation {
    pub(crate) fn retain_baseline_finish(
        &self,
        finish: Arc<super::super::owned_baseline::Finish>,
    ) -> Result<(), durable::Error> {
        if !matches!(&self.0.request, Request::Baseline { .. }) || !self.current() {
            return Err(durable::Error::Stale);
        }
        let mut held = self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if held.status != Status::Running
            || held
                .baseline_finish
                .as_ref()
                .is_some_and(|old| !Arc::ptr_eq(old, &finish))
        {
            return Err(durable::Error::Stale);
        }
        held.baseline_finish = Some(finish);
        Ok(())
    }
    fn matches_baseline(&self, held: &Evidence, barrier: &Arc<ProjectionBarrier>) -> bool {
        let Request::Baseline { evidence, .. } = &self.0.request else {
            return false;
        };
        barrier.before().scope() == &self.0.scope
            && barrier.after().scope() == &self.0.scope
            && barrier.before().same(evidence.review())
            && barrier.serial() == self.0.serial
            && barrier.after().row().state().consumption().is_none()
            && (matches!((&held.baseline_finish, &barrier.owned_finish),
                (Some(held), Some(actual)) if Arc::ptr_eq(held, actual))
                || self.matches_legacy_baseline(held, barrier))
    }
    fn matches_legacy_baseline(&self, held: &Evidence, barrier: &Arc<ProjectionBarrier>) -> bool {
        held.baseline_finish.is_none()
            && barrier.owned_finish.is_none()
            && self
                .0
                .account
                .upgrade()
                .is_some_and(|account| account.owned.is_none())
    }
    pub(crate) fn applied(&self, value: &Applied) -> Result<(), durable::Error> {
        if !matches!(&self.0.request, Request::Baseline { .. }) {
            return self.consumed(value);
        }
        let barrier = value.barrier.as_ref().ok_or(durable::Error::Stale)?;
        let mut held = self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if !self.matches_baseline(&held, barrier)
            || !barrier.before().same(&value.before)
            || !barrier.after().same(&value.after)
            || value.serial != self.0.serial
            || held
                .baseline_applied
                .as_ref()
                .is_some_and(|old| !Arc::ptr_eq(old, barrier))
        {
            return Err(durable::Error::Stale);
        }
        held.baseline_applied = Some(barrier.clone());
        Ok(())
    }
    pub(crate) fn record_baseline_projection(
        &self,
        proof: Arc<Acknowledged>,
    ) -> Result<(), durable::Error> {
        let mut held = self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if !std::sync::Weak::ptr_eq(&self.0.editor, &proof.editor)
            || !self.matches_baseline(&held, &proof.barrier)
            || !proof.review.same(proof.barrier.after())
            || held
                .baseline_applied
                .as_ref()
                .is_some_and(|old| !Arc::ptr_eq(old, &proof.barrier))
            || held
                .baseline_projection
                .as_ref()
                .is_some_and(|old| !Arc::ptr_eq(&old.barrier, &proof.barrier))
        {
            return Err(durable::Error::Stale);
        }
        // A storage retry creates its barrier only after the original failed
        // action ended. The exact held Finish Arc binds that late result here.
        held.baseline_applied = Some(proof.barrier.clone());
        held.baseline_projection = Some(proof);
        Ok(())
    }
    pub(crate) fn baseline_pending(&self) -> bool {
        matches!(&self.0.request, Request::Baseline { .. })
            && self
                .0
                .evidence
                .lock()
                .is_ok_and(|v| v.baseline_finish.is_some())
    }
    pub(crate) fn baseline_projection_pending(&self) -> bool {
        self.0
            .evidence
            .lock()
            .is_ok_and(|v| v.baseline_projection.is_some())
    }
    pub(crate) fn acknowledge_baseline_projection(&self) -> Result<Status, durable::Error> {
        let account = self.0.account.upgrade().ok_or(durable::Error::Stale)?;
        let mut records = account
            .operations
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if records
            .entries
            .get(&self.id())
            .is_none_or(|r| !Arc::ptr_eq(r, &self.0))
        {
            return Err(durable::Error::Stale);
        }
        let held = self
            .0
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if !matches!(held.status, Status::Finished | Status::Failed(_)) {
            return Err(durable::Error::Pending);
        }
        let proof = held
            .baseline_projection
            .as_ref()
            .ok_or(durable::Error::Pending)?;
        if !self.matches_baseline(&held, &proof.barrier)
            || !std::sync::Weak::ptr_eq(&self.0.editor, &proof.editor)
            || !proof.review.same(proof.barrier.after())
        {
            return Err(durable::Error::Stale);
        }
        // Idempotent only for this actual previously observed Review. A prior
        // refused core retirement can be retried here without reprojecting DOM,
        // rereading Slack, or minting another choice/job.
        if let Some(finish) = &held.baseline_finish {
            finish.acknowledge(&proof.review)?;
        } else if account.owned.is_some() {
            return Err(durable::Error::Stale);
        }
        let observed = held.status;
        let finish = held.baseline_finish.clone();
        drop(held);
        records.entries.remove(&self.id());
        drop(records);
        // Projection may have completed while core retirement was temporarily
        // refused. Release its Editor-held reference after the same receipt
        // finally retires, without taking Editor state under the record lock.
        if let (Some(editor), Some(finish)) = (self.0.editor.upgrade(), finish) {
            if let Ok(mut state) = editor.state.lock() {
                state
                    .owned_baseline_finishes
                    .retain(|held| !Arc::ptr_eq(held, &finish));
            }
        }
        Ok(observed)
    }
}

#[cfg(test)]
#[path = "baseline_tests.rs"]
pub(super) mod tests;
