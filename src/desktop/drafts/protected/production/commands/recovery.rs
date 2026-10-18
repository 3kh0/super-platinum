//! Explicit local completion of retained acknowledged Slash operations. Scans
//! the existing Account/Root records; no new queue or remote execution path.
use super::*;

pub(crate) fn presentation(shell: &ShellState, target: ComposerTarget) -> Vec<u64> {
    let Some(id) = shell.core.active_account.as_deref() else {
        return vec![];
    };
    let Some(key) = shell.composer_key(target) else {
        return vec![];
    };
    let Ok(account) = super::super::account(id) else {
        return vec![];
    };
    let Some(editor) = super::super::root().lock().ok().and_then(|r| {
        r.host()
            .installation(&account, &key)
            .map(|i| i.editor().clone())
    }) else {
        return vec![];
    };
    account
        .retained_operations()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            p.current()
                && p.belongs(&editor)
                && matches!(&p.record().request, operation::Request::Slash { .. })
                && p.record()
                    .evidence
                    .lock()
                    .is_ok_and(|e| e.status == operation::Status::Finished)
        })
        .map(|p| p.id())
        .collect()
}
pub(crate) fn finish(mut state: Signal<ShellState>, target: ComposerTarget, id: u64) {
    let captured = (|| {
        let actions =
            dioxus::core::try_consume_context::<Rc<ActionRoot>>().ok_or(durable::Error::Stale)?;
        let mut shell = state.write();
        let account = super::super::account(
            shell
                .core
                .active_account
                .as_deref()
                .ok_or(durable::Error::Stale)?,
        )?;
        let key = shell.composer_key(target).ok_or(durable::Error::Stale)?;
        let editor = super::super::root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .host()
            .installation(&account, &key)
            .ok_or(durable::Error::Stale)?
            .editor()
            .clone();
        let parent = account
            .retained_operations()?
            .into_iter()
            .find(|p| {
                p.id() == id
                    && p.belongs(&editor)
                    && matches!(&p.record().request, operation::Request::Slash { .. })
            })
            .ok_or(durable::Error::Stale)?;
        if parent
            .record()
            .evidence
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .status
            != operation::Status::Finished
        {
            return Err(durable::Error::Pending);
        }
        let epoch = shell.core.account_epoch;
        if !shell
            .draft_writes
            .begin_send(account.id(), epoch, key.clone())
        {
            return Err(durable::Error::Pending);
        }
        Ok((actions, account, key, epoch, editor, parent))
    })();
    let Ok((actions, account, key, epoch, editor, parent)) = captured else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = async {
            let current = editor.archive_slash(&parent).await?;
            let capture = native::capture(state, target, current.local()).await?;
            let admission = capture
                .lock_preserving(&editor, editor.snapshot().serial)
                .await?;
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            if shell.core.active_account.as_deref() != Some(account.id())
                || shell.core.account_epoch != epoch
                || shell.composer_key(target).as_ref() != Some(&key)
                || !admission.current(&shell)
                || !registry(&shell)?.current(account.id(), epoch, &key, &editor)
            {
                return Err(durable::Error::Stale);
            }
            let previous = session_view(&shell, &key).ok_or(durable::Error::Stale)?;
            if !publish_session(
                &shell,
                &previous,
                View::new(Inventory::Managed(current.clone()), None),
            ) {
                return Err(durable::Error::Stale);
            }
            editor.acknowledge_slash(&parent, &current, || admission.current(&shell))?;
            actions.acknowledge(&parent)
        }
        .await;
        let mut shell = state.write();
        if shell.core.active_account.as_deref() == Some(account.id())
            && shell.core.account_epoch == epoch
        {
            shell.draft_writes.finish_send(account.id(), epoch, &key);
            shell.show_toast(if result.is_ok() {
                "Saved command completion is reconciled. Nothing was sent again."
            } else {
                "The saved command completion still needs review. Its original outcome is retained."
            });
        }
    });
}
