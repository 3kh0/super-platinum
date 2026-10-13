//! Read tracking belongs to an explicitly opened file visit, never to refreshes
//! or picker changes. All requests use a single-attempt, no-offline-hold path.
use super::*;
use super_platinum_core::slack::api;

#[derive(Clone)]
struct Attempt {
    scope: Scope,
    id: String,
    visit: u64,
}
impl Attempt {
    fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell.overlay == Some(Overlay::SlackList)
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                view.id == self.id && view.generation == self.visit && !view.unavailable
            })
    }
}
fn claim(view: &mut View) -> Option<(bool, bool)> {
    let loaded = view.loaded.as_ref()?;
    if view.unavailable {
        return None;
    }
    let open = !view.layout.read_started;
    let personal = !view.layout.personal_started
        && loaded
            .list_experiments
            .as_ref()
            .is_some_and(|flags| flags.default_view_dual_write);
    if !open && !personal {
        return None;
    }
    view.layout.read_started |= open;
    view.layout.personal_started |= personal;
    Some((open, personal))
}
pub(super) fn ensure(mut state: Signal<ShellState>) {
    let (attempt, open_enabled, personal_enabled) = {
        let mut shell = state.write();
        if fixture::enabled(&shell) || shell.overlay != Some(Overlay::SlackList) {
            return;
        }
        let Some(scope) = shell
            .slack_lists
            .scope
            .as_ref()
            .filter(|scope| scope.request_ready(&shell))
            .cloned()
        else {
            return;
        };
        let Some(view) = shell.slack_lists.view.as_mut() else {
            return;
        };
        let Some((open, personal)) = claim(view) else {
            return;
        };
        (
            Attempt {
                scope,
                id: view.id.clone(),
                visit: view.generation,
            },
            open,
            personal,
        )
    };
    let mark = attempt.clone();
    let personal_attempt = attempt.clone();
    if open_enabled {
        dioxus::core::spawn_forever(async move {
            if !attempt.current(&state.peek()) {
                return;
            }
            let Some(context) = attempt.scope.request(&state.peek()) else {
                return;
            };
            let result = context
                .transport
                .execute_list_read_state(api::lists_open(
                    &context.client,
                    &context.session,
                    &attempt.id,
                ))
                .await
                .and_then(|value| api::parse_list_open(&value));
            let mut shell = state.write();
            if !attempt.current(&shell) {
                return;
            }
            if let Ok(result) = result {
                accept_open(&mut shell, &attempt, result);
            }
        });
    }
    if personal_enabled {
        dioxus::core::spawn_forever(async move {
            if !personal_attempt.current(&state.peek()) {
                return;
            }
            let Some(context) = personal_attempt.scope.request(&state.peek()) else {
                return;
            };
            let result = api::fetch_list_default_view(
                &context.transport,
                &context.client,
                &context.session,
                &personal_attempt.id,
            )
            .await;
            let mut shell = state.write();
            if !personal_attempt.current(&shell) {
                return;
            }
            if let Ok(personal) = result {
                let resolved_present = shell
                    .slack_lists
                    .view
                    .as_ref()
                    .unwrap()
                    .layout
                    .resolved_present;
                settle_personal(
                    &mut shell,
                    &personal_attempt,
                    personal.as_deref(),
                    resolved_present,
                );
            }
        });
    }
    if open_enabled {
        dioxus::core::spawn_forever(async move {
            if !mark.current(&state.peek()) {
                return;
            }
            let timestamp = lifecycle::now_ms();
            let eligible = state
                .peek()
                .slack_lists
                .view
                .as_ref()
                .and_then(|view| view.loaded.as_ref())
                .and_then(|loaded| loaded.document.raw.get("created"))
                .and_then(Value::as_u64)
                .is_some_and(|created| {
                    created > 0
                        && created
                            .checked_mul(1000)
                            .is_some_and(|created| created <= timestamp)
                });
            if !eligible {
                return;
            }
            let Some(context) = mark.scope.request(&state.peek()) else {
                return;
            };
            let _ = context
                .transport
                .execute_list_read_state(api::files_mark_read(
                    &context.client,
                    &context.session,
                    &mark.id,
                    timestamp,
                ))
                .await;
        });
    }
}
fn accept_open(shell: &mut ShellState, attempt: &Attempt, result: api::ListOpen) {
    if !attempt.current(shell) {
        return;
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    let previous_selection = view.layout.selected.clone();
    let read_changed = view.layout.last_read_timestamp != result.last_read_timestamp;
    view.layout.last_read_timestamp = result.last_read_timestamp;
    view.layout.resolved_present = result.resolved_default_view.is_some();
    if read_changed {
        date_context::remember_prior_read(view);
    }
    settle(shell, attempt, result.resolved_default_view.as_deref());
    if read_changed
        && shell
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|view| view.layout.selected == previous_selection)
    {
        date_context::prior_read_changed(shell);
    }
}

fn settle(shell: &mut ShellState, attempt: &Attempt, id: Option<&str>) {
    if !attempt.current(shell) {
        return;
    }
    if shell.slack_lists.operations.contains_key(&attempt.id) {
        return;
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    if view.editor.is_some() || view.layout.manual {
        return;
    }
    let Some(loaded) = view.loaded.as_ref() else {
        return;
    };
    let Some(selection) = view.layout.selection.as_mut() else {
        return;
    };
    match selection.apply_resolved(&loaded.document, id) {
        Ok(true) => {
            view.layout.selected = selection.selected_id.clone();
            view.layout.selection_generation = next_id();
            view.layout.generation = next_id();
            layout::rebuild(view, shell.core.workspaces.get(&attempt.scope.team));
        }
        Ok(false) => {}
        Err(error) => view.layout.notice = Some(error.reason),
    }
}
fn settle_personal(
    shell: &mut ShellState,
    attempt: &Attempt,
    id: Option<&str>,
    resolved_present: bool,
) {
    if !attempt.current(shell) || shell.slack_lists.operations.contains_key(&attempt.id) {
        return;
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    if view.editor.is_some() || view.layout.manual {
        return;
    }
    let Some(loaded) = view.loaded.as_ref() else {
        return;
    };
    let Some(selection) = view.layout.selection.as_mut() else {
        return;
    };
    match selection.apply_personal(&loaded.document, id, resolved_present) {
        Ok(true) => {
            view.layout.selected = selection.selected_id.clone();
            view.layout.selection_generation = next_id();
            view.layout.generation = next_id();
            layout::rebuild(view, shell.core.workspaces.get(&attempt.scope.team));
        }
        Ok(false) => {}
        Err(error) => view.layout.notice = Some(error.reason),
    }
}
#[cfg(test)]
mod tests;
