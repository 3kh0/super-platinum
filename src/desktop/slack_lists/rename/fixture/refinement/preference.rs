//! Independently held child uses the same ledger; no replacement executor.
use super::*;
pub(in crate::slack_lists::rename) async fn read_preference(
    state: Signal<ShellState>,
    request: PreparedRequest,
    initial: bool,
    current: impl Fn() -> bool,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::NotRequested;
    if sort_check_enabled() {
        return sort_refinement::read_preference(state, request, initial, current).await;
    }
    if !check_enabled() || !enabled(&state.peek()) || !current() {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    forms::preference_read(&request).map_err(NotRequested)?;
    {
        let l = ledger().lock().unwrap();
        if (initial && (l.requests.len() != 3 || l.responses.len() != 3))
            || (!initial && l.refinement_child_pending.is_some())
        {
            return Err(NotRequested(Error::Api(
                "fixture_preference_read_order".into(),
            )));
        }
    }
    record(request).map_err(NotRequested)?;
    let raw = {
        let mut l = ledger().lock().unwrap();
        let raw = l
            .refinement_preferences
            .clone()
            .ok_or(NotRequested(Error::TransportNotConfigured))?;
        l.responses.push(raw.clone());
        l.refinement_events.push(
            json!({"kind":if initial{"preference-preflight"}else{"preference-observed-read"}}),
        );
        raw
    };
    Ok(raw)
}
pub(in crate::slack_lists::rename) async fn write_preference(
    state: Signal<ShellState>,
    guard: &Guard,
    request: PreparedRequest,
    current: impl Fn() -> bool,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::{NotRequested, Unconfirmed};
    if sort_check_enabled() {
        return sort_refinement::write_preference(state, guard, request, current).await;
    }
    if !check_enabled() || !enabled(&state.peek()) || !current() {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let mode = case().ok_or(NotRequested(Error::TransportNotConfigured))?;
    if mode == Case::Lost {
        return Err(NotRequested(Error::Api(
            "fixture_lost_ack_cannot_start_child".into(),
        )));
    }
    forms::preference_write(&request, mode).map_err(NotRequested)?;
    // Observe the actual worker publication before recording its HTTP dispatch.
    let witness = {
        let shell = state.peek();
        let v = shell
            .slack_lists
            .view
            .as_ref()
            .ok_or(NotRequested(Error::TransportNotConfigured))?;
        let receipt = with_registry(|r| {
            r.entries
                .get(&guard.key)
                .and_then(|e| e.refinement_receipt.clone())
        })
        .ok_or(NotRequested(Error::TransportNotConfigured))?;
        if v.layout.selected.as_deref() != Some(mode.view_id())
            || v.layout.board_order.is_none()
            || v.layout.refinements.is_empty()
            || receipt.dismissed_on_ack
            || (mode.created() && v.layout.pending_refinement.as_deref() != Some(mode.view_id()))
        {
            return Err(NotRequested(Error::Api(
                "fixture_preference_source_timing".into(),
            )));
        }
        json!({"kind":"preference-dispatched","serial":guard.serial,"selected":v.layout.selected,"pending":v.layout.pending_refinement,
            "provider":v.layout.board_order.as_ref().map(|p|p.typed_order()),"localDirty":!v.layout.refinements.is_empty(),"dismissed":receipt.dismissed_on_ack,
            "document":v.loaded.as_ref().map(|l|&l.document.raw),"population":v.loaded.as_ref().map(|l|&l.records.raw)})
    };
    {
        let l = ledger().lock().unwrap();
        if l.requests.len() != 5
            || l.responses.len() != 5
            || l.pending.is_some()
            || l.refinement_child_pending.is_some()
        {
            return Err(NotRequested(Error::Api(
                "fixture_child_dispatch_order".into(),
            )));
        }
    }
    record(request).map_err(NotRequested)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        l.refinement_child_pending = Some(tx);
        l.responses
            .push(json!({"syntheticFailure":"preference_response_lost"}));
        l.refinement_events.push(witness);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("fixture_child_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("fixture_child_closed".into())))?;
    let mut l = ledger().lock().unwrap();
    l.refinement_preferences = Some(
        json!({"ok":true,"prefs":{"lists_default_view":forms::preferences(mode).to_string()}}),
    );
    l.refinement_events
        .push(json!({"kind":"preference-response-released","serial":guard.serial}));
    Err(Unconfirmed(Error::Transport(
        "fixture_preference_response_lost".into(),
    )))
}
