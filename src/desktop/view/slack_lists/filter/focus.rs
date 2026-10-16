use super::*;
use std::time::Duration;
pub(super) fn action(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    structure: u64,
    action: filter::Action,
    clear: Option<Signal<String>>,
) {
    spawn(async move {
        let proof = serde_json::json!({"serial":serial.to_string(),"visit":visit.to_string(),"structure":structure.to_string()});
        let mut eval = dioxus::document::eval(&format!(
            "{};\nawait compoundFilterFocus({proof});",
            include_str!("focus.js")
        ));
        if !matches!(
            tokio::time::timeout(Duration::from_secs(1), eval.recv::<bool>()).await,
            Ok(Ok(true))
        ) {
            return;
        }
        let result = filter::structural(&state.peek(), serial, visit, structure, action);
        if result.is_some() {
            if let Some(mut clear) = clear {
                clear.set(String::new());
            }
            state.write();
        }
        let value = result
            .map(|(structure, id)| serde_json::json!({"structure":structure.to_string(),"id":id}));
        let _ = eval.send(value);
    });
}
