//! Calendar-dependent layouts reproject at most once per minute while visible.
//! A timezone change can alter historical offsets without changing today's offset;
//! do not use only the current date/offset to suppress those projections. Ordinary
//! silent ticks compare the deadline and do not project before it is due.
use super::*;
use std::time::{Duration, Instant};
use super_platinum_core::slack_lists::saved_views::{
    self, DateContext, DateDependencies, SavedView,
};
const SAMPLE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Default)]
pub(super) struct State {
    pub context: Option<DateContext>,
    dependencies: DateDependencies,
    next_sample: Option<Instant>,
    fixed: Option<DateContext>,
    sampled: Option<DateContext>,
}

pub(super) fn prepare(
    state: &mut State,
    document: &Document,
    selected: Option<&SavedView>,
    last_read: Option<u64>,
) {
    state.dependencies = selected
        .map(|view| saved_views::date_dependencies(document, view))
        .unwrap_or_default();
    state.context = selected.map(|_| {
        state
            .sampled
            .take()
            .or_else(|| state.fixed.clone())
            .unwrap_or_else(|| DateContext::local_now(last_read))
    });
    state.next_sample =
        (selected.is_some() && state.fixed.is_none() && relevant(state.dependencies))
            .then(|| Instant::now() + SAMPLE_INTERVAL);
}
fn relevant(dependencies: DateDependencies) -> bool {
    dependencies.calendar || dependencies.prior_read || dependencies.clock
}
/// Native fixtures use an explicit calendar; live sessions cannot override it.
pub(crate) fn set_override(shell: &mut ShellState, context: DateContext) -> bool {
    if !fixture::enabled(shell) {
        return false;
    }
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return false;
    };
    view.layout.dates.fixed = Some(context);
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|scope| shell.core.workspaces.get(&scope.team));
    layout::rebuild(view, workspace);
    true
}
pub(super) fn remember_prior_read(view: &mut View) {
    if let Some(context) = view.layout.dates.fixed.as_mut() {
        *context = context
            .clone()
            .with_last_read_timestamp(view.layout.last_read_timestamp);
    }
}
pub(super) fn prior_read_changed(shell: &mut ShellState) {
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return;
    };
    if !view.layout.dates.dependencies.prior_read {
        return;
    }
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|scope| shell.core.workspaces.get(&scope.team));
    layout::rebuild(view, workspace);
}
pub(super) fn poll(shell: &mut ShellState) -> bool {
    poll_with(shell, Instant::now(), DateContext::local_now)
}
fn poll_with(
    shell: &mut ShellState,
    now: Instant,
    sample: impl FnOnce(Option<u64>) -> DateContext,
) -> bool {
    if shell.overlay != Some(Overlay::SlackList)
        || shell
            .slack_lists
            .scope
            .as_ref()
            .is_none_or(|scope| !scope.current(shell))
    {
        return false;
    }
    let Some(view) = shell
        .slack_lists
        .view
        .as_mut()
        .filter(|view| !view.unavailable)
    else {
        return false;
    };
    let dates = &mut view.layout.dates;
    if dates.next_sample.is_none_or(|deadline| now < deadline) {
        return false;
    }
    dates.next_sample = Some(now + SAMPLE_INTERVAL);
    let context = sample(view.layout.last_read_timestamp);
    let changed = dates.dependencies.calendar
        || dates.dependencies.clock
        || dates
            .context
            .as_ref()
            .is_none_or(|old| old.calendar_key() != context.calendar_key());
    if !changed {
        return false;
    }
    dates.sampled = Some(context);
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|scope| shell.core.workspaces.get(&scope.team));
    layout::rebuild(view, workspace);
    true
}
/// Unknown feature assignments cannot silently change date representation.
pub(super) fn uses_timestamps(loaded: &Loaded) -> Option<bool> {
    let personal = loaded
        .personal_task_lists
        .as_ref()
        .map(|policy| policy.contains(&loaded.document.id));
    match (personal, loaded.list_experiments.as_ref()) {
        (Some(false), _) => Some(false),
        (_, Some(flags)) if !flags.todos || flags.todos_rollback => Some(false),
        (Some(true), Some(_)) => Some(true),
        _ => None,
    }
}
#[cfg(test)]
mod tests;
