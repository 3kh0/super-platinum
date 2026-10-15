//! Reserve capacity before fixture I/O; retain every acquisition and failure.
//! No ordinary path/loader and no Current-cutover or graceful-drain capability.
use super::*;
use durable::owned_store::{OwnedRoute, StoreKind, StoreReview};
use std::sync::OnceLock;

pub(super) enum FixtureOrigin {
    Fresh,
    Reopen(String),
}
pub(super) struct Slot {
    root: Arc<()>,
    route: OnceLock<OwnedRoute>,
    prepared: Arc<Job<Prepared>>,
    acquired: Mutex<Option<Acquisition>>,
}
struct Prepared {
    review: StoreReview,
}
#[derive(Clone)]
pub(super) struct Reservation(Arc<Slot>);
/// Exhaustive for this inactive fixture source. LegacyOwned intentionally has no
/// constructor: ordinary ownership has not been implemented or asserted here.
pub(super) enum Decision {
    Pending,
    NeedsConsent(StoreReview),
    ProtectedOwned,
    Unavailable(Error),
}
impl Reservation {
    pub async fn ready(&self) {
        self.0.prepared.wait().ready().await;
    }
    pub fn decision(&self) -> Decision {
        match self.0.prepared.result() {
            Err(Error::Pending) => Decision::Pending,
            Err(error) => Decision::Unavailable(error),
            Ok(prepared) => {
                if self.0.route.get().and_then(OwnedRoute::opened).is_some() {
                    Decision::ProtectedOwned
                } else {
                    Decision::NeedsConsent(prepared.review.clone())
                }
            }
        }
    }
    // Only after real inspection succeeded. These opaque handles are for the
    // future explicit consent controller; it cannot synthesize StoreReview.
    pub fn route(&self) -> Result<OwnedRoute, Error> {
        self.0.prepared.result()?;
        self.0.route.get().cloned().ok_or(Error::Stale)
    }
}
impl Root {
    pub(super) fn occupied(&self) -> usize {
        self.accounts.len()
            + self
                .reservations
                .iter()
                .filter(|slot| {
                    slot.acquired
                        .lock()
                        .map_or(true, |acquired| acquired.is_none())
                })
                .count()
    }
    pub(super) fn reserve_fixture(&mut self, origin: FixtureOrigin) -> Result<Reservation, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if self.occupied() >= MAX_ACCOUNTS {
            return Err(Error::Capacity);
        }
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| Error::Storage(durable::Error::Storage))?;
        let slot = Arc::new(Slot {
            root: self.identity.clone(),
            route: OnceLock::new(),
            prepared: Job::new(),
            acquired: Mutex::new(None),
        });
        // Slot is visible and counted before even scheduling opening/inspection.
        self.reservations.push(slot.clone());
        let completion = slot.prepared.completion();
        let owned = slot.clone();
        runtime.spawn_blocking(move || {
            let result = (|| {
                let route = match origin {
                    FixtureOrigin::Fresh => OwnedRoute::fresh_fixture(),
                    FixtureOrigin::Reopen(ticket) => OwnedRoute::reopen_fixture(&ticket),
                }?;
                owned.route.set(route.clone()).map_err(|_| Error::Stale)?;
                let review = route.inspect()?.wait()?;
                if review.metadata().kind == StoreKind::Protected7 {
                    // Already protected: validate actual handoff, no Commit or
                    // normalization and no implicit consent for older formats.
                    review.clone().open()?;
                }
                Ok(Prepared { review })
            })();
            completion.finish(result);
        });
        Ok(Reservation(slot))
    }
    pub(super) fn retained_reservations(&self) -> Vec<Reservation> {
        self.reservations.iter().cloned().map(Reservation).collect()
    }
    pub(super) fn acquire_reserved(
        &mut self,
        reservation: &Reservation,
    ) -> Result<Acquisition, Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if !Arc::ptr_eq(&reservation.0.root, &self.identity)
            || !self
                .reservations
                .iter()
                .any(|slot| Arc::ptr_eq(slot, &reservation.0))
        {
            return Err(Error::Identity);
        }
        let mut acquired = reservation
            .0
            .acquired
            .lock()
            .map_err(|_| Error::Storage(durable::Error::Storage))?;
        if let Some(value) = &*acquired {
            return Ok(value.clone());
        }
        reservation.0.prepared.result()?;
        let opened = reservation
            .0
            .route
            .get()
            .and_then(OwnedRoute::opened)
            .ok_or(Error::Pending)?;
        let source = Source::owned(opened);
        // The reserved slot is exchanged for an account entry synchronously;
        // no extra capacity is admitted and no async gap exposes it as free.
        let value = self.acquire_inner(source, true)?;
        *acquired = Some(value.clone());
        Ok(value)
    }
}
