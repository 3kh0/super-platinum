//! One shared Checkbox or Number filter, staged without changing the current projection.
use super::*;
use super_platinum_core::slack_lists::{NumberFilterOperator, ViewFilter, ViewFilterChoice};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub original: ViewFilterChoice,
    pub desired: ViewFilterChoice,
    pub current: Option<ViewFilterChoice>,
    pub choices: Vec<(String, String)>,
    pub choice: u64,
    pub numbers: Vec<String>,
}
fn choices(doc: &Document) -> Vec<(String, String)> {
    doc.columns
        .iter()
        .filter(|c| ViewFilter::supports_column(c))
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
        .is_none_or(|l| !l.document.columns.iter().any(ViewFilter::supports_column))
    {
        return false;
    }
    can_target(shell, &Target::Filter(id.into()))
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
    open_target(shell, Target::Filter(view), visit)
}
pub(crate) fn input(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewFilterChoice,
    desired: ViewFilterChoice,
) {
    if change(&state.peek(), serial, visit, choice, &rendered, desired) {
        state.write();
    }
}
pub(crate) fn change(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewFilterChoice,
    desired: ViewFilterChoice,
) -> bool {
    edit(shell, serial, visit, choice, rendered, |e| {
        let Input::Filter(current) = &e.input else {
            return false;
        };
        if current == &desired || !supported(&e.original, &desired) {
            return false;
        }
        e.input = Input::Filter(desired);
        // Field/condition changes rotate identity, including A->B->A.
        e.filter_choice = next_id();
        true
    })
}
fn supported(doc: &Document, choice: &ViewFilterChoice) -> bool {
    let (id, kind) = match choice {
        ViewFilterChoice::None => return true,
        ViewFilterChoice::Checkbox { column_id, .. } => (column_id, "checkbox"),
        ViewFilterChoice::Number { column_id, .. } => (column_id, "number"),
    };
    doc.column(id)
        .is_some_and(|c| c.kind == kind && ViewFilter::supports_column(c))
}
// Value is deliberately excluded: multiple input events can precede a render.
// Field/operator transitions rotate filter_choice, so even ABA cannot revive it.
fn same_condition(a: &ViewFilterChoice, b: &ViewFilterChoice) -> bool {
    match (a, b) {
        (
            ViewFilterChoice::Number {
                column_id: a,
                operator: x,
                ..
            },
            ViewFilterChoice::Number {
                column_id: b,
                operator: y,
                ..
            },
        ) => a == b && x == y,
        _ => a == b,
    }
}
fn edit(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewFilterChoice,
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
                && e.target.is_filter()
                && selection_current(shell, e)
        }) else {
            return false;
        };
        let Input::Filter(current) = &e.input else {
            return false;
        };
        if e.filter_choice != choice || !same_condition(current, rendered) {
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
        let Target::Filter(view) = &e.target else {
            return false;
        };
        if ViewFilter::current(&e.original, view).is_none() {
            return false;
        }
        apply(e)
    })
}
pub(crate) fn number_value(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewFilterChoice,
    value: String,
) {
    if value_change(&state.peek(), serial, visit, choice, &rendered, value) {
        state.write();
    }
}
pub(super) fn value_change(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewFilterChoice,
    value: String,
) -> bool {
    edit(shell, serial, visit, choice, rendered, |e| {
        let Input::Filter(ViewFilterChoice::Number {
            operator,
            value: old,
            ..
        }) = &mut e.input
        else {
            return false;
        };
        if !operator.requires_value() {
            return false;
        }
        // Retain an invalid draft. Only the core capture may admit a finite value.
        *old = Some(value);
        true
    })
}
pub(crate) fn number_operator(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewFilterChoice,
    operator: NumberFilterOperator,
) {
    if operator_change(&state.peek(), serial, visit, choice, &rendered, operator) {
        state.write();
    }
}
pub(super) fn operator_change(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewFilterChoice,
    operator: NumberFilterOperator,
) -> bool {
    edit(shell, serial, visit, choice, rendered, |e| {
        let Input::Filter(ViewFilterChoice::Number {
            operator: old,
            value,
            ..
        }) = &mut e.input
        else {
            return false;
        };
        if *old == operator {
            return false;
        }
        *old = operator;
        if !operator.requires_value() {
            *value = None;
        }
        e.filter_choice = next_id();
        true
    })
}
pub(crate) fn valid(choice: &ViewFilterChoice) -> bool {
    match choice {
        ViewFilterChoice::Number {
            operator, value, ..
        } => operator.valid_value(value.as_deref()),
        _ => true,
    }
}
/// The renderer supplied an actual value/validity snapshot. This never accepts a
/// stale field/operator, visit, owner, policy or source, and claims the shared job
/// in the same state borrow before a queued input can change the admitted value.
pub(crate) fn confirm_number(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: ViewFilterChoice,
    value: String,
    validity: bool,
) {
    let claimed = prepare_number(
        &mut state.write(),
        serial,
        visit,
        choice,
        &rendered,
        value,
        validity,
    );
    if let Some((guard, intent)) = claimed {
        network::save(state, guard, intent);
    }
}
pub(super) fn prepare_number(
    shell: &mut ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewFilterChoice,
    value: String,
    validity: bool,
) -> Option<(Guard, Intent)> {
    let ViewFilterChoice::Number { operator, .. } = rendered else {
        return None;
    };
    if !validity || !operator.requires_value() || !operator.valid_value(Some(&value)) {
        return None;
    }
    if !value_change(shell, serial, visit, choice, rendered, value) {
        return None;
    }
    prepare(shell, serial)
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    let Target::Filter(view) = &e.target else {
        return None;
    };
    let Input::Filter(desired) = &e.input else {
        return None;
    };
    Some(Presentation {
        original: ViewFilter::current(&e.original, view)?,
        desired: desired.clone(),
        current: shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .and_then(|l| ViewFilter::current(&l.document, view)),
        choices: choices(&e.original),
        choice: e.filter_choice,
        numbers: e
            .original
            .columns
            .iter()
            .filter(|c| c.kind == "number" && ViewFilter::supports_column(c))
            .map(|c| c.id.clone())
            .collect(),
    })
}
