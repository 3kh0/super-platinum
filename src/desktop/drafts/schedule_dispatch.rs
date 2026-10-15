use super::super::{
    Owner,
    dispatch::{DispatchResult, Outcome},
    network::Attempt,
};
use super::*;
use super_platinum_core::drafts::sync::ConsumeIntent;

pub(super) fn supported(
    lease: &ScheduleLease,
    snapshot: &Snapshot,
    time: i64,
) -> Result<(), Fault> {
    let before = &lease
        .before()
        .stored
        .as_ref()
        .ok_or(Fault::Unmanaged)?
        .state;
    let mut next = before.clone();
    next.request_reviewed_schedule(before, snapshot.clone(), time)
        .map_err(|_| Fault::Unsupported)?;
    next.begin_operation(lease.scope())
        .filter(Operation::known_schedule_check)
        .map(|_| ())
        .ok_or(Fault::Unsupported)
}
pub(super) fn owner_current(dialog: &Dialog, shell: &ShellState) -> bool {
    dialog.request.binding.current(shell)
        && shell.draft_writes.view_id == dialog.request.shell_visit
        && shell.core.active_team.as_ref() == Some(&dialog.request.key.team)
        && shell
            .draft_writes
            .writers
            .get(
                &dialog.request.account,
                dialog.request.epoch,
                &dialog.request.key,
            )
            .is_some_and(|current| Arc::ptr_eq(&current, &dialog.editor))
}
pub(super) fn project_consumed(mut state: Signal<ShellState>, dialog: &Dialog) {
    let mut shell = match state.try_write() {
        Ok(shell) => shell,
        Err(_) => return,
    };
    if !owner_current(dialog, &shell) {
        return;
    }
    let snapshot = dialog.editor.snapshot();
    let mut view = snapshot.view;
    view.uncommitted = snapshot.pending;
    shell.managed_drafts.apply_view(
        &dialog.request.account,
        dialog.request.epoch,
        dialog.request.key.clone(),
        view,
    );
    let Mode::Create {
        target,
        local: expected,
        field,
        thread,
    } = &dialog.request.mode
    else {
        return;
    };
    // Editor serial protects edit/revert ABA; the native field additionally checks
    // current scope/text/composition before applying the existing conditional clear.
    if snapshot.serial != dialog.lease.serial()
        || snapshot.unsaved
        || shell.composer_key(*target).as_ref() != Some(&dialog.request.key)
        || crate::view::composer::field_scope(&shell, *target) != *field
        || thread.is_some_and(|generation| generation != shell.thread_generation)
        || !super::super::same_local(&local(&shell, *target), expected)
    {
        return;
    }
    if shell
        .core
        .drafts
        .items
        .get(&dialog.request.key)
        .is_some_and(|draft| super::super::same_local(draft, expected))
    {
        shell.core.drafts.items.remove(&dialog.request.key);
    }
    match target {
        ComposerTarget::Channel => {
            shell.core.composer = Default::default();
            shell.core.composer_attachments.clear();
        }
        ComposerTarget::Thread => {
            shell.core.thread_composer = Default::default();
            shell.core.thread_composer_attachments.clear();
        }
    }
    drop(shell);
    crate::view::composer::clear_delivered_field(
        if *target == ComposerTarget::Thread {
            "thread-composer"
        } else {
            "channel-composer"
        },
        field,
        &expected.composer.text,
    );
}

