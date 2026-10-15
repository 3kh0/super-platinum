//! Parked input is reviewable only when its exact writer version is durable.
use super::*;
use super::{
    editor::Editor,
    editor_proof::{Capture, ReviewLease},
};

impl Editor {
    pub(super) fn capture_settlement(
        &self,
        visit: Visit,
        live: &LocalDraft,
    ) -> Result<Capture, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.retired
            || state.running
            || state.busy
            || state.retry.is_some()
            || state.released.is_some()
            || state.handoff_lost
            || state.review.uncommitted().is_some()
            || state.writer.enrollment() != state.review.row().lineage().enrollment()
            || !visit.current()
        {
            return Err(durable::Error::Pending);
        }
        if !same_local(live, state.review.local())
            && !state
                .review
                .blocked()
                .is_some_and(|blocked| same_local(live, &blocked.draft))
            && !state
                .review
                .alternates()
                .iter()
                .any(|alternate| same_local(live, &alternate.draft))
        {
            return Err(durable::Error::ReviewRequired);
        }
        if let Some((serial, local)) = &state.pending {
            if !state.review.alternates().iter().any(|alternate| {
                alternate.writer_id == state.writer.id()
                    && alternate.revision == *serial
                    && same_local(&alternate.draft, local)
            }) || !state.review.origins().iter().any(|origin| {
                origin.writer_id() == state.writer.id()
                    && origin.origin()
                        == &durable::WriterOrigin::Enrollment(state.writer.enrollment().to_owned())
            }) {
                return Err(durable::Error::ReviewRequired);
            }
        }
        Ok(Capture {
            identity: self.identity.clone(),
            writer: state.writer.clone(),
            serial: state.serial,
            review: state.review.clone(),
            pending: state.pending.as_ref().map(|(_, local)| local.clone()),
            visit,
            visible: live.clone(),
        })
    }
    /// The UI renders this returned inventory before accepting a choice. It is
    /// not silently refreshed at confirmation; all intervening changes refuse.
    pub async fn prepare_settlement(
        &self,
        visit: Visit,
        live: &LocalDraft,
    ) -> Result<ReviewLease, durable::Error> {
        let _gate = self.gate.lock().await;
        let before = {
            let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.running
                || state.busy
                || state.retry.is_some()
                || state.released.is_some()
                || state.handoff_lost
            {
                return Err(durable::Error::Pending);
            }
            (
                state.serial,
                state.review.clone(),
                state.writer.id().to_owned(),
            )
        };
        let fresh = self.owner.review().await?;
        {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if state.serial != before.0
                || state.writer.id() != before.2
                || !state.review.same(&before.1)
                || state.running
                || state.busy
                || !fresh.row().same_enrollment(before.1.row())
                || !visit.current()
            {
                return Err(durable::Error::Stale);
            }
            state.review = fresh;
        }
        ReviewLease::capture(self.capture_settlement(visit, live)?)
    }
}
