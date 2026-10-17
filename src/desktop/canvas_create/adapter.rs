//! Canvas creation delivery custody. This TMP overlay is not activated in main.
use super_platinum_core::canvas::creation as domain;

#[path = "custody.rs"]
mod custody;
#[path = "delivery.rs"]
mod delivery;
#[path = "recovery.rs"]
mod recovery;
#[path = "source.rs"]
mod source;
#[path = "storage.rs"]
mod storage;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub(super) use custody::{Custody, Phase, Ticket};
pub(super) use source::LiveSource;
pub(super) use storage::Storage;

use std::{future::Future, pin::Pin};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient},
};

// This seam is private to the Canvas controller. Native fixture implementation
// must possess its exact startup capability; no environment-to-live fallback.
pub(super) trait Source: 'static {
    fn owner(&self) -> &domain::Owner;
    fn current(&self) -> bool;
    fn client(&self) -> &SlackClient;
    fn workspace(&self) -> &WorkspaceSession;
    // Fixture-only observation of the actual builder output. Live sources do nothing.
    fn prepared(&self, _request: &PreparedRequest) {}
    fn execute(
        &self,
        request: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, Error>> + '_>>;
}

pub(super) fn retain(future: impl Future<Output = ()> + 'static) {
    #[cfg(not(test))]
    {
        // Pinned Dioxus 57d6794 global_context.rs:199: explicitly ROOT scope.
        // Never expose the Task to a dialog cleanup/cancellation handler.
        dioxus::core::spawn_forever(future);
    }
    #[cfg(test)]
    {
        tokio::task::spawn_local(future);
    }
}

pub(super) fn now() -> domain::Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|v| v.as_secs())
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or(domain::Fault::Invalid)
}
