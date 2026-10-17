//! Stable row keys and whole-vector callback epochs; no index-based event authority.
use super::*;
pub(in crate::slack_lists::rename) fn rows_for(input: &Input) -> Vec<u64> {
    let Input::Sort(value) = input else {
        return vec![];
    };
    value.rules().iter().map(|_| next_id()).collect()
}
#[derive(Clone)]
pub(crate) enum Action {
    Add(String),
    Remove(u64),
    Move(u64, bool),
    Field(u64, String),
    Direction(u64, bool),
    Clear,
}
pub(crate) fn structural(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
    action: Action,
) -> Option<(u64, String)> {
    apply(shell, serial, visit, choice, rendered, None, action)
}
pub(super) fn apply(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    choice: u64,
    rendered: &ViewSortChoice,
    pending: Option<u64>,
    action: Action,
) -> Option<(u64, String)> {
    let mut result = None;
    edit_pending(shell, serial, visit, choice, rendered, pending, |e| {
        e.sort_pending = None;
        let mut values = rendered.rules().to_vec();
        let mut ids = e.sort_rows.clone();
        if values.len() != ids.len() {
            return false;
        }
        let target = match action {
            Action::Add(id) => {
                if values
                    .iter()
                    .any(|v| v.field().is_some_and(|(key, _)| key == id))
                {
                    return false;
                }
                let Some(value) = e
                    .original
                    .column(&id)
                    .and_then(|c| ViewSort::choice(c, true))
                else {
                    return false;
                };
                let row = next_id();
                values.push(value);
                ids.push(row);
                format!("sort-direction-{row}")
            }
            Action::Remove(row) => {
                let Some(i) = ids.iter().position(|id| *id == row) else {
                    return false;
                };
                values.remove(i);
                ids.remove(i);
                ids.get(i)
                    .or_else(|| ids.last())
                    .map(|id| format!("sort-field-{id}"))
                    .unwrap_or_else(|| "slack-list-sort-add-field".into())
            }
            Action::Move(row, up) => {
                let Some(i) = ids.iter().position(|id| *id == row) else {
                    return false;
                };
                let j = if up {
                    i.checked_sub(1)
                } else {
                    i.checked_add(1).filter(|j| *j < ids.len())
                };
                let Some(j) = j else {
                    return false;
                };
                values.swap(i, j);
                ids.swap(i, j);
                format!("sort-field-{row}")
            }
            Action::Field(row, id) => {
                let Some(i) = ids.iter().position(|id| *id == row) else {
                    return false;
                };
                let Some((old, ascending)) = values[i].field() else {
                    return false;
                };
                if old == id
                    || values
                        .iter()
                        .enumerate()
                        .any(|(j, v)| j != i && v.field().is_some_and(|(key, _)| key == id))
                {
                    return false;
                }
                let Some(value) = e
                    .original
                    .column(&id)
                    .and_then(|c| ViewSort::choice(c, ascending))
                else {
                    return false;
                };
                values[i] = value;
                format!("sort-field-{row}")
            }
            Action::Direction(row, ascending) => {
                let Some(i) = ids.iter().position(|id| *id == row) else {
                    return false;
                };
                let Some((id, old)) = values[i].field() else {
                    return false;
                };
                if old == ascending {
                    return false;
                }
                let Some(value) = e
                    .original
                    .column(id)
                    .and_then(|c| ViewSort::choice(c, ascending))
                else {
                    return false;
                };
                values[i] = value;
                format!("sort-direction-{row}")
            }
            Action::Clear => {
                if values.is_empty() {
                    return false;
                }
                values.clear();
                ids.clear();
                "slack-list-sort-add-field".into()
            }
        };
        let desired = ViewSortChoice::from_rules(values);
        if !ViewSort::valid_choice(&e.original, &desired) {
            return false;
        }
        e.input = Input::Sort(desired);
        e.sort_rows = ids;
        e.sort_choice = next_id();
        result = Some((e.sort_choice, target));
        true
    });
    result
}
