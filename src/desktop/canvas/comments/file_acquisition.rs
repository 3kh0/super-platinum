//! Finite absent/event-invalidated display acquisition for original visible roots.
//! Metadata arrival never navigates, replaces a row, or remounts its reply.
use super::{file_source::Cold, *};
use serde_json::{Value, json};
use std::sync::Arc;
mod known;
mod rate;
mod run;
pub(super) use known::observe;
pub(in crate::canvas) use run::{known_request, request};
#[cfg(test)]
mod tests;

const LOADING: &str = "Loading file details...";
const FAILED: &str = "File details could not be loaded. Retry when ready.";
const OFFLINE: &str = "File details are unavailable while offline. Retry when connected.";
const LIMITED: &str = "Slack asked us to wait before retrying file details.";
const RETRY_READY: &str = "You can retry loading file details now.";
const RETIRED: &str = "File details changed. Refresh comments to load them again.";
#[derive(Default)]
pub(super) struct State {
    attempts: Vec<Attempt>,
    worker: Option<Arc<()>>,
    limit: Option<Limit>,
}
struct Limit {
    scope: Scope,
    until: Option<Instant>,
}
#[derive(Clone)]
struct Attempt {
    token: Arc<()>,
    cold: Cold,
    phase: Phase,
    notice: Option<&'static str>,
    receipt: Option<Value>,
}
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Queued,
    Loading,
    Ready,
    Unavailable,
    Retired,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
            Self::Retired => "retired",
        }
    }
}
#[derive(Clone)]
pub(super) struct Retry {
    token: Arc<()>,
    cold: Cold,
}
pub(super) struct Presentation {
    pub phase: &'static str,
    pub notice: &'static str,
    pub retry: Option<Retry>,
    pub disabled: bool,
}
fn enabled(shell: &ShellState) -> bool {
    !crate::fixture::is_fixture()
        || fixture::canonical_files::cold::enabled(shell)
        || fixture::canonical_files::known::enabled(shell)
}
fn unavailable() -> Error {
    Error::Api("canvas_file_acquisition_retired".into())
}
fn cooling(shell: &ShellState, model: &super::State) -> bool {
    rate::current(shell).is_some()
        || model.file_acquisition.limit.as_ref().is_some_and(|limit| {
            limit.scope.stable_current(shell)
                && limit.until.is_none_or(|until| Instant::now() < until)
        })
}
fn sync_limit(shell: &ShellState, model: &mut super::State) {
    let Some(wait) = rate::current(shell) else {
        return;
    };
    let Some(owner) = model.owner.as_ref().filter(|o| o.current(shell)) else {
        return;
    };
    let until = model
        .file_acquisition
        .limit
        .as_ref()
        .filter(|l| l.scope.stable_current(shell))
        .map_or(wait.deadline(), |old| match (old.until, wait.deadline()) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        });
    model.file_acquisition.limit = Some(Limit {
        scope: owner.scope.clone(),
        until,
    });
}
pub(super) fn retire(state: &mut State) {
    state.attempts.clear();
    state.worker = None;
    // Endpoint debt survives a new page/read under the same stable Slack owner.
}
pub(super) fn after_read(
    state: Signal<ShellState>,
    mut model: Signal<super::State>,
    ticket: &Ticket,
) {
    let work = {
        let shell = state.peek();
        let value = model.peek();
        if !enabled(&shell)
            || !value.accepts(&shell, ticket)
            || value.loading
            || value.error.is_some()
        {
            return;
        }
        pending(&shell, &value)
    };
    if work.is_empty() {
        return;
    }
    let wake = {
        let shell = state.peek();
        let mut value = model.write();
        sync_limit(&shell, &mut value);
        let limited = cooling(&shell, &value);
        for cold in work {
            value.file_acquisition.attempts.push(Attempt {
                token: Arc::new(()),
                cold,
                phase: if limited {
                    Phase::Unavailable
                } else {
                    Phase::Queued
                },
                notice: limited.then_some(LIMITED),
                receipt: None,
            });
        }
        value
            .file_acquisition
            .limit
            .as_ref()
            .and_then(|l| l.until.map(|at| (l.scope.clone(), at)))
    };
    if let Some((scope, at)) = wake {
        run::arm(state, model, scope, at);
    }
    start(state, model);
}
fn pending(shell: &ShellState, model: &super::State) -> Vec<Cold> {
    let mut work: Vec<Cold> = Vec::new();
    for row in model.rows.iter().take(MAX_ROWS) {
        if let Some(cold) = file_source::cold(shell, model, &row.message) {
            if !work.iter().any(|old| old.id() == cold.id())
                && !model
                    .file_acquisition
                    .attempts
                    .iter()
                    .any(|a| a.cold.id() == cold.id())
            {
                work.push(cold);
            }
        }
    }
    work
}
fn slot_current(shell: &ShellState, model: &super::State, attempt: &Attempt) -> bool {
    attempt.cold.source_current(shell, model)
        && model
            .file_acquisition
            .attempts
            .iter()
            .any(|a| Arc::ptr_eq(&a.token, &attempt.token))
}
fn worker_current(model: &super::State, worker: &Arc<()>) -> bool {
    model
        .file_acquisition
        .worker
        .as_ref()
        .is_some_and(|old| Arc::ptr_eq(old, worker))
}
fn start(state: Signal<ShellState>, mut model: Signal<super::State>) {
    let worker = {
        let mut value = model.write();
        if value.file_acquisition.worker.is_some()
            || !value
                .file_acquisition
                .attempts
                .iter()
                .any(|a| a.phase == Phase::Queued)
        {
            return;
        }
        let worker = Arc::new(());
        value.file_acquisition.worker = Some(worker.clone());
        worker
    };
    // Explicit original guards retire work even when the spawned caller ends.
    dioxus::core::spawn_forever(run::worker(state, model, worker));
}
pub(super) fn presentation(
    shell: &ShellState,
    model: &super::State,
    message: &Message,
) -> Option<Presentation> {
    if !enabled(shell) {
        return None;
    }
    let attempt = model.file_acquisition.attempts.iter().find(|a| {
        message.ts.as_deref() == Some(a.cold.ts()) && a.cold.source_current(shell, model)
    })?;
    if attempt.phase == Phase::Ready {
        return None;
    }
    if !attempt.cold.current(shell, model) || attempt.phase == Phase::Retired {
        return Some(Presentation {
            phase: "retired",
            notice: RETIRED,
            retry: None,
            disabled: true,
        });
    }
    let loading = matches!(attempt.phase, Phase::Queued | Phase::Loading);
    let limited = cooling(shell, model);
    Some(Presentation {
        phase: attempt.phase.name(),
        notice: if loading {
            LOADING
        } else if limited {
            LIMITED
        } else if attempt.notice == Some(LIMITED) {
            RETRY_READY
        } else {
            attempt.notice.unwrap_or(FAILED)
        },
        retry: (!loading).then(|| Retry {
            token: attempt.token.clone(),
            cold: attempt.cold.clone(),
        }),
        disabled: loading || limited,
    })
}
pub(super) fn retry(state: Signal<ShellState>, mut model: Signal<super::State>, retry: Retry) {
    let accepted = {
        let shell = state.peek();
        let mut value = model.write();
        enabled(&shell) && requeue(&shell, &mut value, &retry)
    };
    if accepted {
        start(state, model);
    }
}
fn requeue(shell: &ShellState, value: &mut super::State, retry: &Retry) -> bool {
    sync_limit(shell, value);
    if !retry.cold.demanded(shell, value) || cooling(shell, value) {
        return false;
    }
    let Some(attempt) = value
        .file_acquisition
        .attempts
        .iter_mut()
        .find(|a| Arc::ptr_eq(&a.token, &retry.token) && a.phase == Phase::Unavailable)
    else {
        return false;
    };
    attempt.token = Arc::new(());
    attempt.phase = Phase::Queued;
    attempt.notice = None;
    attempt.receipt = None;
    true
}
pub(in crate::canvas) fn snapshot(shell: &ShellState, model: &super::State) -> Value {
    if !cfg!(test)
        && !fixture::canonical_files::cold::enabled(shell)
        && !fixture::canonical_files::known::enabled(shell)
    {
        return Value::Null;
    }
    json!({"running":model.file_acquisition.worker.is_some(),"cooling":cooling(shell,model),
        "rows":model.file_acquisition.attempts.iter().map(|a|{
            let mut row = json!({
            "id":a.cold.id(),"rootTs":a.cold.ts(),"phase":if a.cold.current(shell,model){a.phase.name()}else{"retired"},
            "current":a.cold.current(shell,model),"retryable":a.phase==Phase::Unavailable&&a.cold.demanded(shell,model)&&!cooling(shell,model),
            "notice":a.notice,"receipt":a.receipt
            });
            if a.cold.known() {
                row["demand"] = json!("known-event");
                row["eventRevision"] = json!(a.cold.event_revision());
            }
            row
        }).collect::<Vec<_>>()})
}
