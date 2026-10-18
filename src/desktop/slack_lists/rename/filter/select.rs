//! Ordered option toggles retain stable input nodes while rotating callback epochs.
use super::*;
pub(crate) fn update_select(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    row: u64,
    epoch: u64,
    id: &str,
    checked: bool,
) -> bool {
    let Some(key) = rows::ready(shell, serial, visit) else {
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
        let mut all = input.conditions().to_vec();
        let Some(ViewFilterChoice::Select {
            column_id,
            operator,
            values,
        }) = all.get_mut(i)
        else {
            return false;
        };
        if !operator.requires_values()
            || values.iter().any(|v| v == id) == checked
            || !e
                .original
                .column(column_id)
                .and_then(ViewFilter::select_choices)
                .is_some_and(|choices| choices.iter().any(|(known, _)| known == id))
        {
            return false;
        }
        if checked {
            values.push(id.to_owned());
        } else {
            values.retain(|v| v != id);
        }
        e.input = Input::Filter(ViewFilterChoice::from_conditions(all));
        e.filter_rows[i].epoch = next_id();
        e.filter_choice = next_id();
        true
    })
}
