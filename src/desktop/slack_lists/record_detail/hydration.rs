//! One positive known-parent hydration per explicit detail mount.
//! Does not settle mutations or establish global population completeness.
use super::*;
use super_platinum_core::{slack::api, slack_lists::ListRecordHydration};
mod cooldown;
pub(crate) mod native_fixture;

#[derive(Clone, Copy, Default)]
pub(super) enum State {
    #[default]
    Idle,
    Pending(u64),
    Finished,
}

#[derive(Clone)]
struct Attempt {
    scope: Scope,
    cookie: Option<String>,
    enterprise: Option<String>,
    list: String,
    parent: String,
    visit: u64,
    open: u64,
    detail: u64,
    selection: u64,
    revision: u64,
    read: u64,
    serial: u64,
    document: Document,
    page: RecordsPage,
}

fn enterprise(shell: &ShellState, team: &str) -> Option<String> {
    shell
        .core
        .session
        .as_ref()?
        .workspaces
        .get(team)?
        .enterprise_id
        .clone()
}
impl Attempt {
    fn dispatch_ready(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && !cooldown::blocked(&self.scope)
            && !rename::creation_cooldown(&self.scope)
    }
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell.core.session.as_ref().map(|s| &s.d_cookie) == self.cookie.as_ref()
            && enterprise(shell, &self.scope.team) == self.enterprise
            && !self.scope.deleted(shell, &self.list)
            && shell.overlay == Some(Overlay::SlackList)
            && !create::blocks_mutation(shell)
            && !rename::blocks_mutation(shell)
            && !delete::blocks_mutation(shell)
            && !reminder::blocks_mutation(shell)
            && !completion::blocks_mutation(shell)
            && !editor::assignment::blocks_mutation(shell)
            && !shell.slack_lists.operations.contains_key(&self.list)
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                view.id == self.list
                    && view.generation == self.visit
                    && view.revision == self.revision
                    && view.read == self.read
                    && view.layout.selection_generation == self.selection
                    && !view.loading
                    && !view.stale
                    && !view.unavailable
                    && view.editor.is_none()
                    && view.loaded.as_ref().is_some_and(|loaded| {
                        loaded.document == self.document && loaded.records == self.page
                    })
                    && view.detail.as_ref().is_some_and(|detail| {
                        detail.open_generation == self.open
                            && detail.generation == self.detail
                            && detail.record_id == self.parent
                            && !detail.unavailable
                            && matches!(detail.hydration, State::Pending(id) if id == self.serial)
                            && canonical(view, detail).is_some()
                    })
            })
    }
}
fn claim(shell: &mut ShellState, open: u64) -> Option<Attempt> {
    let scope = shell.slack_lists.scope.clone()?;
    if cooldown::blocked(&scope) || rename::creation_cooldown(&scope) {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    let detail = view.detail.as_ref()?;
    let loaded = view.loaded.as_ref()?;
    if detail.open_generation != open
        || !matches!(detail.hydration, State::Idle)
        || loaded.records.complete()
        || view.retry_at > now()
    {
        return None;
    }
    let attempt = Attempt {
        cookie: shell.core.session.as_ref().map(|s| s.d_cookie.clone()),
        enterprise: enterprise(shell, &scope.team),
        scope,
        list: view.id.clone(),
        parent: detail.record_id.clone(),
        visit: view.generation,
        open,
        detail: detail.generation,
        selection: view.layout.selection_generation,
        revision: view.revision,
        read: view.read,
        serial: next_id(),
        document: loaded.document.clone(),
        page: loaded.records.clone(),
    };
    shell.slack_lists.view.as_mut()?.detail.as_mut()?.hydration = State::Pending(attempt.serial);
    if !attempt.current(shell) {
        shell.slack_lists.view.as_mut()?.detail.as_mut()?.hydration = State::Idle;
        return None;
    }
    Some(attempt)
}

fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<ListRecordHydration, Error>,
) -> bool {
    if let Err(error) = &result {
        rename::record_creation_limit(&attempt.scope, error);
    }
    // A stale view cannot adopt data, but the originating account still owes
    // Slack this cooldown. Record it before any projection-eligibility return.
    let retry_hint = result
        .as_ref()
        .err()
        .and_then(|error| cooldown::record(&attempt.scope, error));
    if !attempt.current(shell) {
        return false;
    }
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .detail
        .as_mut()
        .unwrap()
        .hydration = State::Finished;
    let merged = result.and_then(|hydration| {
        if hydration.list_id() != attempt.list || hydration.parent_id() != attempt.parent {
            return Err(Error::Api("list_hydration_target_mismatch".into()));
        }
        hydration
            .merge(&attempt.document, &attempt.page)
            .map_err(|error| Error::Api(error.to_string()))
    });
    match merged {
        Ok((document, records)) => {
            let view = shell.slack_lists.view.as_mut().unwrap();
            view.title = document.title.clone();
            let loaded = view.loaded.as_mut().unwrap();
            loaded.document = document;
            loaded.records = records;
            view.revision = next_id();
            rebuild_rows(shell);
            // Rebuild invalidates prior detail callbacks. No create/delete/cell
            // receipt is resolved from this partial, positively merged response.
            true
        }
        Err(_) => {
            let view = shell.slack_lists.view.as_mut().unwrap();
            view.retry_at = view.retry_at.max(retry_hint.unwrap_or(0));
            view.error = Some("Could not load this item's subtasks. Previously loaded items are still shown. Reopen the item to try again.".into());
            false
        }
    }
}

pub(super) fn start(mut state: Signal<ShellState>, open: u64) {
    // Existing offline fixtures never fall through to real sessions/network.
    let disconnected = fixture::enabled(&state.peek());
    if disconnected && !native_fixture::check_enabled() {
        return;
    }
    let Some(attempt) = claim(&mut state.write(), open) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !attempt.dispatch_ready(&state.peek()) {
            return;
        }
        if disconnected {
            let result = native_fixture::execute(state, &attempt).await;
            let applied = finish(&mut state.write(), &attempt, result);
            native_fixture::completed(attempt.serial, applied);
            return;
        }
        let Some(context) = attempt.scope.request(&state.peek()) else {
            finish(
                &mut state.write(),
                &attempt,
                Err(Error::TransportNotConfigured),
            );
            return;
        };
        // Final guard has no intervening await. Existing info transport sends
        // once, does not hold offline, and preserves rate-limit failures.
        let result = api::fetch_list_record_subtasks(
            &context.transport,
            &context.client,
            &context.session,
            &attempt.list,
            &attempt.parent,
        )
        .await;
        finish(&mut state.write(), &attempt, result);
    });
}

/// A canonical refresh can reveal incompleteness after a complete detail mount.
/// The same open may claim once when eligible; no failed claim is replayed.
pub(in crate::slack_lists) fn ensure(state: Signal<ShellState>) {
    let open = state
        .peek()
        .slack_lists
        .view
        .as_ref()
        .and_then(|view| view.detail.as_ref().map(|detail| detail.open_generation));
    if let Some(open) = open {
        start(state, open);
    }
}

#[cfg(test)]
mod tests;
