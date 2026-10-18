//! Existing hosted attachments enter the same configured Editor and Send button.
use super::send::Captured;
use super::*;
use crate::state::ComposerTarget;
use dioxus::prelude::*;

pub(crate) fn snapshot(
    shell: &ShellState,
    target: ComposerTarget,
) -> Option<owned_hosted::Snapshot> {
    if target != ComposerTarget::Channel {
        return None;
    }
    let key = shell.composer_key(target)?;
    let root = root().lock().ok()?;
    let account = root
        .configured_account(shell.core.active_account.as_deref()?)
        .ok()?;
    if let Some(installed) = root.host().installation(&account, &key) {
        if let Ok(snapshot) = installed.editor().hosted().snapshot() {
            if snapshot.loaded || snapshot.retry_load {
                return Some(snapshot);
            }
        }
    }
    let inventory = root.host().discovery().hosted(shell)?;
    let draft = inventory.drafts().iter().find(|d| d.scope().key == key);
    let claim = inventory.claims().iter().find(|r| r.scope().key == key);
    let completed: Vec<_> = inventory
        .completed()
        .iter()
        .filter(|r| r.record().scope().key == key)
        .cloned()
        .collect();
    let input = draft
        .map(|d| d.input())
        .or_else(|| claim.map(|r| r.input()))
        .or_else(|| completed.first().map(|r| r.record().input()))?;
    Some(owned_hosted::Snapshot {
        selection: owned_hosted::Selection::from_input(input),
        version: 0,
        saved: true,
        loaded: false,
        retry_load: false,
        retry_saved: false,
        retry_result: false,
        sending: claim.is_some() || !completed.is_empty(),
        can_retire: claim.is_none(),
        completed,
        retry_retirement: false,
    })
}
pub(crate) fn present(shell: &ShellState, target: ComposerTarget) -> bool {
    snapshot(shell, target)
        .is_some_and(|s| !s.selection.sources().is_empty() || s.sending || !s.saved)
}

pub(crate) fn retry_storage(state: Signal<ShellState>, returned: bool) {
    retry(
        state,
        if returned {
            Retry::Returned
        } else {
            Retry::Saved
        },
    );
}
pub(crate) fn retry_load(state: Signal<ShellState>) {
    retry(state, Retry::Load);
}
pub(crate) fn retry_retirement(state: Signal<ShellState>) {
    retry(state, Retry::Retirement);
}
enum Retry {
    Retirement,
    Load,
    Saved,
    Returned,
}
fn retry(mut state: Signal<ShellState>, kind: Retry) {
    let editor = (|| {
        let shell = state.peek();
        let key = shell.composer_key(ComposerTarget::Channel)?;
        let root = root().lock().ok()?;
        let account = root
            .configured_account(shell.core.active_account.as_deref()?)
            .ok()?;
        root.host()
            .installation(&account, &key)
            .map(|v| (v.editor().clone(), account, shell.core.account_epoch, key))
    })();
    let Some((editor, account, epoch, key)) = editor else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let hosted = editor.hosted();
        let result = match kind {
            Retry::Retirement => hosted.retry_retirement().await,
            Retry::Returned => hosted
                .retry_returned_storage()
                .await
                .and_then(|_| hosted.acknowledge_send()),
            Retry::Saved => hosted.retry_saved().await.map(|_| ()),
            Retry::Load => hosted.retry_load(&editor).await.map(|_| ()),
        };
        let mut shell = state.write();
        if shell.core.active_account.as_deref() != Some(account.id())
            || shell.core.account_epoch != epoch
            || shell.composer_key(ComposerTarget::Channel).as_ref() != Some(&key)
        {
            return;
        }
        if result.is_err() {
            shell.show_toast(
                "The saved file share still needs attention. Its original content is retained.",
            );
        }
        shell.refresh_from_core();
    });
}

