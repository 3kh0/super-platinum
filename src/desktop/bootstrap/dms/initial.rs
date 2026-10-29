//! Explicit first-page recovery. Scroll continuation remains a separate type.
use super::feed::{Outcome, Request};
use super::{Arc, Frame, ShellState};
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::future::Future;
use super_platinum_core::slack::{Error, PreparedRequest, api, models::User};
#[derive(Clone)]
pub(crate) struct Source {
    frame: Arc<Frame>,
    sequence: u64,
}
pub(crate) fn phase(shell: &ShellState) -> bool {
    !shell.core.dms.loaded
        && !shell
            .core
            .dms
            .next_cursor
            .as_ref()
            .is_some_and(|c| !c.is_empty())
}
impl Source {
    pub(crate) fn capture(shell: &ShellState, frame: &Arc<Frame>) -> Option<Self> {
        let source = Self {
            frame: frame.clone(),
            sequence: shell.core.dms.load_seq,
        };
        source.current(shell).then_some(source)
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.frame.current(shell)
            && phase(shell)
            && !shell.core.dms.loading
            && self.sequence == shell.core.dms.load_seq
    }
    fn reserve(self, shell: &mut ShellState) -> Option<Request> {
        if !self.current(shell) {
            return None;
        }
        Request::begin(shell, self.frame.owner(), false, None)
    }
}
pub(crate) async fn load(state: Signal<ShellState>, source: Source) {
    let owner = source.frame.owner();
    load_with(
        state,
        source,
        |request| owner.transport.execute(request),
        |ids| api::fetch_users_info(&owner.transport, &owner.client, &owner.session, ids),
    )
    .await;
}
pub(super) async fn load_with<F, Fut, L, LFut>(
    mut state: Signal<ShellState>,
    source: Source,
    execute: F,
    lookup: L,
) -> Outcome
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    L: FnMut(Vec<String>) -> LFut,
    LFut: Future<Output = Result<Vec<User>, Error>>,
{
    let owner = source.frame.owner();
    let Some(request) = state
        .try_write()
        .ok()
        .and_then(|mut s| source.reserve(&mut s))
    else {
        return Outcome::Retired;
    };
    let accepted = request.clone();
    let result = super::feed::run(state, request, execute).await;
    // Primary rows and busy state are already published before optional names.
    // The accepted feed scope survives its own loaded-state transition; local
    // filters do not redefine this account-owned feed, just like primary reads.
    if result == Outcome::Published {
        names::hydrate_with(state, &accepted, &owner, lookup).await;
    }
    result
}
mod names;
#[cfg(test)]
mod tests;
