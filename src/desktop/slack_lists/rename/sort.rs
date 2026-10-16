//! One shared Number sort, staged without changing the current projection.
use super::*;
use super_platinum_core::slack_lists::{ViewSort, ViewSortChoice};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub original: ViewSortChoice,
    pub desired: ViewSortChoice,
    pub current: Option<ViewSortChoice>,
    pub choices: Vec<(String, String)>,
    pub choice: u64,
}
fn choices(doc: &Document) -> Vec<(String, String)> {
    doc.columns
        .iter()
        .filter(|c| ViewSort::supports_column(c))
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect()
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
    open_shell(&mut state.write(), visit, selection);
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
pub(super) fn change(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
    desired: ViewSortChoice,
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
        if e.sort_choice != choice
            || e.input != Input::Sort(rendered.clone())
            || *rendered == desired
        {
            return false;
        }
        if shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_none_or(|l| l.document != e.original)
        {
            return false;
        }
        let Target::Sort(view) = &e.target else {
            return false;
        };
        if ViewSort::current(&e.original, view).is_none() {
            return false;
        }
        if let ViewSortChoice::Number { column_id, .. } = &desired
            && e.original
                .column(column_id)
                .is_none_or(|c| !ViewSort::supports_column(c))
        {
            return false;
        }
        e.input = Input::Sort(desired);
        // Direction changes also rotate the callback identity, including A->B->A.
        e.sort_choice = next_id();
        true
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
        current: shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .and_then(|l| ViewSort::current(&l.document, view)),
        choices: choices(&e.original),
        choice: e.sort_choice,
    })
}