pub(super) fn start(state: Signal<ShellState>, captured: Captured) {
    dioxus::core::spawn_forever(async move {
        let result = run(state, &captured).await;
        let mut state = state;
        let mut shell = state.write();
        if shell.core.active_account.as_deref() == Some(captured.account.id())
            && shell.core.account_epoch == captured.epoch
        {
            shell
                .draft_writes
                .finish_send(captured.account.id(), captured.epoch, &captured.key);
            if result.is_err() {
                shell.show_toast("Your files and message are retained. Review their saved delivery status before trying again.");
            }
        }
    });
}
async fn run(mut state: Signal<ShellState>, captured: &Captured) -> Result<(), durable::Error> {
    let (original, editor) = send::prepare_source(state, captured).await?;
    let before = editor.flush().await?;
    if !captured.unchanged(&state.peek()) || !same_local(before.view.local(), &captured.local) {
        return Err(durable::Error::Stale);
    }
    captured
        .source()?
        .prepare_basis(
            state,
            editor.owner.clone(),
            before.view.inventory().managed()?.clone(),
        )
        .await?;
    editor.refresh_receipt().await?;
    let hosted = editor.hosted();
    hosted.load(&editor).await?;
    let stream = captured.source()?.stream(&state.peek())?;
    let saved = hosted.persist(&editor, stream.clone()).await?;
    if !captured.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let capture = editor.capture(Visit::new())?;
    let input = saved.selection.input(&capture.review)?;
    let observed = crate::file_share::composer::observe(state, &input).await?;
    if !captured.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let prepared = hosted.begin_send(&editor, stream, saved.version)?;
    if prepared.input() != &input {
        return Err(durable::Error::Stale);
    }
    let completion = hosted.claim_send().await?;
    let admission = original.lock_preserving(&editor, prepared.serial()).await?;
    let fresh = observed.fresh(&state.peek(), prepared.clone())?;
    // This closure is run inside the credential adapter immediately before core
    // arms the actual owned claim. No independent check can replace another.
    let attempt = captured
        .binding
        .arm_hosted(&completion, fresh, || {
            let shell = state.peek();
            captured.unchanged(&shell)
                && admission.current(&shell)
                && observed.current(&shell)
                && hosted.send_current(&editor, &prepared)
        })
        .map_err(|_| durable::Error::Stale)?;
    let original = admission.release();
    let execution = attempt.execute().await;
    // Even a storage enqueue failure retains the actual HTTP result. Never
    // condition original settlement on the user's current account or visit.
    if completion.returned().is_none() {
        return Err(durable::Error::ReviewRequired);
    }
    if execution.is_err() {
        return Err(durable::Error::Storage);
    }
    hosted.settle_send().await?;
    let receipt_id = hosted.completed_for(&prepared)?;
    let paint = async {
        if !captured.unchanged(&state.peek()) {
            return Err(durable::Error::Stale);
        }
        hosted.clear_basis(&editor)?;
        let admission = original.lock_preserving(&editor, prepared.serial()).await?;
        if !captured.unchanged(&state.peek()) {
            return Err(durable::Error::Stale);
        }
        admission.clear_hosted(&editor).await?;
        let mut shell = state.write();
        if !captured.unchanged(&shell) {
            return Err(durable::Error::Stale);
        }
        shell.core.composer = Default::default();
        shell.core.drafts.items.remove(&captured.key);
        shell.refresh_from_core();
        Ok(())
    }
    .await;
    // Acceptance storage retirement is separate from renderer success. A later
    // visit never gets a clear token reconstructed from the accepted receipt.
    hosted.acknowledge_send()?;
    if paint.is_ok() {
        hosted
            .retire_reviewed(&editor, &hosted.snapshot()?, &receipt_id)
            .await?;
    }
    paint
}

/// Explicit picker Insert/remove/reorder only. Selection alone is presentation.
/// Captured local edits are retained by the existing Editor before persistence.
pub(crate) async fn edit_reviewed(
    state: Signal<ShellState>,
    expected: owned_hosted::Snapshot,
    change: impl FnOnce(&owned_hosted::Composer, u64) -> Result<u64, durable::Error>,
) -> Result<(), durable::Error> {
    edit(state, ComposerTarget::Channel, |composer, version| {
        let current = composer.snapshot()?;
        if expected.sending
            || current.sending
            || (expected.loaded && expected.version != version)
            || expected.selection != current.selection
        {
            return Err(durable::Error::Stale);
        }
        change(composer, version)
    })
    .await
}

