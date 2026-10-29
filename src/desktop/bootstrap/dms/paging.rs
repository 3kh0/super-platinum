//! Continuation reads retain the render that requested them, even through a DOM probe.
use super::feed::{Outcome, Request};
use super::{Arc, Frame, ShellState};
use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use std::future::Future;
use super_platinum_core::slack::{Error, PreparedRequest};

#[derive(Clone)]
pub(crate) struct Source {
    frame: Arc<Frame>,
    cursor: String,
    sequence: u64,
}
impl Source {
    pub(crate) fn capture(shell: &ShellState, frame: &Arc<Frame>) -> Option<Self> {
        if !frame.current(shell) {
            return None;
        }
        let cursor = shell
            .core
            .dms
            .next_cursor
            .as_ref()
            .filter(|c| !c.is_empty())?
            .clone();
        Some(Self {
            frame: frame.clone(),
            cursor,
            sequence: shell.core.dms.load_seq,
        })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.frame.current(shell)
            && !shell.core.dms.loading
            && shell.core.dms.load_seq == self.sequence
            && shell.core.dms.next_cursor.as_ref() == Some(&self.cursor)
    }
    fn reserve(self, shell: &mut ShellState) -> Option<Request> {
        // Validate the original continuation and reserve it in the same mutation.
        if !self.current(shell) {
            return None;
        }
        Request::begin(shell, self.frame.owner(), true, None)
    }
}

pub(crate) async fn load_more(state: Signal<ShellState>, source: Source) {
    load_more_if(state, source, async { true }).await;
}
pub(crate) async fn load_more_if(
    state: Signal<ShellState>,
    source: Source,
    near_bottom: impl Future<Output = bool>,
) {
    let owner = source.frame.owner();
    load_if_with(state, source, near_bottom, |request| {
        owner.transport.execute(request)
    })
    .await;
}
pub(super) async fn load_if_with<F, Fut>(
    mut state: Signal<ShellState>,
    source: Source,
    near_bottom: impl Future<Output = bool>,
    execute: F,
) -> Outcome
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    if !state.try_read().ok().is_some_and(|s| source.current(&s)) {
        return Outcome::Retired;
    }
    if !near_bottom.await {
        return Outcome::Retired;
    }
    let Some(request) = state
        .try_write()
        .ok()
        .and_then(|mut s| source.reserve(&mut s))
    else {
        return Outcome::Retired;
    };
    super::feed::run(state, request, execute).await
}
#[cfg(test)]
mod tests;
