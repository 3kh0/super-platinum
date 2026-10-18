//! Cancellation stops future dispatch, waits for the bounded original request,
//! and keeps its exact ticket. No delete occurs concurrently with upload bytes.
use super::*;

pub(super) fn merge_intent(e: &Entry, record: &mut protocol::custody::Record) -> Result<(), ()> {
    if let Some(cancelled) = &e.cancel_requested {
        if record.pending().is_some_and(|p| p.id() == cancelled) {
            record.request_cancel().map_err(|_| ())?;
        } else if e
            .retained
            .as_ref()
            .and_then(|r| r.pending())
            .is_some_and(|p| p.id() == cancelled)
        {
            // A completion snapshot made before Cancel cannot erase its intent.
            return Err(());
        }
    }
    Ok(())
}

pub(super) fn requested(shell: &ShellState, r: &Receipt) -> bool {
    shell
        .profile_edit
        .recording
        .entries
        .get(&r.token.owner)
        .is_some_and(|e| {
            e.cancel_requested.is_some()
                || e.retained.as_ref().is_some_and(|v| v.cancel_requested())
        })
}
pub(super) fn request_shell(shell: &mut ShellState, visit: u64, serial: u64) -> Option<Receipt> {
    if !presentation(shell)
        .is_some_and(|v| v.visit == visit && v.serial == serial && v.can_cancel_upload)
    {
        return None;
    }
    let owner = shell
        .profile_edit
        .recording
        .pane
        .as_ref()?
        .token
        .owner
        .clone();
    let e = shell.profile_edit.recording.entries.get_mut(&owner)?;
    let r = e.receipt.clone()?;
    e.cancel_requested = Some(e.retained.as_ref()?.pending()?.id().into());
    e.message = Some(
        "Canceling upload. Your local recording is kept while the current request finishes.".into(),
    );
    Some(r)
}
pub(crate) fn request(mut state: Signal<ShellState>, visit: u64, serial: u64) {
    if state.peek().profile_edit.recording.fixture.is_some() {
        fixture::cancel_upload_shell(&mut state.write(), visit, serial);
        return;
    }
    let r = { request_shell(&mut state.write(), visit, serial) };
    if let Some(r) = r {
        dioxus::core::spawn_forever(async move {
            retain_intent(state, &r).await;
        });
    }
}
async fn retain_intent(mut state: Signal<ShellState>, r: &Receipt) -> bool {
    let gate = storage::io_gate(state, &r.token);
    let _guard = gate.lock().await;
    let Ok(mut record) = storage::snapshot(&state.peek(), &r.token) else {
        return false;
    };
    let matches = state
        .peek()
        .profile_edit
        .recording
        .entries
        .get(&r.token.owner)
        .and_then(|e| e.cancel_requested.as_deref().map(str::to_owned))
        .is_some_and(|id| record.pending().is_some_and(|p| p.id() == id));
    if !matches {
        return false;
    }
    if record.request_cancel().is_err() {
        return false;
    }
    storage::persist_locked(state, &r.token, record).await
}
pub(super) async fn settled(mut state: Signal<ShellState>, r: &Receipt) {
    if !requested(&state.peek(), r) {
        return;
    }
    if !retain_intent(state, r).await {
        return;
    }
    let gate = storage::io_gate(state, &r.token);
    let _guard = gate.lock().await;
    let Ok(mut record) = storage::snapshot(&state.peek(), &r.token) else {
        return;
    };
    if state
        .peek()
        .profile_edit
        .recording
        .entries
        .get(&r.token.owner)
        .is_some_and(|e| e.inflight == Some(r.id) && !e.upload_started)
    {
        if record.cancel_before_dispatch().is_err() {
            return;
        }
        if let Some(e) = entry_mut(&mut state.write(), r) {
            e.cancel_requested = None;
        }
        storage::persist_locked(state, &r.token, record).await;
    }
}
pub(super) fn cleanup_if_ready(state: Signal<ShellState>, r: &Receipt) {
    if current(&state.peek(), &r.token, r.visit, r.serial)
        && requested(&state.peek(), r)
        && presentation(&state.peek()).is_some_and(|v| v.can_cleanup)
    {
        cleanup(state, r.visit, r.serial);
    }
}
