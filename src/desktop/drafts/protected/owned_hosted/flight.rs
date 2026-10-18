//! Closed Send family held in the same tracked Composer, before every await.
use super::*;
pub(super) struct Flight {
    version: u64,
    capture: editor_proof::Capture,
    input: Arc<o::HostedPrepared>,
    claim: Result<o::HostedClaimed, o::AdmissionRefused<Arc<o::HostedPrepared>>>,
    completion: Option<o::HostedCompletion>,
    cleanup: Option<o::HostedCleanup>,
    receipt: Option<o::HostedReceipt>,
    clear: Option<o::HostedFreshClear>,
    acknowledgment: Option<o::HostedAcknowledgment>,
    acknowledged: bool,
}
impl Flight {
    pub(super) fn retired_by(&self, id: &str) -> bool {
        self.acknowledged && self.receipt.as_ref().is_some_and(|r| r.record().id() == id)
    }
    pub(super) fn acknowledged(&self) -> bool {
        self.acknowledged
    }
    pub(super) fn storage_failed(&self) -> bool {
        self.cleanup
            .as_ref()
            .is_some_and(|c| c.state() == o::JobState::Failed(durable::Error::Storage))
            || (self.cleanup.is_none()
                && self
                    .completion
                    .as_ref()
                    .is_some_and(|c| c.state() == o::JobState::Failed(durable::Error::Storage)))
    }
}
impl Composer {
    pub(crate) fn completed_for(
        &self,
        input: &Arc<o::HostedPrepared>,
    ) -> Result<String, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let flight = state.flight.as_ref().ok_or(durable::Error::Pending)?;
        if !Arc::ptr_eq(&flight.input, input) {
            return Err(durable::Error::Stale);
        }
        Ok(flight
            .receipt
            .as_ref()
            .ok_or(durable::Error::Pending)?
            .record()
            .id()
            .to_owned())
    }

    pub(crate) fn begin_send(
        &self,
        editor: &Arc<editor::Editor>,
        stream: durable::Stream,
        version: u64,
    ) -> Result<Arc<o::HostedPrepared>, durable::Error> {
        let capture = editor.capture(Visit::new())?;
        if capture.pending.is_some() {
            return Err(durable::Error::Pending);
        }
        editor.owner.check_stream(&stream)?;
        let route = editor
            .owner
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !state.loaded
            || !state.historical.is_empty()
            || !state.completed.is_empty()
            || state.retirement.is_some()
            || state.flight.is_some()
            || state.save.is_some()
            || state.version != version
            || state.persisted != version
            || state.current.is_none()
        {
            return Err(durable::Error::Pending);
        }
        let input = Arc::new(o::HostedPrepared::new(
            stream,
            capture.review.clone(),
            capture.writer.clone(),
            capture.serial,
            state.selection.input(&capture.review)?,
        )?);
        let flight = Flight {
            version,
            capture,
            input: input.clone(),
            claim: route.operations.hosted(input.clone()),
            completion: None,
            cleanup: None,
            receipt: None,
            clear: None,
            acknowledgment: None,
            acknowledged: false,
        };
        state.flight = Some(flight);
        Ok(input)
    }
    /// Checks the original actor serial, full review and ordered selection. No
    /// read/refresh can substitute a newer actor basis after the durable claim.
    pub(crate) fn send_current(
        &self,
        editor: &editor::Editor,
        input: &Arc<o::HostedPrepared>,
    ) -> bool {
        let Ok(state) = self.state.lock() else {
            return false;
        };
        let Some(flight) = state.flight.as_ref() else {
            return false;
        };
        Arc::ptr_eq(&flight.input, input)
            && state.version == flight.version
            && state
                .selection
                .input(&flight.capture.review)
                .is_ok_and(|v| &v == input.input())
            && editor
                .capture(flight.capture.visit.clone())
                .is_ok_and(|now| flight.capture.same(&now))
    }
    pub(crate) async fn claim_send(&self) -> Result<o::HostedCompletion, durable::Error> {
        let claim = {
            let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            state
                .flight
                .as_ref()
                .ok_or(durable::Error::Pending)?
                .claim
                .as_ref()
                .map_err(|e| ui(e.reason))?
                .clone()
        };
        claim.submit().map_err(ui)?;
        let waiting = claim.clone();
        tokio::task::spawn_blocking(move || waiting.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)?;
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let flight = state.flight.as_mut().ok_or(durable::Error::Pending)?;
        if flight.completion.is_none() {
            flight.completion = Some(claim.completion().map_err(ui)?);
        }
        flight.completion.clone().ok_or(durable::Error::Pending)
    }
    /// Returned attachment already lives in the core completion before this is
    /// called. Settlement never consults the active account or visible composer.
    pub(crate) async fn settle_send(&self) -> Result<(), durable::Error> {
        let completion = {
            let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            state
                .flight
                .as_ref()
                .and_then(|f| f.completion.clone())
                .ok_or(durable::Error::Pending)?
        };
        let waiting = completion.clone();
        tokio::task::spawn_blocking(move || waiting.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)?;
        completion.result().ok_or(durable::Error::Pending)??;
        let cleanup = {
            let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            let flight = state.flight.as_mut().ok_or(durable::Error::Pending)?;
            if flight.cleanup.is_none() {
                flight.cleanup = Some(completion.cleanup().map_err(ui)?);
            }
            flight.cleanup.clone().ok_or(durable::Error::Pending)?
        };
        cleanup.submit().map_err(ui)?;
        let waiting = cleanup.clone();
        tokio::task::spawn_blocking(move || waiting.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)?;
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let flight = state.flight.as_mut().ok_or(durable::Error::Pending)?;
        if flight.receipt.is_none() {
            let (receipt, clear, acknowledgment) = cleanup.observe().map_err(ui)?;
            flight.receipt = Some(receipt);
            flight.clear = clear;
            flight.acknowledgment = Some(acknowledgment);
        }
        let receipt = flight.receipt.clone().ok_or(durable::Error::Pending)?;
        if receipt.effect() == o::HostedEffect::ClearedExact {
            state.current = None;
        }
        if !state
            .completed
            .iter()
            .any(|r| r.record().id() == receipt.record().id())
        {
            state.completed.push(receipt);
        }
        Ok(())
    }
    pub(crate) async fn retry_returned_storage(&self) -> Result<(), durable::Error> {
        let (completion, cleanup) = {
            let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
            let flight = state.flight.as_ref().ok_or(durable::Error::Pending)?;
            (
                flight.completion.clone().ok_or(durable::Error::Pending)?,
                flight.cleanup.clone(),
            )
        };
        if let Some(cleanup) = cleanup {
            cleanup.retry_storage().map_err(ui)?;
        } else {
            completion.retry_storage().map_err(ui)?;
        }
        self.settle_send().await
    }
    /// Borrow the one fresh witness without consuming it on a failed native
    /// check. Historical receipts cannot recreate this clear opportunity.
    pub(crate) fn clear_basis(
        &self,
        editor: &editor::Editor,
    ) -> Result<durable::Review, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let flight = state.flight.as_ref().ok_or(durable::Error::Pending)?;
        let clear = flight
            .clear
            .as_ref()
            .ok_or(durable::Error::ReviewRequired)?;
        if state.version != flight.version
            || clear.original() != flight.input.input()
            || !editor
                .capture(flight.capture.visit.clone())
                .is_ok_and(|now| flight.capture.same(&now))
        {
            return Err(durable::Error::Stale);
        }
        clear.after()
    }
    /// Called only after the exact native clear receipt and final actor CAS.
    /// This advances the existing editor, never submits an empty text mutation.
    pub(crate) fn cleared(
        &self,
        editor: &editor::Editor,
        after: &durable::Review,
    ) -> Result<(), durable::Error> {
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let flight = state.flight.as_ref().ok_or(durable::Error::Pending)?;
        let clear = flight
            .clear
            .as_ref()
            .ok_or(durable::Error::ReviewRequired)?;
        if state.version != flight.version || !clear.after()?.same(after) {
            return Err(durable::Error::Stale);
        }
        let mut actor = editor.state.lock().map_err(|_| durable::Error::Storage)?;
        if actor.serial != flight.capture.serial
            || actor.pending.is_some()
            || actor.running
            || actor.busy
            || actor.retired
            || !actor.review.same(&flight.capture.review)
        {
            return Err(durable::Error::Stale);
        }
        actor.review = after.clone();
        state.selection = Selection::default();
        state.current = None;
        state.version = state
            .version
            .checked_add(1)
            .ok_or(durable::Error::Unsupported)?;
        state.persisted = state.version;
        state.flight.as_mut().ok_or(durable::Error::Pending)?.clear = None;
        Ok(())
    }
    pub(crate) fn acknowledge_send(&self) -> Result<(), durable::Error> {
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let flight = state.flight.as_mut().ok_or(durable::Error::Pending)?;
        if flight.acknowledged {
            return Ok(());
        }
        let acknowledgment = flight
            .acknowledgment
            .take()
            .ok_or(durable::Error::Pending)?;
        if let Err((error, acknowledgment)) = acknowledgment.acknowledge() {
            flight.acknowledgment = Some(acknowledgment);
            return Err(ui(error));
        }
        flight.acknowledged = true;
        // A preserved-current outcome remains visible for explicit review.
        if flight.clear.is_none() && state.selection.sources().is_empty() {
            state.flight = None;
        }
        Ok(())
    }
}
