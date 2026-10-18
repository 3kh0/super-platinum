//! Exact-fixture passive witness of the real Lane tail-drop callback.
use super::*;
use std::sync::{Mutex, OnceLock};

fn events() -> &'static Mutex<Vec<Value>> {
    static EVENTS: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
    EVENTS.get_or_init(|| Mutex::new(Vec::new()))
}
pub(super) fn reset() {
    events().lock().unwrap().clear();
}
pub(crate) fn tail_drop_observed(
    shell: &ShellState,
    destination: &str,
    allows_tail: bool,
    captured: Option<&crate::slack_lists::rename::card_order::Drag>,
    admitted: bool,
) {
    // Ordinary production events never allocate or alter a checker trace.
    if !check_enabled() || case() != Some(Case::Tail) || !enabled(shell) {
        return;
    }
    let mut trace = events().lock().unwrap();
    let ordinal = trace.len();
    trace.push(
        json!({"ordinal":ordinal,"allowsTail":allows_tail,"captured":captured.is_some(),
        "source":captured.map(|d|d.source.as_str()),"record":captured.map(|d|d.record.as_str()),"destination":destination,
        "admitted":admitted,"visit":generation(shell),"selection":selection(shell),
        "view":selected_view(shell)}),
    );
}
pub(crate) fn tail_callbacks(shell: &ShellState) -> Result<Value, String> {
    if !check_enabled() || !enabled(shell) {
        return Err("Exact disconnected card checker required".into());
    }
    Ok(json!(events().lock().unwrap().clone()))
}
