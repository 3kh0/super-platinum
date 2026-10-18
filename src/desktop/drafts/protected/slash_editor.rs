//! ReviewLease admission and guarded post-Slash reconciliation, separate from Send.
use super::editor::Editor;
use super::owned_route::ui;
use super::*;

impl Editor {
    /// An actual keep_input acknowledgment changes no native or canonical text.
    /// It only adopts the independently read receipt under current native proof.
    pub(crate) fn reconcile_slash_keep<G: FnOnce() -> bool>(
        &self,
        child: &owned_slash::Slash,
        after: &durable::Review,
        native_current: G,
    ) -> Result<(), durable::Error> {
        if !child.acknowledged_without_clear() {
            return Err(durable::Error::Pending);
        }
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !Arc::ptr_eq(&self.identity, &child.capture.identity)
            || s.retired
            || s.running
            || s.busy
            || s.parked
            || s.error.is_some()
            || s.pending.is_some()
            || s.retry.is_some()
            || s.released.is_some()
            || s.handoff_lost
            || s.serial != child.capture.serial
            || s.writer.id() != child.capture.writer.id()
            || s.writer.enrollment() != child.capture.writer.enrollment()
            || !s.review.same(&child.capture.review)
            || !editor_proof::can_rebase(&s.review, after, &s.writer)
            || !native_current()
        {
            return Err(durable::Error::Stale);
        }
        s.review = after.clone();
        s.direct = after.row().state().local_preservation_intent().cloned();
        drop(s);
        self.notify();
        Ok(())
    }

    pub(crate) async fn prepare_slash<G>(
        self: &Arc<Self>,
        lease: ReviewLease,
        parent: &operation::Operation,
        guard: G,
    ) -> Result<Arc<owned_slash::Slash>, durable::Error>
    where
        G: FnOnce() -> Result<durable::Stream, durable::Error>,
    {
        if !parent.belongs(self) || !parent.current() {
            return Err(durable::Error::Stale);
        }
        let operation::Request::Slash {
            context,
            choice,
            client_token,
        } = &parent.record().request
        else {
            return Err(durable::Error::Unsupported);
        };
        self.flush().await?;
        let _gate = self.gate.lock().await;
        let fresh = self.owner.review().await?;
        let captured = self.capture(lease.visit())?;
        if !fresh.same(lease.review()) {
            return Err(durable::Error::Stale);
        }
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.retired
            || state.serial != captured.serial
            || !state.review.same(&captured.review)
            || state.running
            || state.busy
            || state.parked
            || state.pending.is_some()
        {
            return Err(durable::Error::Stale);
        }
        let stream = guard()?;
        self.owner.check_stream(&stream)?;
        let expected = lease.claim(&captured)?;
        let route = self
            .owner
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        let input = context
            .prepare(
                &route.operations,
                stream,
                expected,
                captured.writer.clone(),
                captured.serial,
                choice,
                client_token,
            )
            .map_err(ui)?;
        let child = owned_slash::Slash::new(&route.operations, context.clone(), captured, input);
        parent.retain(|s| s.owned_slash = Some(child.clone()))?;
        child.submit()?;
        // No editor busy/network lock: later typing stays admitted and invalidates clear.
        drop(state);
        Ok(child)
    }
    pub(crate) fn slash_current(&self, child: &owned_slash::Slash) -> bool {
        self.state.lock().is_ok_and(|s| {
            Arc::ptr_eq(&self.identity, &child.capture.identity)
                && !s.retired
                && !s.running
                && !s.busy
                && !s.parked
                && s.error.is_none()
                && s.pending.is_none()
                && s.retry.is_none()
                && s.released.is_none()
                && !s.handoff_lost
                && s.serial == child.capture.serial
                && s.writer.id() == child.capture.writer.id()
                && s.writer.enrollment() == child.capture.writer.enrollment()
                && s.review.same(&child.capture.review)
        })
    }
    pub(crate) async fn slash_canonical(
        &self,
        child: &owned_slash::Slash,
    ) -> Result<durable::Review, durable::Error> {
        let review = self.owner.review().await?;
        child.canonical(review.clone())?;
        Ok(review)
    }
    pub(crate) fn reconcile_slash<G: FnOnce() -> bool>(
        &self,
        child: &owned_slash::Slash,
        after: &durable::Review,
        project: G,
    ) -> Result<(), durable::Error> {
        if !child.permits_reconcile(after) {
            return Err(durable::Error::Stale);
        }
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !Arc::ptr_eq(&self.identity, &child.capture.identity)
            || s.retired
            || s.running
            || s.busy
            || s.parked
            || s.error.is_some()
            || s.pending.is_some()
            || s.retry.is_some()
            || s.released.is_some()
            || s.handoff_lost
            || s.serial != child.capture.serial
            || s.writer.id() != child.capture.writer.id()
            || s.writer.enrollment() != child.capture.writer.enrollment()
            || !s.review.same(&child.capture.review)
            || after.scope() != self.scope()
            || !after.local().empty()
            || !after.row().same_enrollment(child.input.review().row())
            || !editor_proof::clean(after)
        {
            return Err(durable::Error::Stale);
        }
        if !project() {
            return Err(durable::Error::Stale);
        }
        s.review = after.clone();
        s.direct = after.row().state().local_preservation_intent().cloned();
        drop(s);
        self.notify();
        Ok(())
    }
}
