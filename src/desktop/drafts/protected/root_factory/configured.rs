//! Off-renderer cold acquisition. No ordinary load runs before this sequence.
use super::*;
use durable::owned_store::{FactoryIdentity, Initialized, OwnedRoute, StoreKind};

pub(super) fn initialize(id: &str, factory: &FactoryIdentity) -> Result<Initialized, Error> {
    let route = OwnedRoute::acquire_configured_cold(id)?;
    initialize_route(route, factory)
}

// Kept separate for actual isolated-worker tests, never accepts an account/path
// proof. Both callers must already hold an opaque core-owned route.
pub(super) fn initialize_route(
    route: OwnedRoute,
    factory: &FactoryIdentity,
) -> Result<Initialized, Error> {
    let mut review = route.inspect()?.wait()?;
    if review.metadata().kind != StoreKind::Protected7 {
        // Exact inspected ordinary/legacy image, not a guessed migration mode.
        route.commit(&review)?.wait()?;
        review = route.inspect()?.wait()?;
    }
    route.initialize_slash(&review)?.wait()?;
    let review = route.inspect()?.wait()?;
    route.initialize_hosted(&review)?.wait()?;
    // Initialization advances the worker fence; never reuse the old review.
    let opened = route.inspect()?.wait()?.open()?;
    Ok(opened.initialize_cold(factory)?.wait()?)
}
