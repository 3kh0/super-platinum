//! Ordered shared supported-field sorts, staged without changing the current projection.
use super::*;
use super_platinum_core::slack_lists::{ViewSort, ViewSortChoice};
mod pending;
mod rows;
pub(crate) use pending::reserve;
pub(super) use rows::rows_for;
pub(crate) use rows::{Action, structural};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub original: ViewSortChoice,
    pub desired: ViewSortChoice,
    pub current: Option<ViewSortChoice>,
    pub choices: Vec<(String, String)>,
    pub choice: u64,
    pub rows: Vec<u64>,
}
fn choices(doc: &Document) -> Vec<(String, String)> {
    doc.columns
        .iter()
        .filter(|c| ViewSort::supports_column(c))
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect()
}
pub(crate) fn column_choice(
    shell: &ShellState,
    column_id: &str,
    ascending: bool,
) -> Option<ViewSortChoice> {
    shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .and_then(|l| l.document.column(column_id))
        .and_then(|c| ViewSort::choice(c, ascending))
}
pub(crate) fn valid_choice(shell: &ShellState, desired: &ViewSortChoice) -> bool {
    shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .is_some_and(|l| ViewSort::valid_choice(&l.document, desired))
}
pub(crate) fn can_open(shell: &ShellState) -> bool {
    if !owner::available(shell) || blocks_mutation(shell) {
        return false;
    }
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    if view.layout.notice.is_some()
        || view.layout.kind != crate::slack_lists::ProjectionLayout::Table
    {
        return false;
    }
    let Some(id) = selected_view(shell) else {
        return false;
    };
    if view
        .loaded
        .as_ref()
        .is_none_or(|l| !l.document.columns.iter().any(ViewSort::supports_column))
    {
        return false;
    }
    can_target(shell, &Target::Sort(id.into()))
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64, selection: u64) {
    local_refinement::open(state, local_refinement::Kind::Sort, visit, selection);
}
pub(crate) fn open_shell(shell: &mut ShellState, visit: u64, selection: u64) -> bool {
    if super::selection(shell) != Some(selection) || !can_open(shell) {
        return false;
    }
    let Some(view) = selected_view(shell).map(str::to_owned) else {
        return false;
    };
    open_target(shell, Target::Sort(view), visit)
}
pub(crate) fn input(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewSortChoice,
    desired: ViewSortChoice,
) {
    if change(&state.peek(), serial, visit, choice, &rendered, desired) {
        state.write();
    }
}
/// Resolve the event's ID to the canonical kind, then use the same immutable
/// rendered-choice/owner/ABA admission as direction and clear events.
pub(crate) fn input_column(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewSortChoice,
    column_id: String,
    ascending: bool,
) {
    if change_column(
        &state.peek(),
        serial,
        visit,
        choice,
        &rendered,
        &column_id,
        ascending,
    ) {
        state.write();
    }
}
pub(super) fn change_column(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
    column_id: &str,
    ascending: bool,
) -> bool {
    let desired = if column_id.is_empty() {
        Some(ViewSortChoice::None)
    } else {
        shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .and_then(|l| l.document.column(column_id))
            .and_then(|c| ViewSort::choice(c, ascending))
    };
    desired.is_some_and(|desired| change(shell, serial, visit, choice, rendered, desired))
}
pub(super) fn change(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
    desired: ViewSortChoice,
) -> bool {
    edit(shell, serial, visit, choice, rendered, |e| {
        if *rendered == desired || !ViewSort::valid_choice(&e.original, &desired) {
            return false;
        }
        e.input = Input::Sort(desired);
        e.sort_rows = rows_for(&e.input);
        e.sort_choice = next_id();
        true
    })
}
fn edit(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
    apply: impl FnOnce(&mut registry::Entry) -> bool,
) -> bool {
    edit_pending(shell, serial, visit, choice, rendered, None, apply)
}
fn edit_pending(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
    pending: Option<u64>,
    apply: impl FnOnce(&mut registry::Entry) -> bool,
) -> bool {
    if generation(shell) != Some(visit) || !owner::available(shell) {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    if registry::blocked(&key.owner) {
        return false;
    }
    with_registry(|r| {
        let Some(e) = r.entries.get_mut(&key).filter(|e| {
            e.serial == serial
                && e.stage == Stage::Editing
                && e.target.is_sort()
                && selection_current(shell, e)
        }) else {
            return false;
        };
        if e.sort_pending != pending
            || e.sort_choice != choice
            || e.input != Input::Sort(rendered.clone())
        {
            return false;
        }
        if !local_refinement::original_current(shell, e) {
            return false;
        }
        let Target::Sort(view) = &e.target else {
            return false;
        };
        if ViewSort::current(&e.original, view).is_none() {
            return false;
        }
        apply(e)
    })
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    let Target::Sort(view) = &e.target else {
        return None;
    };
    let Input::Sort(desired) = &e.input else {
        return None;
    };
    Some(Presentation {
        original: ViewSort::current(&e.original, view)?,
        desired: desired.clone(),
        current: local_refinement::current_document(shell, e)
            .and_then(|doc| ViewSort::current(doc, view)),
        choices: choices(&e.original),
        choice: e.sort_choice,
        rows: e.sort_rows.clone(),
    })
}

pub(crate) fn editable(shell: &ShellState, serial: u64, visit: u64, choice: u64) -> bool {
    let Some(e) = entry(shell) else {
        return false;
    };
    let Input::Sort(value) = e.input else {
        return false;
    };
    edit(shell, serial, visit, choice, &value, |_| true)
}
