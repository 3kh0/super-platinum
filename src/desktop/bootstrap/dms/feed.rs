use super::{Arc, Owner, ShellState};
use crate::state::MainView;
use dioxus::prelude::*;
use std::future::Future;
use super_platinum_core::slack::{Error, PreparedRequest, api};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Published,
    Failed,
    Retired,
}

#[derive(Clone)]
pub(super) struct Request {
    owner: Arc<Owner>,
    sequence: u64,
    visit: u64,
    append: bool,
    cursor: Option<String>,
    reconnect: Option<super::super::reconnect::Visit>,
}
impl Request {
    pub(super) fn begin(
        shell: &mut ShellState,
        owner: Arc<Owner>,
        append: bool,
        reconnect: Option<super::super::reconnect::Visit>,
    ) -> Option<Self> {
        if shell.main_view != MainView::Dms || reconnect.as_ref().is_some_and(|v| !v.current(shell))
        {
            return None;
        }
        if !owner.current(shell) {
            return None;
        }
        let cursor = if append {
            if shell.core.dms.loading {
                return None;
            }
            // A reservation does not consume its cursor. A superseding first-page
            // refresh may fail and must leave this continuation available.
            Some(shell.core.dms.next_cursor.clone()?)
        } else {
            None
        };
        if reconnect.is_none() {
            shell.core.dms.loading = true;
        }
        shell.core.dms.load_seq = shell.core.dms.load_seq.wrapping_add(1);
        Some(Self {
            owner,
            sequence: shell.core.dms.load_seq,
            visit: shell.thread_generation,
            append,
            cursor,
            reconnect,
        })
    }
    fn owns(&self, shell: &ShellState) -> bool {
        self.owner.current(shell) && shell.core.dms.load_seq == self.sequence
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.owns(shell)
            && shell.main_view == MainView::Dms
            // Rail navigation closes the thread even when no thread is open.
            // Its existing visit clock also detects leaving and returning.
            && shell.thread_generation == self.visit
            && self.reconnect.as_ref().is_none_or(|v| v.current(shell))
    }
}
struct Lease {
    state: Signal<ShellState>,
    request: Request,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut shell) = self.state.try_write() {
            // Navigation eligibility may retire independently of this owned slot.
            if self.request.owns(&shell) {
                shell.core.dms.loading = false;
            }
        }
    }
}

pub(crate) async fn load(
    state: Signal<ShellState>,
    reconnect: Option<&super::super::reconnect::Visit>,
) -> bool {
    let owner = state.try_read().ok().and_then(|s| Owner::capture(&s));
    let Some(owner) = owner else {
        return false;
    };
    let outcome = load_with(state, owner.clone(), false, reconnect.cloned(), |request| {
        owner.transport.execute(request)
    })
    .await;
    // Existing current-surface hydration also serves retained rows after failure.
    // A retired account/load/view must not start that optional follow-up.
    outcome != Outcome::Retired
        && state.try_read().ok().is_some_and(|s| {
            owner.current(&s)
                && s.main_view == MainView::Dms
                && reconnect.is_none_or(|v| v.current(&s))
        })
}
pub(super) async fn load_with<F, Fut>(
    mut state: Signal<ShellState>,
    owner: Arc<Owner>,
    append: bool,
    reconnect: Option<super::super::reconnect::Visit>,
    execute: F,
) -> Outcome
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let Some(request) = state
        .try_write()
        .ok()
        .and_then(|mut s| Request::begin(&mut s, owner, append, reconnect))
    else {
        return Outcome::Retired;
    };
    run(state, request, execute).await
}

pub(super) async fn run<F, Fut>(
    mut state: Signal<ShellState>,
    request: Request,
    execute: F,
) -> Outcome
where
    F: FnOnce(PreparedRequest) -> Fut,
    Fut: Future<Output = Result<serde_json::Value, Error>>,
{
    let owner = request.owner.clone();
    let cursor = request.cursor.clone();
    let lease = Lease { state, request };
    if !state
        .try_read()
        .ok()
        .is_some_and(|s| lease.request.current(&s))
    {
        return Outcome::Retired;
    }
    let result =
        api::fetch_client_dms_with(&owner.client, &owner.session, 100, cursor, execute).await;
    let Ok(mut shell) = state.try_write() else {
        return Outcome::Retired;
    };
    if !lease.request.current(&shell) {
        return Outcome::Retired;
    }
    match result {
        Ok(page) => {
            if !lease.request.append {
                shell.core.dms.entries.clear();
            }
            for entry in page.dms {
                shell.core.dms.upsert(entry);
            }
            shell.core.dms.next_cursor = page
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());
            shell.core.dms.loading = false;
            if !lease.request.append {
                shell.core.dms.loaded = true;
            }
            Outcome::Published
        }
        Err(error) => {
            shell.core.dms.loading = false;
            shell.report_failure(&error, "Direct messages need a connection", || {
                "Could not load direct messages. Please try again.".into()
            });
            Outcome::Failed
        }
    }
}

#[cfg(test)]
mod tests;
