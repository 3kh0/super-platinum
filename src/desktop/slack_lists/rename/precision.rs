//! Number display input in the existing bounded schema-operation registry.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub field: String,
    pub original: u8,
    pub desired: u8,
    pub current: Option<u8>,
}

/// Presentation gate only; the consumed core permit reparses full raw evidence.
pub(super) fn current(document: &Document, column: &str) -> Option<u8> {
    let column = document.column(column)?;
    let value = column.raw.get("options")?.get("precision")?.as_u64()?;
    (column.kind == "number"
        && value <= 5
        && column.raw.get("options") == Some(&json!({"precision":value})))
    .then_some(value as u8)
}
pub(crate) fn supports(shell: &ShellState, column: &str) -> bool {
    shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .is_some_and(|l| current(&l.document, column).is_some())
}
pub(crate) fn can_open(shell: &ShellState, column: &str) -> bool {
    can_target(shell, &Target::Precision(column.into()))
}
pub(crate) fn open(mut state: Signal<ShellState>, column: String, visit: u64) {
    open_shell(&mut state.write(), &column, visit);
}
pub(crate) fn open_shell(shell: &mut ShellState, column: &str, visit: u64) -> bool {
    open_target(shell, Target::Precision(column.into()), visit)
}
pub(crate) fn input(mut state: Signal<ShellState>, serial: u64, visit: u64, value: u8) {
    let changed = {
        let shell = state.peek();
        set_input(&shell, serial, visit, value)
    };
    if changed {
        state.write();
    }
}
pub(super) fn set_input(shell: &ShellState, serial: u64, visit: u64, value: u8) -> bool {
    if value > 5 || generation(shell) != Some(visit) || !owner::available(shell) {
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
                && e.target.is_precision()
                && matches!(e.input, Input::Precision(_))
        }) else {
            return false;
        };
        if !shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| l.document == e.original)
        {
            return false;
        }
        e.input = Input::Precision(value);
        true
    })
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    let Target::Precision(column) = &e.target else {
        return None;
    };
    let Input::Precision(desired) = e.input else {
        return None;
    };
    Some(Presentation {
        field: e.before.clone(),
        original: current(&e.original, column)?,
        desired,
        current: shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .and_then(|l| current(&l.document, column)),
    })
}

#[cfg(test)]
pub(crate) fn discard_fixture(shell: &mut ShellState) {
    if let Some(key) = Key::current(shell) {
        with_registry(|r| {
            if r.entries
                .get(&key)
                .is_some_and(|e| e.target.is_precision() && e.stage == Stage::Editing)
            {
                r.entries.remove(&key);
            }
        });
    }
}
