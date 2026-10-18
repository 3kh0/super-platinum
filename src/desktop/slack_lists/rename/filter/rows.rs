//! Stable row identity is separate from the canonical wire condition vector.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub id: u64,
    pub epoch: u64,
}
pub(crate) fn rows_for(input: &Input) -> Vec<Row> {
    let Input::Filter(value) = input else {
        return vec![];
    };
    value
        .conditions()
        .iter()
        .map(|_| Row {
            id: next_id(),
            epoch: next_id(),
        })
        .collect()
}
#[derive(Clone)]
pub(crate) enum Action {
    Add(String),
    Remove(u64),
    Clear,
    Field(u64, u64, String),
    Operator(u64, u64, NumberFilterOperator),
    SelectOperator(u64, u64, SelectFilterOperator),
    DateOperator(u64, u64, DateFilterOperator),
    DateValue(u64, u64, String),
}
fn default(doc: &Document, id: &str) -> Option<ViewFilterChoice> {
    let c = doc.column(id).filter(|c| ViewFilter::supports_column(c))?;
    Some(if c.kind == "number" {
        ViewFilterChoice::Number {
            column_id: id.into(),
            operator: NumberFilterOperator::Equal,
            value: None,
        }
    } else if c.kind == "date" {
        ViewFilterChoice::Date {
            column_id: id.into(),
            operator: DateFilterOperator::Equal,
            value: None,
        }
    } else if c.kind == "select" {
        ViewFilterChoice::Select {
            column_id: id.into(),
            operator: SelectFilterOperator::Contains,
            values: vec![],
        }
    } else {
        ViewFilterChoice::Checkbox {
            column_id: id.into(),
            checked: false,
        }
    })
}
pub(super) fn ready(shell: &ShellState, serial: u64, visit: u64) -> Option<Key> {
    if generation(shell) != Some(visit) || !owner::available(shell) {
        return None;
    }
    let key = Key::current(shell)?;
    if registry::blocked(&key.owner) {
        return None;
    }
    let e = entry(shell)?;
    let Target::Filter(view) = &e.target else {
        return None;
    };
    if e.serial != serial
        || e.stage != Stage::Editing
        || e.filter_capture.is_some()
        || !selection_current(shell, &e)
        || shell.slack_lists.view.as_ref()?.loaded.as_ref()?.document != e.original
        || ViewFilter::current(&e.original, view).is_none()
    {
        return None;
    }
    Some(key)
}
/// Return the exact successful structural focus destination, never a position key.
pub(crate) fn structural(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    structure: u64,
    action: Action,
) -> Option<(u64, String)> {
    let key = ready(shell, serial, visit)?;
    with_registry(|r| {
        let e = r.entries.get_mut(&key)?;
        if e.filter_choice != structure {
            return None;
        }
        let Input::Filter(value) = &e.input else {
            return None;
        };
        let mut values = value.conditions().to_vec();
        if values.len() != e.filter_rows.len() {
            return None;
        }
        let target = match action {
            Action::Add(id) => {
                if values.len() >= 8 || values.iter().any(|v| v.column_id() == Some(&id)) {
                    return None;
                }
                values.push(default(&e.original, &id)?);
                let row = Row {
                    id: next_id(),
                    epoch: next_id(),
                };
                let target = format!("filter-operator-{}", row.id);
                e.filter_rows.push(row);
                target
            }
            Action::Remove(id) => {
                let i = e.filter_rows.iter().position(|r| r.id == id)?;
                values.remove(i);
                e.filter_rows.remove(i);
                e.filter_rows
                    .get(i)
                    .or_else(|| e.filter_rows.last())
                    .map(|r| format!("filter-field-{}", r.id))
                    .unwrap_or_else(|| "slack-list-filter-add-field".into())
            }
            Action::Clear => {
                if values.is_empty() {
                    return None;
                }
                values.clear();
                e.filter_rows.clear();
                "slack-list-filter-add-field".into()
            }
            Action::Field(id, epoch, field) => {
                let i = e
                    .filter_rows
                    .iter()
                    .position(|r| r.id == id && r.epoch == epoch)?;
                if values[i].column_id() == Some(&field)
                    || values
                        .iter()
                        .enumerate()
                        .any(|(n, v)| n != i && v.column_id() == Some(&field))
                {
                    return None;
                }
                values[i] = default(&e.original, &field)?;
                e.filter_rows[i].epoch = next_id();
                format!("filter-field-{id}")
            }
            Action::DateValue(id, epoch, selected) => {
                let i = e
                    .filter_rows
                    .iter()
                    .position(|r| r.id == id && r.epoch == epoch)?;
                let ViewFilterChoice::Date {
                    operator, value, ..
                } = &mut values[i]
                else {
                    return None;
                };
                if !operator.requires_value()
                    || (selected != "custom" && !DateFilterOperator::is_preset(&selected))
                {
                    return None;
                }
                if selected == "custom" {
                    if !value.as_deref().is_some_and(DateFilterOperator::is_preset) {
                        return None;
                    }
                    // Captured Custom -> on changes operator to Equal; retain no invented date.
                    *operator = DateFilterOperator::Equal;
                    *value = None;
                } else {
                    if value.as_deref() == Some(selected.as_str()) {
                        return None;
                    }
                    *value = Some(selected);
                }
                e.filter_rows[i].epoch = next_id();
                format!("filter-date-kind-{id}")
            }
            Action::DateOperator(id, epoch, op) => {
                let i = e
                    .filter_rows
                    .iter()
                    .position(|r| r.id == id && r.epoch == epoch)?;
                let ViewFilterChoice::Date {
                    operator, value, ..
                } = &mut values[i]
                else {
                    return None;
                };
                if *operator == op {
                    return None;
                }
                *operator = op;
                if !op.requires_value() {
                    *value = None;
                }
                e.filter_rows[i].epoch = next_id();
                format!("filter-operator-{id}")
            }
            Action::SelectOperator(id, epoch, op) => {
                let i = e
                    .filter_rows
                    .iter()
                    .position(|r| r.id == id && r.epoch == epoch)?;
                let ViewFilterChoice::Select {
                    operator, values, ..
                } = &mut values[i]
                else {
                    return None;
                };
                if *operator == op {
                    return None;
                }
                *operator = op;
                if !op.requires_values() {
                    values.clear();
                }
                e.filter_rows[i].epoch = next_id();
                format!("filter-operator-{id}")
            }
            Action::Operator(id, epoch, op) => {
                let i = e
                    .filter_rows
                    .iter()
                    .position(|r| r.id == id && r.epoch == epoch)?;
                match &mut values[i] {
                    ViewFilterChoice::Checkbox { checked, .. } => {
                        let desired = match op {
                            NumberFilterOperator::IsEmpty => false,
                            NumberFilterOperator::IsNotEmpty => true,
                            _ => return None,
                        };
                        if *checked == desired {
                            return None;
                        }
                        *checked = desired;
                    }
                    ViewFilterChoice::Number {
                        operator, value, ..
                    } => {
                        if *operator == op {
                            return None;
                        }
                        *operator = op;
                        if !op.requires_value() {
                            *value = None;
                        }
                    }
                    _ => return None,
                }
                e.filter_rows[i].epoch = next_id();
                format!("filter-operator-{id}")
            }
        };
        e.input = Input::Filter(ViewFilterChoice::from_conditions(values));
        e.filter_choice = next_id();
        Some((e.filter_choice, target))
    })
}
pub(crate) fn update_value(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    row: u64,
    epoch: u64,
    value: String,
) -> bool {
    // Retain invalid current input; the snapshot/core boundary enforces wire bounds.
    let Some(key) = ready(shell, serial, visit) else {
        return false;
    };
    with_registry(|r| {
        let Some(e) = r.entries.get_mut(&key) else {
            return false;
        };
        let Some(i) = e
            .filter_rows
            .iter()
            .position(|r| r.id == row && r.epoch == epoch)
        else {
            return false;
        };
        let Input::Filter(input) = &e.input else {
            return false;
        };
        let mut values = input.conditions().to_vec();
        match values.get_mut(i) {
            Some(ViewFilterChoice::Number {
                operator,
                value: old,
                ..
            }) if operator.requires_value() => *old = Some(value),
            Some(ViewFilterChoice::Date {
                operator,
                value: old,
                ..
            }) if operator.requires_value()
                && !old.as_deref().is_some_and(DateFilterOperator::is_preset)
                && !DateFilterOperator::is_preset(&value) =>
            {
                *old = Some(value)
            }
            _ => return false,
        }
        e.input = Input::Filter(ViewFilterChoice::from_conditions(values));
        true
    })
}

pub(crate) fn editable(shell: &ShellState, serial: u64, visit: u64, structure: u64) -> bool {
    ready(shell, serial, visit).is_some()
        && entry(shell).is_some_and(|e| e.filter_choice == structure)
}