pub(crate) async fn edit(
    state: Signal<ShellState>,
    target: ComposerTarget,
    change: impl FnOnce(&owned_hosted::Composer, u64) -> Result<u64, durable::Error>,
) -> Result<(), durable::Error> {
    if target != ComposerTarget::Channel {
        return Err(durable::Error::Unsupported);
    }
    let (account, key, local, epoch, view) = {
        let shell = state.peek();
        let key = shell.composer_key(target).ok_or(durable::Error::Stale)?;
        let local = send::local(&shell, target);
        if !local.attachments.is_empty() || shell.draft_writes.sending.contains(&key) {
            return Err(durable::Error::Pending);
        }
        (
            account(
                shell
                    .core
                    .active_account
                    .as_deref()
                    .ok_or(durable::Error::Stale)?,
            )?,
            key,
            local,
            shell.core.account_epoch,
            shell.draft_writes.view_id,
        )
    };
    let current = |shell: &ShellState| {
        shell.core.active_account.as_deref() == Some(account.id())
            && shell.core.account_epoch == epoch
            && shell.draft_writes.view_id == view
            && shell.composer_key(target).as_ref() == Some(&key)
            && same_local(&send::local(shell, target), &local)
    };
    let editor = enroll::editor_hosted(state, target, account.clone(), &key, &local).await?;
    let hosted = editor.hosted();
    let loaded = hosted.load(&editor).await?;
    if !current(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    change(&hosted, loaded.version)?;
    let source = socket::capture(&state.peek(), &key.team)?
        .ensure(state)
        .await?;
    if !current(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let before = editor.flush().await?;
    source
        .prepare_basis(
            state,
            editor.owner.clone(),
            before.view.inventory().managed()?.clone(),
        )
        .await?;
    editor.refresh_receipt().await?;
    if !current(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    hosted
        .persist(&editor, source.stream(&state.peek())?)
        .await?;
    Ok(())
}

/// Only local accepted-history/attachment retirement. Current text is untouched.
pub(crate) fn review_owner(shell: &ShellState) -> String {
    format!(
        "{:?}:{}:{}:{}:{:?}",
        shell.core.active_account,
        shell.core.account_epoch,
        shell.draft_writes.view_id,
        shell.channel_generation,
        shell.composer_key(ComposerTarget::Channel)
    )
}
pub(crate) fn retire_completed(
    mut state: Signal<ShellState>,
    expected: owned_hosted::Snapshot,
    id: String,
    owner: String,
) {
    if review_owner(&state.peek()) != owner {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = async {
            let (account, key, local) = {
                let shell = state.peek();
                if review_owner(&shell) != owner {
                    return Err(durable::Error::Stale);
                }
                (
                    account(
                        shell
                            .core
                            .active_account
                            .as_deref()
                            .ok_or(durable::Error::Stale)?,
                    )?,
                    shell
                        .composer_key(ComposerTarget::Channel)
                        .ok_or(durable::Error::Stale)?,
                    send::local(&shell, ComposerTarget::Channel),
                )
            };
            let editor =
                enroll::editor_hosted(state, ComposerTarget::Channel, account, &key, &local)
                    .await?;
            let hosted = editor.hosted();
            hosted.load(&editor).await?;
            if review_owner(&state.peek()) != owner {
                return Err(durable::Error::Stale);
            }
            hosted.retire_reviewed(&editor, &expected, &id).await
        }
        .await;
        if review_owner(&state.peek()) == owner {
            let mut shell = state.write();
            if result.is_err() {
                shell.show_toast("The saved share record is retained. Review its current status or retry saving this removal.");
            }
            shell.refresh_from_core();
        }
    });
}
