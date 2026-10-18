//! Independent child held separately; parent settlement is never manufactured.
use super::*;
pub(in crate::slack_lists::rename) async fn read_preference(
    state: Signal<ShellState>,
    request: PreparedRequest,
    initial: bool,
    current: impl Fn() -> bool,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::NotRequested;
    require(&state.peek()).map_err(|e| NotRequested(Error::Api(e)))?;
    if !current() {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    forms::preference_read(&request).map_err(NotRequested)?;
    {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.refinement_child_pending.is_some()
            || initial
                && l.requests.last().is_none_or(|r| {
                    r["fields"][1] != json!(["prefs", "task_lists,assigned_to_list"])
                })
            || !initial
                && l.requests
                    .last()
                    .is_none_or(|r| r["path"] != "/api/users.prefs.set")
        {
            return Err(NotRequested(Error::Api(
                "sort_preference_read_order".into(),
            )));
        }
    }
    record(request).map_err(NotRequested)?;
    let mut l = ledger().lock().unwrap();
    let raw = l
        .refinement_preferences
        .clone()
        .ok_or(NotRequested(Error::TransportNotConfigured))?;
    l.responses.push(raw.clone());
    l.refinement_events
        .push(json!({"kind":if initial{"preference-preflight"}else{"preference-observed-read"}}));
    Ok(raw)
}
pub(in crate::slack_lists::rename) async fn write_preference(
    state: Signal<ShellState>,
    guard: &Guard,
    request: PreparedRequest,
    current: impl Fn() -> bool,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::{NotRequested, Unconfirmed};
    require(&state.peek()).map_err(|e| NotRequested(Error::Api(e)))?;
    if !current() {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    forms::preference_write(&request).map_err(NotRequested)?;
    let witness = {
        let shell = state.peek();
        let v = shell
            .slack_lists
            .view
            .as_ref()
            .ok_or(NotRequested(Error::TransportNotConfigured))?;
        if v.layout.selected.as_deref() != Some("V_SORT")
            || v.layout.refinements.is_empty()
            || v.layout.board_order.is_some()
        {
            return Err(NotRequested(Error::Api("sort_preference_custody".into())));
        }
        json!({"kind":"preference-dispatched","serial":guard.serial,"selected":v.layout.selected,"localDirty":true,"document":v.loaded.as_ref().map(|l|&l.document.raw),"population":v.loaded.as_ref().map(|l|&l.records.raw)})
    };
    {
        let l = ledger().lock().unwrap();
        if index(&l) < 2
            || l.pending.is_some()
            || l.held_read.is_some()
            || l.refinement_child_pending.is_some()
            || l.requests
                .last()
                .is_none_or(|r| r["path"] != "/api/lists.views.update")
        {
            return Err(NotRequested(Error::Api(
                "sort_preference_dispatch_order".into(),
            )));
        }
    }
    record(request).map_err(NotRequested)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        l.refinement_child_pending = Some(tx);
        l.responses.push(json!({"ok":true}));
        l.refinement_events.push(witness);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("sort_child_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("sort_child_closed".into())))?;
    let mut l = ledger().lock().unwrap();
    l.refinement_preferences = Some(forms::preferences(true));
    l.refinement_events
        .push(json!({"kind":"preference-response-released","serial":guard.serial}));
    Ok(json!({"ok":true}))
}
