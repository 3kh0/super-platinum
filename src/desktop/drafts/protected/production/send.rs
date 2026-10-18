//! The ordinary button enters the existing protected renderer-root action.
//! Storage preparation is retained by Host; actual HTTP remains in Delivery.
use super::*;
use crate::{
    state::{ComposerTarget, ShellState},
    view::composer::{
        protected_actions as actions, protected_enrollment as native,
        protected_operations::Root as ActionRoot,
    },
};
use dioxus::prelude::*;
use std::rc::Rc;

pub(super) struct Captured {
    pub(super) account: Arc<Account>,
    pub(super) key: DraftKey,
    pub(super) local: LocalDraft,
    pub(super) hosted: bool,
    pub(super) target: ComposerTarget,
    pub(super) epoch: u64,
    pub(super) view: u64,
    pub(super) channel: u64,
    pub(super) thread: u64,
    source: OnceLock<Arc<Source>>,
    connection: Arc<socket::Connection>,
    pub(super) binding: crate::drafts::network::Binding,
}
pub(super) fn local(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
    match target {
        ComposerTarget::Channel => LocalDraft {
            composer: shell.core.composer.clone(),
            attachments: shell.core.composer_attachments.clone(),
        },
        ComposerTarget::Thread => LocalDraft {
            composer: shell.core.thread_composer.clone(),
            attachments: shell.core.thread_composer_attachments.clone(),
        },
    }
}
impl Captured {
    pub(super) fn source(&self) -> Result<&Arc<Source>, durable::Error> {
        self.source.get().ok_or(durable::Error::Stale)
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_deref() == Some(self.account.id())
            && shell.core.account_epoch == self.epoch
            && shell.draft_writes.view_id == self.view
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.composer_key(self.target).as_ref() == Some(&self.key)
            && self.connection.current(shell)
            && self.source.get().is_none_or(|source| source.current(shell))
            && self.binding.current(shell)
    }
    pub(super) fn unchanged(&self, shell: &ShellState) -> bool {
        self.current(shell)
            && same_local(&local(shell, self.target), &self.local)
            && shell.core.drafts.items.get(&self.key).map_or_else(
                || {
                    self.hosted
                        && self.local.composer == Default::default()
                        && self.local.attachments.is_empty()
                },
                |v| same_local(v, &self.local),
            )
    }
}

