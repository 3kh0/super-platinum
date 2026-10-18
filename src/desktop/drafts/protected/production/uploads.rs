//! Existing upload/share lifecycle after the selected owned Local lane flush.
//! No text Consume conversion, fallback owner, replay, or alternate HTTP path.
use super::send::Captured;
use super::*;
use crate::state::ComposerTarget;
use dioxus::prelude::*;

pub(super) fn start(mut state: Signal<ShellState>, captured: Captured) {
    dioxus::core::spawn_forever(async move {
        let result = prepare(state, &captured).await;
        if result.is_ok() {
            // This existing function synchronously takes the exact current
            // caption/files before spawning its retained upload future. Its
            // reserve/finalize/cancel/load journal uses this same Account.
            crate::messaging::start_file_upload(state, captured.target == ComposerTarget::Thread)
                .await;
        }
        let mut shell = state.write();
        if shell.core.active_account.as_deref() == Some(captured.account.id())
            && shell.core.account_epoch == captured.epoch
        {
            shell
                .draft_writes
                .finish_send(captured.account.id(), captured.epoch, &captured.key);
            if result.is_err() {
                shell.show_toast("Your files and caption are retained. Their saved draft needs review before uploading.");
            }
        }
    });
}

async fn prepare(state: Signal<ShellState>, c: &Captured) -> Result<(), durable::Error> {
    if c.local.attachments.is_empty()
        || c.local.attachments.iter().any(|a| a.uploading)
        || !c.unchanged(&state.peek())
    {
        return Err(durable::Error::Stale);
    }
    let opened = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let mut root = root().lock().map_err(|_| durable::Error::Storage)?;
        let host = root.host_mut();
        host.select(&shell, c.account.clone())?;
        let route = host.route_snapshot(&shell, &c.key)?;
        if route.kind != "local" {
            // A managed collision/alternate is not permission to bypass the
            // protected editor by opening another LocalWriter.
            return Err(durable::Error::ReviewRequired);
        }
        host.retained_locals(&c.account)
            .into_iter()
            .find(|v| v.id() == route.identity)
            .ok_or(durable::Error::Stale)?
    };
    opened.ready().await;
    let lane = root()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .host()
        .local(&opened)?;
    let snapshot = lane.writer().flush().await?;
    if !ready(&snapshot, &c.local) || !c.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
    let current = root()
        .lock()
        .map_err(|_| durable::Error::Storage)?
        .host()
        .route_snapshot(&shell, &c.key)?;
    if current.kind != "local"
        || current.identity != opened.id()
        || current.serial != Some(snapshot.serial)
        || !c.unchanged(&shell)
    {
        return Err(durable::Error::Stale);
    }
    Ok(())
}

fn ready(snapshot: &local_writer::LocalSnapshot, expected: &LocalDraft) -> bool {
    !snapshot.running
        && snapshot.pending.is_none()
        && snapshot.error.is_none()
        && snapshot.failed_inputs.is_empty()
        && snapshot
            .inventory
            .as_ref()
            .is_some_and(|v| same_local(v.local(), expected))
}
