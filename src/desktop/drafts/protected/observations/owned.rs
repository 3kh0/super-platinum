//! Actual family handles stay in accepted Work before any submit or await.
use super::super::owned_route::{Route, o, ui};
use super::*;

#[derive(Default)]
pub(super) struct State {
    reserved: Option<Result<o::ObservationReserved, o::AdmissionRefused<o::ObservationInput>>>,
    completion: Option<o::ObservationCompletion>,
    outcome: Option<o::ObservationOutcome>,
    acknowledgment: Option<o::ObservationAcknowledgment>,
    acknowledged: bool,
}
pub(super) enum ResultValue {
    Owned(o::ObservationOutcome),
    Legacy(durable::ObservationResult),
}
impl ResultValue {
    pub fn before(&self) -> &durable::Review {
        match self {
            Self::Owned(v) => v.before(),
            Self::Legacy(v) => v.before(),
        }
    }
    pub fn after(&self) -> &durable::Review {
        match self {
            Self::Owned(v) => v.after(),
            Self::Legacy(v) => v.after(),
        }
    }
}
pub(super) async fn run(
    queue: &ObservationQueue,
    work: &Work,
    shell: Signal<ShellState>,
    route: &Arc<Route>,
    stream: durable::Stream,
    before: durable::Review,
    event: super_platinum_core::drafts::sync::DraftObservation,
) -> Result<o::ObservationOutcome, durable::Error> {
    let reserved = {
        let mut state = work.storage.lock().map_err(|_| durable::Error::Storage)?;
        // Work is admitted once. A canceled waiter cannot replace its input.
        if state.reserved.is_none() {
            queue.source.inventory.clock.change();
            state.reserved = Some(
                route
                    .operations
                    .observe(o::ObservationInput::new(stream, before, event)),
            );
        }
        state
            .reserved
            .as_ref()
            .unwrap()
            .as_ref()
            .map_err(|e| ui(e.reason))?
            .clone()
    };
    reserved.submit().map_err(ui)?;
    let waiting = reserved.clone();
    let status = tokio::task::spawn_blocking(move || waiting.wait())
        .await
        .map_err(|_| durable::Error::Storage)?
        .map_err(ui)?;
    if let o::JobState::Failed(error) = status {
        return Err(error);
    }
    if status != o::JobState::Ready {
        return Err(durable::Error::Stale);
    }
    if !shell
        .try_peek()
        .is_ok_and(|shell| queue.source.current(&shell))
    {
        // Issued ticket remains in reserved. Revocation never fabricates finish.
        return Err(durable::Error::Gap);
    }
    let completion = {
        let mut state = work.storage.lock().map_err(|_| durable::Error::Storage)?;
        if state.completion.is_none() {
            queue.source.inventory.clock.change();
            state.completion = Some(reserved.finish().map_err(ui)?);
        }
        state.completion.as_ref().unwrap().clone()
    };
    completion.submit().map_err(ui)?;
    let result = tokio::task::spawn_blocking(move || completion.wait())
        .await
        .map_err(|_| durable::Error::Storage)?
        .map_err(ui)??;
    let mut state = work.storage.lock().map_err(|_| durable::Error::Storage)?;
    queue.source.inventory.clock.change();
    state.outcome = Some(result.clone());
    Ok(result)
}

/// Called only by the closed delivery path after the exact projection is handled.
/// Failed acknowledgment is retained, with no loss of the outcome or input.
pub(super) fn acknowledge(
    queue: &ObservationQueue,
    work: &Work,
    observed: &Observed,
) -> Result<(), durable::Error> {
    let mut state = work.storage.lock().map_err(|_| durable::Error::Storage)?;
    if state.acknowledged {
        return Err(durable::Error::Stale);
    }
    let Some(outcome) = state.outcome.as_ref().cloned() else {
        // Prepared::resolve may prove an already-observed no-op before a job exists.
        return if state.reserved.is_none() && observed.before.same(&observed.after) {
            Ok(())
        } else {
            Err(durable::Error::Stale)
        };
    };
    if !outcome.before().same(&observed.before) || !outcome.after().same(&observed.after) {
        return Err(durable::Error::Stale);
    }
    if state.acknowledgment.is_none() {
        let (actual, receipt) = state
            .completion
            .as_ref()
            .ok_or(durable::Error::Stale)?
            .observe_success()
            .map_err(ui)?;
        if !receipt.matches(&outcome) || !receipt.matches(&actual) {
            return Err(durable::Error::Stale);
        }
        state.acknowledgment = Some(receipt);
    }
    queue.source.inventory.clock.change();
    match state.acknowledgment.take().unwrap().acknowledge() {
        Ok(()) => {
            state.acknowledged = true;
            Ok(())
        }
        Err((error, receipt)) => {
            state.acknowledgment = Some(receipt);
            Err(ui(error))
        }
    }
}
