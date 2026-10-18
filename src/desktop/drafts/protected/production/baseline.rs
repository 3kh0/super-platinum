//! Existing review UI's configured-owner backend. One explicit actual read and
//! one explicit human choice; no automatic reconciliation or fabricated proof.
use super::*;
use crate::{
    state::ComposerTarget,
    view::composer::{
        protected_actions as actions, protected_enrollment as native,
        protected_operations::Root as ActionRoot, protected_projection,
    },
};
use dioxus::prelude::*;
use std::rc::Rc;
use super_platinum_core::drafts::store::sync::RemoteChoice;

/// A named owned native fixture must never fall back to the historical static
/// controller, including while its owned initialization is pending or refused.
pub(crate) fn configured_backend() -> bool {
    #[cfg(any(test, feature = "protected-native-fixture"))]
    if matches!(
        std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
        Ok("protected-owned-baseline" | "protected-owned-baseline-narrow")
    ) {
        return true;
    }
    !crate::fixture::is_fixture()
}

#[derive(Clone)]
pub(crate) struct Prepared {
    pub evidence: Arc<Evidence>,
    editor: Arc<editor::Editor>,
    target: ComposerTarget,
    epoch: u64,
    view: u64,
    channel: u64,
    thread: u64,
    serial: u64,
    local: LocalDraft,
}
impl Prepared {
    pub fn destination_current(&self, shell: &ShellState) -> bool {
        let scope = self.editor.scope();
        shell.core.active_account.as_deref() == Some(scope.account_id.as_str())
            && shell.core.account_epoch == self.epoch
            && shell.draft_writes.view_id == self.view
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.composer_key(self.target).as_ref() == Some(&scope.key)
            && registry(shell)
                .is_ok_and(|r| r.current(&scope.account_id, self.epoch, &scope.key, &self.editor))
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.destination_current(shell)
            && self.evidence.source().current(shell)
            && self.evidence.binding().current(shell)
            && self.editor.snapshot().serial == self.serial
            && same_local(&local(shell, self.target), &self.local)
            && self
                .editor
                .snapshot()
                .view
                .inventory()
                .managed()
                .is_ok_and(|r| r.same(self.evidence.review()))
    }
    pub fn team(&self) -> &str {
        &self.editor.scope().key.team
    }
}
fn local(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
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
pub(crate) fn available(shell: &ShellState, target: ComposerTarget) -> bool {
    shell
        .composer_key(target)
        .and_then(|key| session_view(shell, &key))
        .is_some_and(|view| matches!(view.inventory(), Inventory::Managed(_)))
}
fn installed(shell: &ShellState, target: ComposerTarget) -> Option<Arc<editor::Editor>> {
    let id = shell.core.active_account.as_deref()?;
    let account = account(id).ok()?;
    let key = shell.composer_key(target)?;
    let root = root().lock().ok()?;
    Some(root.host().installation(&account, &key)?.editor().clone())
}
pub(crate) fn can_finish(shell: &ShellState, target: ComposerTarget) -> bool {
    installed(shell, target).is_some_and(|editor| {
        let s = editor.snapshot();
        !s.running
            && (s.projection.is_some()
                || s.choice_retry_available
                || editor
                    .owner
                    .account
                    .retained_operations()
                    .is_ok_and(|values| {
                        values
                            .iter()
                            .any(|v| v.belongs(&editor) && v.baseline_projection_pending())
                    }))
    })
}
pub(crate) async fn open(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Prepared, durable::Error> {
    let (account, key, local, epoch, view, channel, thread, binding, connection) = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let key = shell.composer_key(target).ok_or(durable::Error::Stale)?;
        let account = account(
            shell
                .core
                .active_account
                .as_deref()
                .ok_or(durable::Error::Stale)?,
        )?;
        (
            account,
            key.clone(),
            local(&shell, target),
            shell.core.account_epoch,
            shell.draft_writes.view_id,
            shell.channel_generation,
            shell.thread_generation,
            crate::drafts::network::Binding::capture(&shell, &key.team)
                .map_err(|_| durable::Error::Gap)?,
            socket::capture(&shell, &key.team)?,
        )
    };
    let original = native::capture_baseline(state, target, &local).await?;
    let editor = enroll::editor_baseline(state, target, account, &key, &local).await?;
    let snapshot = editor.flush().await?;
    let expected = snapshot.view.inventory().managed()?.clone();
    let source = connection.ensure(state).await?;
    let admission = original.lock_preserving(&editor, snapshot.serial).await?;
    let read = {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        if !admission.current(&shell)
            || shell.core.account_epoch != epoch
            || shell.draft_writes.view_id != view
            || shell.channel_generation != channel
            || shell.thread_generation != thread
        {
            return Err(durable::Error::Stale);
        }
        editor.owner.begin_owned_baseline(
            &shell,
            expected,
            binding,
            source,
            crate::drafts::send::now_ms(),
        )?
    };
    drop(admission);
    let result = read.explicit_read(state, crate::drafts::send::now_ms).await;
    if let Some(raw) = read.transport_read() {
        if let Some(error) = &raw.error {
            let mut shell = state.write();
            if connection.current(&shell) {
                shell.report_failure(error, "Connect to Slack to check draft versions.", || {
                    "Draft versions could not be checked. Your input is retained.".into()
                });
            }
        }
    }
    let evidence = match result? {
        BaselineRun::Evidence(evidence) => Arc::new(evidence),
        BaselineRun::Deferred { until } => return Err(durable::Error::Cooldown(until)),
        BaselineRun::Nonpositive(raw) => {
            let _ = read.acknowledge();
            return Err(raw
                .cooldown_until_ms
                .map(durable::Error::Cooldown)
                .unwrap_or(durable::Error::ReviewRequired));
        }
    };
    let prepared = Prepared {
        evidence,
        editor,
        target,
        epoch,
        view,
        channel,
        thread,
        serial: snapshot.serial,
        local,
    };
    if !prepared.current(&state.peek()) {
        // The host has observed and rejected this exact historical result.
        // No projection or new request follows; release only if its original
        // terminal/fence acknowledgment permits that stale disposition.
        let _ = read.acknowledge();
        return Err(durable::Error::Stale);
    }
    // The host now retains the actual full evidence. A refused retirement keeps
    // the same family; never turn this into a new request on subsequent click.
    let _ = read.acknowledge();
    Ok(prepared)
}
pub(crate) async fn apply(
    state: Signal<ShellState>,
    prepared: Prepared,
    choice: RemoteChoice,
) -> Result<(), durable::Error> {
    if !prepared.current(&state.peek()) {
        return Err(durable::Error::Stale);
    }
    let root =
        dioxus::core::try_consume_context::<Rc<ActionRoot>>().ok_or(durable::Error::Stale)?;
    let operation = actions::start_baseline(
        &root,
        actions::StartBaseline {
            state,
            target: prepared.target,
            registry: registry(&state.peek())?,
            editor: prepared.editor,
            visit: Visit::new(),
            evidence: prepared.evidence,
            choice,
        },
    )
    .map_err(|refused| refused.error)?;
    match operation.ready().await? {
        operation::Status::Finished => root.acknowledge_baseline(&operation).map(|_| ()),
        _ => Err(durable::Error::ReviewRequired),
    }
}
/// Complete only the same retained local choice/barrier. No reread or new choice.
pub(crate) async fn finish(
    state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<(), durable::Error> {
    let editor = installed(&state.peek(), target).ok_or(durable::Error::Stale)?;
    let root =
        dioxus::core::try_consume_context::<Rc<ActionRoot>>().ok_or(durable::Error::Stale)?;
    let retained = editor.owner.account.retained_operations()?;
    let mut parents = retained
        .into_iter()
        .filter(|v| v.belongs(&editor) && v.baseline_pending())
        .collect::<Vec<_>>();
    if parents.len() != 1 {
        return Err(durable::Error::Pending);
    }
    let parent = parents.pop().ok_or(durable::Error::Stale)?;
    let _observed_failure = parent.ready().await?;
    if parent.baseline_projection_pending() {
        return root.acknowledge_baseline(&parent).map(|_| ());
    }
    let snapshot = editor.snapshot();
    if snapshot.choice_retry_available {
        editor.retry_baseline_choice(state).await?;
    }
    let barrier = editor.snapshot().projection.ok_or(durable::Error::Stale)?;
    let ticket = editor.prepare_projection(barrier.id()).await?;
    use protected_projection::ProjectionResult;
    let attempt = match protected_projection::capture_baseline_projection(
        state,
        target,
        editor.clone(),
        ticket,
        registry(&state.peek())?,
    )
    .await
    {
        ProjectionResult::Ready(attempt) => attempt,
        ProjectionResult::Changed(changed) => {
            changed.preserve(&editor).await?;
            return Err(durable::Error::ReviewRequired);
        }
        ProjectionResult::Deferred(_) => return Err(durable::Error::ReviewRequired),
    };
    let applied = match attempt.apply().await {
        ProjectionResult::Ready(applied) => applied,
        ProjectionResult::Changed(changed) => {
            changed.preserve(&editor).await?;
            return Err(durable::Error::ReviewRequired);
        }
        ProjectionResult::Deferred(_) => return Err(durable::Error::ReviewRequired),
    };
    let proof = editor
        .acknowledge_projection_observed(applied.install()?)
        .await?;
    parent.record_baseline_projection(proof)?;
    root.acknowledge_baseline(&parent).map(|_| ())
}
