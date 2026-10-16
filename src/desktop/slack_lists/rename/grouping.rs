//! Existing shared view grouping, staged without changing the current projection.
use super::*;
use super_platinum_core::slack_lists::{ViewGrouping, ViewGroupingChoice};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub original: ViewGroupingChoice,
    pub desired: ViewGroupingChoice,
    pub current: Option<ViewGroupingChoice>,
    pub choices: Vec<(String, String)>,
    pub choice: u64,
}
fn choices(doc: &Document) -> Vec<(String, String)> {
    doc.columns
        .iter()
        .filter(|c| ViewGrouping::supports_column(c))
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
        || !matches!(
            view.layout.kind,
            crate::slack_lists::ProjectionLayout::Table
                | crate::slack_lists::ProjectionLayout::Board
        )
    {
        return false;
    }
    let Some(id) = selected_view(shell) else {
        return false;
    };
    if view
        .loaded
        .as_ref()
        .is_none_or(|l| !l.document.columns.iter().any(ViewGrouping::supports_column))
    {
        return false;
    }
    can_target(shell, &Target::Grouping(id.into()))
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
    open_target(shell, Target::Grouping(view), visit)
}
pub(crate) fn input(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewGroupingChoice,
    desired: ViewGroupingChoice,
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
    rendered: &ViewGroupingChoice,
    desired: ViewGroupingChoice,
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
                && e.target.is_grouping()
                && selection_current(shell, e)
        }) else {
            return false;
        };
        if e.grouping_choice != choice
            || e.input != Input::Grouping(rendered.clone())
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
        let Target::Grouping(view) = &e.target else {
            return false;
        };
        if ViewGrouping::current(&e.original, view).is_none() {
            return false;
        }
        if let ViewGroupingChoice::Column { column_id } = &desired
            && e.original
                .column(column_id)
                .is_none_or(|c| !ViewGrouping::supports_column(c))
        {
            return false;
        }
        e.input = Input::Grouping(desired);
        // Every field/None change rotates the callback identity, including A->B->A.
        e.grouping_choice = next_id();
        true
    })
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    let Target::Grouping(view) = &e.target else {
        return None;
    };
    let Input::Grouping(desired) = &e.input else {
        return None;
    };
    Some(Presentation {
        original: ViewGrouping::current(&e.original, view)?,
        desired: desired.clone(),
        current: shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .and_then(|l| ViewGrouping::current(&l.document, view)),
        choices: choices(&e.original),
        choice: e.grouping_choice,
    })
}

pub(super) fn projection_current(shell: &ShellState, target: &Target) -> bool {
    let Target::Grouping(id) = target else {
        return true;
    };
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    view.layout.selected.as_deref() == Some(id.as_str())
        && view.layout.notice.is_none()
        && view.loaded.as_ref().is_some_and(|l| {
            l.document.views.iter().any(|raw| {
                raw["id"] == id.as_str()
                    && match view.layout.kind {
                        crate::slack_lists::ProjectionLayout::Table => raw["type"] == "table",
                        crate::slack_lists::ProjectionLayout::Board => raw["type"] == "board",
                    }
            })
        })
}
pub(crate) fn confirm(state: Signal<ShellState>, serial: u64, visit: u64, choice: u64) {
    if confirm_current(&state.peek(), serial, visit, choice) {
        super::confirm(state, serial);
    }
}
pub(super) fn review_current(shell: &ShellState, e: &registry::Entry) -> bool {
    e.review
        .as_ref()
        .is_some_and(|p| p.navigation_current(shell))
        && projection_current(shell, &e.target)
}
pub(super) fn confirm_current(shell: &ShellState, serial: u64, visit: u64, choice: u64) -> bool {
    generation(shell) == Some(visit)
        && entry(shell).is_some_and(|e| {
            e.serial == serial
                && e.grouping_choice == choice
                && e.target.is_grouping()
                && e.stage == Stage::Editing
                && selection_current(shell, &e)
        })
}
