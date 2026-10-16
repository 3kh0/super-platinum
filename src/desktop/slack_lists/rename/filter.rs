//! A complete shared Checkbox/Number AND draft; canonical projection changes only on readback.
use super::*;
mod capture;
#[cfg(test)]
pub(super) use capture::finish_at;
mod rows;
pub(crate) use capture::{Capture, Snapshot, finish, release, reserve};
pub(super) use rows::rows_for;
pub(crate) use rows::{Action, Row, editable, structural, update_value};
use super_platinum_core::slack_lists::{NumberFilterOperator, ViewFilter, ViewFilterChoice};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub original: ViewFilterChoice,
    pub desired: ViewFilterChoice,
    pub current: Option<ViewFilterChoice>,
    pub choices: Vec<(String, String)>,
    pub choice: u64,
    pub numbers: Vec<String>,
    pub rows: Vec<Row>,
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
        e.filter_rows = rows_for(&e.input);
        // Field/condition changes rotate identity, including A->B->A.
        e.filter_choice = next_id();
        true
    })
}
fn supported(doc: &Document, choice: &ViewFilterChoice) -> bool {
    let (id, kind) = match choice {
        ViewFilterChoice::None => return true,
        ViewFilterChoice::All(rows) => {
            return rows.len() <= 8
                && rows
                    .iter()
                    .all(|r| r.column_id().is_some() && supported(doc, r));
        }
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
                && e.filter_capture.is_none()
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
        ViewFilterChoice::All(rows) => {
            let mut seen = std::collections::BTreeSet::new();
            (2..=8).contains(&rows.len())
                && rows
                    .iter()
                    .all(|row| row.column_id().is_some_and(|id| seen.insert(id)) && valid(row))
        }
        _ => true,
    }
}
/// Existing single-row state tests now enter the complete-set capture boundary.
#[cfg(test)]
pub(super) fn prepare_number(
    shell: &mut ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewFilterChoice,
    value: String,
    validity: bool,
) -> Option<(Guard, Intent)> {
    let ViewFilterChoice::Number {
        column_id,
        operator,
        ..
    } = rendered
    else {
        return None;
    };
    let e = entry(shell)?;
    let Input::Filter(current) = &e.input else {
        return None;
    };
    if !same_condition(current, rendered) || e.filter_rows.len() != 1 {
        return None;
    }
    let capture = reserve(shell, serial, visit, choice)?;
    let row = &e.filter_rows[0];
    let snapshot = vec![Snapshot {
        id: row.id.to_string(),
        epoch: row.epoch.to_string(),
        column: column_id.clone(),
        operator: operator.as_str().into(),
        value: Some(value),
        valid: validity,
    }];
    let result = finish(shell, &capture, snapshot);
    if result.is_none() {
        release(shell, &capture);
    }
    result
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
        rows: e.filter_rows.clone(),
        numbers: e
            .original
            .columns
            .iter()
            .filter(|c| c.kind == "number" && ViewFilter::supports_column(c))
            .map(|c| c.id.clone())
            .collect(),
    })
}

/// Native snapshot submission shares the existing one-attempt network worker.
pub(crate) fn submit(
    mut state: Signal<ShellState>,
    capture: &Capture,
    snapshot: Vec<Snapshot>,
) -> bool {
    let claimed = finish(&mut state.write(), capture, snapshot);
    if let Some((guard, intent)) = claimed {
        network::save(state, guard, intent);
        true
    } else {
        false
    }
}

#[cfg(test)]
pub(crate) fn close_fixture(shell: &ShellState) -> bool {
    let Some(e) = entry(shell).filter(|e| {
        e.target.is_filter() && e.stage == Stage::Editing && e.job.is_none() && e.intent.is_none()
    }) else {
        return false;
    };
    super::close_entry(shell, e.serial)
}