// Only pure prepare CAS may repeat. All operation construction/recording remains
// in the same Owner; no duplicate Schedule-specific persistence reducer exists.
pub(crate) async fn once<F, Fut>(
    owner: &Owner,
    intent: &ConsumeIntent,
    send: F,
) -> Result<(DispatchResult, Attempt), Fault>
where
    F: FnOnce(Operation) -> Fut,
    Fut: std::future::Future<Output = Attempt>,
{
    let mut send = Some(send);
    for _ in 0..4 {
        let view = owner.load().await?;
        let row = view.stored.ok_or(Fault::Unmanaged)?;
        if row.state.consumption() != Some(intent)
            || row.state.pending().is_some()
            || send.is_none()
        {
            return Err(Fault::Pending);
        }
        let mut attempt = None;
        let result = owner
            .dispatch(row.revision, |operation| {
                let output = &mut attempt;
                let send = send.take();
                async move {
                    let Some(send) = send else {
                        return Outcome::Uncertain;
                    };
                    let result = if operation.known_schedule_check() {
                        send(operation).await
                    } else {
                        super::super::network::Attempt::blocked(
                            super::super::network::Block::Unsupported,
                        )
                    };
                    let outcome = result.outcome.clone();
                    *output = Some(result);
                    outcome
                }
            })
            .await;
        match result {
            Err(Fault::Stale) if attempt.is_none() => continue,
            result => {
                let mut receipt = result?;
                let attempt = attempt.ok_or(Fault::Pending)?;
                if let Some(deadline) = attempt.cooldown_until_ms {
                    receipt.stored = owner.defer_check(&receipt.operation, deadline).await?;
                }
                return Ok((receipt, attempt));
            }
        }
    }
    Err(Fault::Stale)
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    dialog: &Dialog,
    intent: &ConsumeIntent,
) -> Result<&'static str, Fault> {
    let owner = dialog.editor.owner();
    let (receipt, attempt) = once(&owner, intent, |operation| async move {
        dialog
            .request
            .binding
            .execute(&operation, super::super::send::now_ms())
            .await
    })
    .await?;
    let text = match (&attempt.outcome, receipt.applied) {
        (Outcome::Scheduled { .. }, true) => "Message scheduled.",
        (Outcome::Rejected, true) => {
            "Message was not scheduled. Review it in Drafts to retry or change the schedule."
        }
        _ => "Schedule is unconfirmed. Check its status in Drafts before trying again.",
    };
    if !state
        .try_peek()
        .is_ok_and(|shell| owner_current(dialog, &shell))
    {
        return Ok(text);
    }
    let snapshot = dialog.editor.refresh_receipt().await?;
    let mut view = snapshot.view;
    view.uncommitted = snapshot.pending.or(view.uncommitted);
    let successor = (receipt.applied && matches!(attempt.outcome, Outcome::Scheduled { .. }))
        .then_some(receipt.stored.state.client_msg_id());
    if let Ok(mut shell) = state.try_write() {
        if !owner_current(dialog, &shell) {
            return Ok(text);
        }
        let Some(current) = shell
            .managed_drafts
            .entry(&dialog.request.account, &dialog.request.key)
        else {
            return Ok(text);
        };
        // A newer native input may have arrived after the Tokio refresh snapshot.
        // Re-read its complete latest state at this final synchronous boundary.
        let latest = dialog.editor.snapshot();
        if !merge_projection(&mut view, latest, current, &receipt, intent, successor) {
            return Ok(text);
        }
        if receipt.applied
            && shell.core.scheduled.generation == dialog.scheduled_generation
            && shell
                .core
                .scheduled
                .matches_scope(dialog.request.epoch, &dialog.request.key.team)
                == dialog.scheduled_scoped
            && let Some(message) = scheduled_message(&receipt.operation, &attempt)
        {
            shell
                .core
                .scheduled
                .ensure_scope(dialog.request.epoch, &dialog.request.key.team);
            if advances_queue(shell.core.scheduled.items.get(&message.id), &message) {
                shell.core.scheduled.accept_remote(message);
            }
        }
        shell.managed_drafts.apply_view(
            &dialog.request.account,
            dialog.request.epoch,
            dialog.request.key.clone(),
            view,
        );
    }
    // Deliberately keep the managed row. Continue editing locally is explicit;
    // generic release after async receipt refresh is not completion-bound.
    Ok(text)
}
fn lineage(view: &View, intent: &ConsumeIntent, successor: Option<&str>) -> bool {
    view.stored.as_ref().is_some_and(|row| {
        row.state.scope() == &intent.scope
            && ((row.state.client_msg_id() == intent.client_msg_id
                && row.state.consumption() == Some(intent))
                || successor == Some(row.state.client_msg_id()))
    })
}
fn merge_projection(
    incoming: &mut View,
    latest: super::super::checkpoint::Snapshot,
    current: &View,
    receipt: &DispatchResult,
    intent: &ConsumeIntent,
    successor: Option<&str>,
) -> bool {
    if !lineage(incoming, intent, successor)
        || !lineage(&latest.view, intent, successor)
        || !lineage(current, intent, successor)
    {
        return false;
    }
    let incoming_row = incoming.stored.as_ref().unwrap();
    let latest_row = latest.view.stored.as_ref().unwrap();
    if incoming_row.revision < receipt.stored.revision
        || (incoming_row.revision == receipt.stored.revision
            && incoming_row.state != receipt.stored.state)
        || latest_row.revision < incoming_row.revision
        || (latest_row.revision == incoming_row.revision && latest_row.state != incoming_row.state)
    {
        return false;
    }
    let mut projected = latest.view;
    projected.uncommitted = latest
        .pending
        .or(projected.uncommitted)
        .or(current.uncommitted.clone());
    if let (Some(latest), Some(current)) = (&projected.blocked, &current.blocked)
        && (latest.token != current.token
            || !super::super::same_local(&latest.draft, &current.draft))
    {
        return false;
    }
    projected.blocked = current.blocked.clone().or(projected.blocked);
    let projected_row = projected.stored.as_ref().unwrap();
    let current_row = current.stored.as_ref().unwrap();
    if current_row.revision > projected_row.revision
        || (current_row.revision == projected_row.revision
            && current_row.state != projected_row.state)
    {
        return false;
    }
    *incoming = projected;
    true
}
pub(crate) fn advances_queue(
    current: Option<&super_platinum_core::scheduled::ScheduledMessage>,
    incoming: &super_platinum_core::scheduled::ScheduledMessage,
) -> bool {
    current.is_none_or(|current| {
        !current.last_updated_ts.is_empty()
            && super_platinum_core::state::cmp_ts(
                Some(&incoming.last_updated_ts),
                Some(&current.last_updated_ts),
            )
            .is_gt()
    })
}
pub(crate) fn scheduled_message(
    operation: &Operation,
    attempt: &Attempt,
) -> Option<super_platinum_core::scheduled::ScheduledMessage> {
    let Outcome::Scheduled { draft, post_at } = &attempt.outcome else {
        return None;
    };
    if !operation.known_schedule_check()
        || draft.scope != operation.scope
        || operation.kind
            != (super_platinum_core::drafts::sync::OperationKind::Schedule { post_at: *post_at })
        || draft.id != operation.base.as_ref()?.id
        || draft.client_msg_id != operation.client_msg_id
        || operation.snapshot.as_ref() != Some(&draft.snapshot)
    {
        return None;
    }
    let message: super_platinum_core::scheduled::ScheduledMessage =
        serde_json::from_value(attempt.raw.as_ref()?.get("draft")?.clone()).ok()?;
    (message.scheduled()
        && message.id == draft.id
        && message.client_msg_id == draft.client_msg_id
        && message.last_updated_ts == draft.last_updated_ts
        && message.post_at == *post_at
        && message.blocks == draft.snapshot.blocks
        && message.destinations == draft.snapshot.destinations
        && message.file_ids == draft.snapshot.file_ids)
        .then_some(message)
}

#[cfg(test)]
#[path = "schedule_projection_tests.rs"]
mod tests;
