//! Modal-only staging of one saved Table visibility bit. No local layout override.
use super::*;
use super_platinum_core::slack_lists::ViewFieldVisibility;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub column: String,
    pub field: String,
    pub original: bool,
    pub desired: bool,
    pub current: Option<bool>,
    pub choices: Vec<(String, String)>,
    pub choice: u64,
}
fn choices(doc: &Document, view: &str) -> Vec<(String, String)> {
    doc.columns
        .iter()
        .filter(|c| ViewFieldVisibility::current(doc, view, &c.id).is_some())
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect()
}
pub(crate) fn can_open(shell: &ShellState) -> bool {
    if !owner::available(shell) || blocks_mutation(shell) {
        return false;
    }
    if shell.slack_lists.view.as_ref().is_none_or(|v| {
        v.layout.notice.is_some()
            || v.layout.kind
                != super_platinum_core::slack_lists::saved_views::ProjectionLayout::Table
    }) {
        return false;
    }
    let Some(view) = selected_view(shell) else {
        return false;
    };
    let Some(loaded) = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
    else {
        return false;
    };
    let Some(column) = loaded
        .document
        .columns
        .iter()
        .find(|c| ViewFieldVisibility::current(&loaded.document, view, &c.id).is_some())
    else {
        return false;
    };
    can_target(
        shell,
        &Target::Visibility {
            view: view.into(),
            column: column.id.clone(),
        },
    )
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64, selection: u64) {
    local_refinement::open(state, local_refinement::Kind::Visibility, visit, selection);
}
pub(crate) fn open_shell(shell: &mut ShellState, visit: u64, selection: u64) -> bool {
    if super::selection(shell) != Some(selection) || !can_open(shell) {
        return false;
    }
    let Some(view) = selected_view(shell).map(str::to_owned) else {
        return false;
    };
    let Some(column) = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .and_then(|l| choices(&l.document, &view).first().map(|c| c.0.clone()))
    else {
        return false;
    };
    open_target(shell, Target::Visibility { view, column }, visit)
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: String,
    column: String,
) {
    if change(
        &state.peek(),
        serial,
        visit,
        choice,
        &rendered,
        Some(&column),
        None,
    ) {
        state.write();
    }
}
pub(crate) fn input(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: String,
    value: bool,
) {
    if change(
        &state.peek(),
        serial,
        visit,
        choice,
        &rendered,
        None,
        Some(value),
    ) {
        state.write();
    }
}
pub(super) fn change(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &str,
    column: Option<&str>,
    value: Option<bool>,
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
                && e.target.is_visibility()
                && selection_current(shell, e)
        }) else {
            return false;
        };
        if !local_refinement::original_current(shell, e) {
            return false;
        }
        let Target::Visibility { view, column: old } = &e.target else {
            return false;
        };
        if e.visibility_choice != choice || old != rendered {
            return false;
        }
        let choosing = column.is_some();
        let column = column.unwrap_or(old);
        let Some(original) = ViewFieldVisibility::current(&e.original, view, column) else {
            return false;
        };
        let name = e.original.column(column).unwrap().name.clone();
        e.target = Target::Visibility {
            view: view.clone(),
            column: column.into(),
        };
        e.before = name;
        e.input = Input::Visibility(value.unwrap_or(original));
        if choosing {
            e.visibility_choice = next_id();
        }
        true
    })
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    let Target::Visibility { view, column } = &e.target else {
        return None;
    };
    let Input::Visibility(desired) = e.input else {
        return None;
    };
    Some(Presentation {
        column: column.clone(),
        field: e.before.clone(),
        original: ViewFieldVisibility::current(&e.original, view, column)?,
        desired,
        current: local_refinement::current_document(shell, e)
            .and_then(|doc| ViewFieldVisibility::current(doc, view, column)),
        choices: choices(&e.original, view),
        choice: e.visibility_choice,
    })
}
