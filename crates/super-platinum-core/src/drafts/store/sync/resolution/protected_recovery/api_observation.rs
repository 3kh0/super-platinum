//! Ordering admission and storage projection remain distinct from DOM adoption.
use super::*;
pub(super) fn reserve(
    store: &DraftStore,
    stream: &Stream,
    expected: &Review,
    event: &crate::drafts::sync::DraftObservation,
) -> Result<ObservationTicket, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    let current = views::exact(&tx, expected)?;
    let inner = storage::reserve_observation(store, &stream.0, &current.inner, event)
        .map_err(|_| Error::Storage)?;
    tx.commit().map_err(|_| Error::Storage)?;
    if !stream.is_live() {
        return Err(Error::Gap);
    }
    Ok(ObservationTicket {
        inner,
        review: current,
    })
}
// Normal consuming API retains its original move semantics.
pub(super) fn finish(
    store: &DraftStore,
    ticket: ObservationTicket,
) -> Result<ObservationResult, Error> {
    let after = finish_after(store, &ticket)?;
    Ok(ObservationResult {
        before: ticket.review,
        after,
    })
}
// Same transaction/guards; only the private retained worker owns this Arc token.
#[cfg(any(test, feature = "protected-test-support"))]
pub(super) fn finish_retained(
    store: &DraftStore,
    ticket: &ObservationTicket,
) -> Result<ObservationResult, Error> {
    let after = finish_after(store, ticket)?;
    Ok(ObservationResult {
        before: ticket.review.clone(),
        after,
    })
}
fn finish_after(store: &DraftStore, ticket: &ObservationTicket) -> Result<Review, Error> {
    let tx = adapter_atomic::begin(&store.0).map_err(|_| Error::Storage)?;
    views::exact(&tx, &ticket.review)?;
    let event = ticket.inner.observation.as_ref().ok_or(Error::Stale)?;
    storage::finish_observation(store, &ticket.inner, event).map_err(|_| Error::Storage)?;
    let after = views::review(&tx, ticket.review.scope())?;
    tx.commit().map_err(|_| Error::Storage)?;
    if !ticket.inner.lease.is_live() {
        return Err(Error::Gap);
    }
    Ok(after)
}