pub(crate) fn intercept(mut state: Signal<ShellState>, target: ComposerTarget) -> bool {
    if crate::fixture::is_fixture() {
        #[cfg(feature = "protected-native-fixture")]
        {
            if !super::hosted_native::named() {
                return false;
            }
            if !super::hosted_native::current(&state.peek()) {
                state
                    .write()
                    .show_toast("The closed hosted fixture owner is no longer current.");
                return true;
            }
        }
        #[cfg(not(feature = "protected-native-fixture"))]
        return false;
    }
    let result = (|| {
        let action_root =
            dioxus::core::try_consume_context::<Rc<ActionRoot>>().ok_or(durable::Error::Stale)?;
        let mut shell = state.write();
        let id = shell
            .core
            .active_account
            .clone()
            .ok_or(durable::Error::Stale)?;
        let key = shell.composer_key(target).ok_or(durable::Error::Stale)?;
        if shell.draft_writes.sending.contains(&key) {
            return Ok(None);
        }
        let local = local(&shell, target);
        let hosted = super::hosted::present(&shell, target);
        if hosted && (target != ComposerTarget::Channel || !local.attachments.is_empty()) {
            return Err(durable::Error::Unsupported);
        }
        if local.empty() && !hosted {
            return Ok(None);
        }
        // Files keep their actual upload/share protocol and owned local/journal
        // path. They never enter the text-only protected Consume codec.
        if !hosted
            && local.attachments.is_empty()
            && crate::drafts::local_snapshot(&key, &local).is_err()
        {
            return Err(durable::Error::Unsupported);
        }
        if !hosted && local.attachments.is_empty() {
            action_root.has_capacity()?;
        }
        operation::bounded(&local)?;
        let account = account(&id)?;
        let connection = socket::capture(&shell, &key.team)?;
        let binding = crate::drafts::network::Binding::capture(&shell, &key.team)
            .map_err(|_| durable::Error::Gap)?;
        shell.checkpoint_composers();
        // The same selected lane owns this exact input before any await. A
        // still-opening lane retains it, without spinning or another writer.
        match root()
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .submit_current(&shell, &key, &local)
        {
            Ok(_) | Err(root_factory::Error::Storage(durable::Error::Pending)) => (),
            Err(e) => return Err(error(e)),
        }
        let epoch = shell.core.account_epoch;
        if !shell.draft_writes.begin_send(&id, epoch, key.clone()) {
            return Ok(None);
        }
        let captured = Captured {
            account,
            key,
            local,
            hosted,
            target,
            epoch: shell.core.account_epoch,
            view: shell.draft_writes.view_id,
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            source: OnceLock::new(),
            connection,
            binding,
        };
        Ok(Some((action_root, captured)))
    })();
    match result {
        Ok(Some((root, captured))) => {
            if captured.hosted {
                super::hosted::start(state, captured);
                return true;
            }
            if !captured.local.attachments.is_empty() {
                super::uploads::start(state, captured);
                return true;
            }
            if captured.local.composer.text.starts_with('/') {
                super::commands::open(state, root, captured);
                return true;
            }
            dioxus::core::spawn_forever(async move {
                let result = prepare(state, root.clone(), &captured).await;
                let result = match result {
                    Ok(operation) => match operation.ready().await {
                        Ok(operation::Status::Finished) => {
                            // Reclamation must also hand the exact released
                            // LocalWriter back to Host before another Send.
                            super::reclaim::complete(
                                state,
                                &root,
                                &captured.account,
                                &captured.key,
                                &operation,
                            )
                        }
                        Ok(_) => Err(durable::Error::ReviewRequired),
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(e),
                };
                let mut state = state;
                let mut shell = state.write();
                if shell.core.active_account.as_deref() == Some(captured.account.id())
                    && shell.core.account_epoch == captured.epoch
                {
                    shell.draft_writes.finish_send(
                        captured.account.id(),
                        captured.epoch,
                        &captured.key,
                    );
                    if result.is_err() {
                        shell.show_toast("Your message is retained. Review its saved delivery status before trying again.");
                    }
                }
            });
        }
        Ok(None) => (),
        Err(_) => state.write().show_toast(
            "This message is retained. Its owned delivery needs attention before sending.",
        ),
    }
    true
}

async fn prepare(
    state: Signal<ShellState>,
    root: Rc<ActionRoot>,
    captured: &Captured,
) -> Result<operation::Operation, durable::Error> {
    let (original, editor) = prepare_source(state, captured).await?;
    let before = editor.flush().await?;
    if !captured.unchanged(&state.peek()) || !same_local(before.view.local(), &captured.local) {
        return Err(durable::Error::Stale);
    }
    let expected = before.view.inventory().managed()?.clone();
    captured
        .source()?
        .prepare_basis(state, editor.owner.clone(), expected)
        .await?;
    let after = editor.refresh_receipt().await?;
    if after.serial != before.serial || !captured.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let previous = session_view(&state.peek(), &captured.key).ok_or(durable::Error::Stale)?;
    if !publish_session(&state.peek(), &previous, after.view.clone()) {
        return Err(durable::Error::Stale);
    }
    let admission = original.lock_preserving(&editor, after.serial).await?;
    if !admission.current(&state.peek())
        || !captured.unchanged(&state.peek())
        || editor.snapshot().serial != after.serial
    {
        return Err(durable::Error::Stale);
    }
    let registry = registry(&state.peek())?;
    let delivery = delivery::Delivery::capture_configured(
        state,
        registry.clone(),
        editor.clone(),
        captured.source()?.clone(),
        captured.binding.clone(),
    )?;
    // No await between this original-generation check and Start's synchronous
    // exact editor/native request capture. Later changes fail that captured CAS.
    drop(admission);
    actions::start(
        &root,
        actions::Start {
            state,
            target: captured.target,
            registry,
            editor,
            visit: Visit::new(),
            request: operation::Request::Send,
            route: actions::Route::Delivery(delivery, crate::drafts::send::now_ms),
        },
    )
    .map_err(|refused| refused.error)
}

/// First Send/command discovery may be the first enrollment in this team.
/// Preserve the exact native generation before enrollment, then retry only the
/// accepted socket captured by this input. No reconnect or dummy destination.
pub(super) async fn prepare_source(
    state: Signal<ShellState>,
    captured: &Captured,
) -> Result<(native::Capture, Arc<editor::Editor>), durable::Error> {
    if !captured.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let original = if captured.hosted {
        native::capture_hosted(state, captured.target, &captured.local).await?
    } else {
        native::capture(state, captured.target, &captured.local).await?
    };
    let editor = if captured.hosted {
        super::enroll::editor_hosted(
            state,
            captured.target,
            captured.account.clone(),
            &captured.key,
            &captured.local,
        )
        .await?
    } else {
        super::enroll::editor(
            state,
            captured.target,
            captured.account.clone(),
            &captured.key,
            &captured.local,
        )
        .await?
    };
    if !captured.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let source = captured.connection.ensure(state).await?;
    if !captured.unchanged(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    if let Some(prior) = captured.source.get() {
        if !Arc::ptr_eq(prior, &source) {
            return Err(durable::Error::Stale);
        }
    } else {
        captured
            .source
            .set(source)
            .map_err(|_| durable::Error::Stale)?;
    }
    Ok((original, editor))
}
