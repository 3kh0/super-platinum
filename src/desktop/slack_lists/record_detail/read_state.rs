//! One captured record-open request per mounted explicit detail visit.
//! Responses are not canonical records and never change the local detail.
use super::*;
use super_platinum_core::slack::api;
#[cfg(test)]
mod tests;

#[derive(Clone)]
struct Attempt {
    scope: Scope,
    list: String,
    record: String,
    visit: u64,
    open: u64,
    created: Option<Value>,
    creator: Option<Value>,
}
impl Attempt {
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell.overlay == Some(Overlay::SlackList)
            && !self.scope.deleted(shell, &self.list)
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                view.id == self.list
                    && view.generation == self.visit
                    && !view.unavailable
                    && !view.stale
                    && view.detail.as_ref().is_some_and(|detail| {
                        detail.open_generation == self.open
                            && detail.record_id == self.record
                            && !detail.unavailable
                            && detail.selection_generation == view.layout.selection_generation
                            && canonical(view, detail).is_some_and(|record| {
                                record.raw.get("date_created") == self.created.as_ref()
                                    && record.raw.get("created_by") == self.creator.as_ref()
                            })
                    })
            })
    }
}
fn claim(shell: &mut ShellState, open: u64) -> Option<Attempt> {
    let view = shell.slack_lists.view.as_ref()?;
    let detail = view.detail.as_ref()?;
    if detail.open_generation != open || detail.read_started {
        return None;
    }
    let attempt = Attempt {
        scope: shell.slack_lists.scope.clone()?,
        list: view.id.clone(),
        record: detail.record_id.clone(),
        visit: view.generation,
        open,
        created: detail.created.clone(),
        creator: detail.creator.clone(),
    };
    if !attempt.current(shell) {
        return None;
    }
    shell
        .slack_lists
        .view
        .as_mut()?
        .detail
        .as_mut()?
        .read_started = true;
    Some(attempt)
}
/// Called by the stable detail component's mount, including accepted-created
/// records. Ordinary shell updates, field paging and fresh reads do not mount it.
pub(crate) fn track_open(mut state: Signal<ShellState>, open_generation: u64) {
    super::hydration::start(state, open_generation);
    let Some(attempt) = claim(&mut state.write(), open_generation) else {
        return;
    };
    if fixture::enabled(&state.peek()) {
        if attempt.current(&state.peek()) {
            fixture::record_open(
                &mut state.write(),
                &attempt.list,
                &attempt.record,
                attempt.open,
            );
        }
        return;
    }
    dioxus::core::spawn_forever(async move {
        if !attempt.current(&state.peek()) {
            trace(&attempt, "dispatch", false, "stale_owner");
            return;
        }
        let Some(context) = attempt.scope.request(&state.peek()) else {
            trace(&attempt, "dispatch", false, "unavailable_session");
            return;
        };
        // No await after the final ownership check; this transport never waits
        // for offline recovery or retries an ambiguous read-state side effect.
        trace(&attempt, "dispatch", true, "started");
        let result = context
            .transport
            .execute_list_read_state(api::lists_records_open(
                &context.client,
                &context.session,
                &attempt.list,
                &attempt.record,
            ))
            .await;
        trace(&attempt, "complete", result.is_ok(), outcome(&result));
        // Deliberately no state mutation: error/offline/late completion cannot
        // close details, adopt returned fields, or send this opening again.
    });
}
fn outcome(result: &Result<Value, Error>) -> &'static str {
    match result {
        Ok(_) => "success",
        Err(Error::Offline(_)) => "offline",
        Err(Error::Transport(_)) => "transport",
        Err(Error::Api(_)) => "api",
        Err(Error::RateLimited { .. }) => "rate_limited",
        Err(Error::HttpStatus { .. }) => "http",
        Err(Error::TransportNotConfigured | Error::MissingWorkspace(_)) => "unavailable_session",
        Err(Error::UploadCanceled) => "cancelled",
    }
}
fn trace(attempt: &Attempt, phase: &str, accepted: bool, outcome: &str) {
    if std::env::var("SUPER_PLATINUM_LIST_RECORD_OPEN_TRACE").as_deref() != Ok("1") {
        return;
    }
    eprintln!(
        "list-record-open: {}",
        serde_json::json!({"phase":phase,"generation":attempt.open,"visit":attempt.visit,
            "list_id":attempt.list,"record_id":attempt.record,"accepted":accepted,"outcome":outcome})
    );
}
