use super::*;
pub(super) fn action(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    structure: u64,
    rendered: ViewSortChoice,
    action: sort::Action,
    clear: Option<Signal<String>>,
) {
    // Registry-only synchronous reservation: do not render-disable the source
    // control before JavaScript captures its exact pre-action focus.
    let Some(pending) = sort::reserve(&state.peek(), serial, visit, structure, &rendered) else {
        return;
    };
    spawn(async move {
        let proof = serde_json::json!({"serial":serial.to_string(),"visit":visit.to_string(),"structure":structure.to_string(),"token":pending.token().to_string()});
        let mut eval = dioxus::document::eval(&format!(
            "{};\nawait orderedSortFocus({proof});",
            include_str!("focus.js")
        ));
        if !matches!(
            tokio::time::timeout(pending.remaining(), eval.recv::<bool>()).await,
            Ok(Ok(true))
        ) {
            if pending.release() {
                state.write();
            }
            return;
        }
        let result = pending.commit(&state.peek(), action);
        pending.release();
        if result.is_some() {
            if let Some(mut clear) = clear {
                clear.set(String::new());
            }
        }
        // Also refresh a refusal that consumed its reservation, so an earlier
        // blocked Save cannot leave stale disabled presentation behind.
        state.write();
        let value = result
            .map(|(structure, id)| serde_json::json!({"structure":structure.to_string(),"token":pending.token().to_string(),"id":id}));
        let _ = eval.send(value);
    });
}
