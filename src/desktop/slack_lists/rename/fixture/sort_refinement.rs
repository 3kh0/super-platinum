//! Four exact Sort fixtures use local Apply and the existing shared Save worker.
use super::*;
mod forms;
mod observer;
mod preference;
#[cfg(test)]
mod tests;
pub(crate) use observer::*;
pub(in crate::slack_lists::rename) use preference::{read_preference, write_preference};
pub(crate) fn seed(shell: &ShellState, server: Value) {
    assert!(sort_check_enabled() && enabled(shell));
    let records = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        board_records: Some(records),
        refinement_preferences: Some(forms::preferences(false)),
        ..Default::default()
    };
}
fn require(shell: &ShellState) -> Result<(), String> {
    if sort_check_enabled() && enabled(shell) {
        Ok(())
    } else {
        Err("Exact disconnected Sort refinement checker required".into())
    }
}
fn index(l: &Ledger) -> usize {
    l.requests
        .iter()
        .filter(|r| r["path"] == "/api/lists.views.update")
        .count()
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let l = ledger().lock().unwrap();
    Ok(
        json!({"requests":l.requests,"responses":l.responses,"finished":l.finished,
        "server":l.server,"population":l.board_records,"preferences":l.refinement_preferences,
        "events":l.refinement_events,"writePending":l.pending.is_some(),
        "readPending":l.held_read.is_some(),"preferencePending":l.refinement_child_pending.is_some()}),
    )
}
pub(crate) fn release_preference(shell: &ShellState) -> Result<(), String> {
    require(shell)?;
    ledger()
        .lock()
        .unwrap()
        .refinement_child_pending
        .take()
        .ok_or("No held Sort preference")?
        .send(())
        .map_err(|_| "Sort preference receiver retired".into())
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::{NotRequested, Unconfirmed};
    require(&state.peek()).map_err(|e| NotRequested(Error::Api(e)))?;
    if !guard.current(&state.peek(), Stage::Saving) || registry::blocked(&guard.key.owner) {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let intent = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::Refinement(i))
                if i.view_id() == "V_SORT"
                    && matches!(
                        i.destination(),
                        super_platinum_core::slack_lists::ViewRefinementDestination::Existing
                    ) =>
            {
                Some(i.clone())
            }
            _ => None,
        })
    })
    .ok_or(NotRequested(Error::TransportNotConfigured))?;
    let n = {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.refinement_child_pending.is_some()
            || l.server.as_ref() != Some(&intent.original().raw)
        {
            return Err(NotRequested(Error::Api(
                "sort_refinement_original_or_pending".into(),
            )));
        }
        index(&l)
    };
    let desired = forms::sorts(n, ordered_sort_check_enabled())
        .ok_or(NotRequested(Error::Api("sort_refinement_step".into())))?;
    let request = permit.request(&state.peek().core.client, &session());
    forms::metadata(&request, &desired).map_err(NotRequested)?;
    record(request).map_err(NotRequested)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let reply = {
        let mut l = ledger().lock().unwrap();
        let saved = forms::saved(l.server.as_ref().unwrap(), &desired).map_err(NotRequested)?;
        let reply = if n == 0 {
            json!({"syntheticFailure":"response_lost"})
        } else {
            json!({"ok":true,"list":saved,"opaque_reply":{"sort_refinement":true}})
        };
        l.responses.push(reply.clone());
        l.pending = Some(tx);
        l.refinement_events
            .push(json!({"kind":"metadata-dispatched","serial":guard.serial,"step":n}));
        reply
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("sort_refinement_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("sort_refinement_closed".into())))?;
    let mut l = ledger().lock().unwrap();
    l.server = Some(forms::saved(l.server.as_ref().unwrap(), &desired).map_err(Unconfirmed)?);
    l.refinement_events
        .push(json!({"kind":"metadata-response-released","serial":guard.serial,"step":n}));
    if n == 0 {
        Err(Unconfirmed(Error::Transport(
            "sort_refinement_response_lost".into(),
        )))
    } else {
        Ok(reply)
    }
}
