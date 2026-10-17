//! Revision-owned creation configuration; uses the shared admission and readback.
use super::*;
use super_platinum_core::slack_lists::NumberCreatePrecision;
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: String,
) {
    if choose_shell(&state.peek(), serial, visit, selection, rendered, &value) {
        state.write();
    }
}
pub(crate) fn choose_shell(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: &str,
) -> bool {
    if rendered.kind != ColumnCreateKind::NumberDefault0 {
        return false;
    }
    let precision = match value {
        "0" => 0,
        "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        "5" => 5,
        _ => return false,
    };
    let precision = NumberCreatePrecision::capture(precision).unwrap();
    with_edit(shell, serial, visit, selection, rendered, |entry| {
        if entry.field_choice.number_precision == precision {
            return false;
        }
        entry.field_choice.number_precision = precision;
        entry.field_choice.revision = next_id();
        true
    })
}

/// Test teardown uses the actual Cancel boundary for this exact Number fixture.
#[cfg(test)]
pub(crate) fn close_fixture(shell: &ShellState) -> bool {
    let Some(e) = super::entry(shell).filter(|e| {
        e.target == Target::NewTextField("V_FILTER".into())
            && e.stage == Stage::Editing
            && e.field_choice.kind == ColumnCreateKind::NumberDefault0
            && e.job.is_none()
            && e.intent.is_none()
    }) else {
        return false;
    };
    super::super::close_entry(shell, e.serial)
}
