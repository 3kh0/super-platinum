//! Project only a durably confirmed Schedule; never clear or restore native input.
use super::*;
use super_platinum_core::scheduled::ScheduledMessage;

// A reset or list/mutation generation change retires this queue projection.
// Durable completion/local handling does not depend on this ephemeral visit.
#[derive(Clone)]
pub(super) struct QueueVisit {
    epoch: u64,
    team: String,
    generation: u64,
    scoped: bool,
}
impl QueueVisit {
    pub(super) fn capture(shell: &ShellState, op: &Operation, epoch: u64) -> Self {
        Self {
            epoch,
            team: op.scope.key.team.clone(),
            generation: shell.core.scheduled.generation,
            scoped: shell
                .core
                .scheduled
                .matches_scope(epoch, &op.scope.key.team),
        }
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.scheduled.generation == self.generation
            && shell.core.scheduled.matches_scope(self.epoch, &self.team) == self.scoped
    }
}

// A completed consumption allocates a fresh client UUID. Preserve this exact
// durable successor before any async metadata refresh; storage revisions alone
// cannot distinguish release/re-enrollment of the same destination.
struct Lineage {
    successor: Option<String>,
}
impl Lineage {
    fn capture(op: &Operation, checked: &Checked) -> Self {
        Self {
            successor: checked
                .confirmed
                .then(|| {
                    checked
                        .view
                        .stored
                        .as_ref()
                        .filter(|stored| stored.state.scope() == &op.scope)
                        .map(|stored| stored.state.client_msg_id().to_owned())
                })
                .flatten(),
        }
    }
    fn accepts(&self, op: &Operation, view: &View) -> bool {
        view.stored.as_ref().is_some_and(|stored| {
            stored.state.scope() == &op.scope
                && (operation(view).as_ref() == Some(op)
                    || self.successor.as_deref() == Some(stored.state.client_msg_id()))
        })
    }
}

pub(super) async fn finish(
    mut state: Signal<ShellState>,
    op: Operation,
    binding: Source,
    epoch: u64,
    editor: Option<Arc<checkpoint::Editor>>,
    mut checked: Checked,
    queue: QueueVisit,
    mut publish_inline: impl FnMut(&'static str) -> bool,
) -> Result<&'static str, CheckError> {
    let lineage = Lineage::capture(&op, &checked);
    // The exact ticket and positive Outcome are durable before this function.
    // A terminal Schedule stays managed until explicit Continue editing locally:
    // generic Editor.release can rebase across a later control-only successor.
    if !state
        .try_peek()
        .is_ok_and(|shell| binding.current(&shell) && current_writer(&shell, &op, epoch, &editor))
    {
        return Ok(feedback(&checked));
    }
    if let Some(editor) = &editor {
        if let Ok(snapshot) = editor.refresh_receipt().await {
            if lineage.accepts(&op, &snapshot.view) {
                checked.view = snapshot.view;
                checked.view.uncommitted = snapshot.pending.or(checked.view.uncommitted);
            }
        } else if let Some(pending) = editor.snapshot().pending {
            checked.view.uncommitted = Some(pending);
        }
    }
    let text = feedback(&checked);
    if let Ok(mut shell) = state.try_write()
        && binding.current(&shell)
        && project(
            &mut shell,
            &op,
            epoch,
            &editor,
            &mut checked,
            &queue,
            &lineage,
        )
    {
        if !inline_feedback(
            &shell,
            &op,
            epoch,
            checked.confirmed,
            text,
            &mut publish_inline,
        ) {
            shell.show_toast(text);
        }
    }
    Ok(text)
}

fn feedback(checked: &Checked) -> &'static str {
    if checked.confirmed {
        "Schedule confirmed. Your current text was kept."
    } else if checked.read.cooldown_until_ms.is_some() {
        "Slack asked us to wait. The saved cooldown applies before checking the schedule again."
    } else {
        "The schedule is still unconfirmed. Nothing was scheduled again; your current text was kept."
    }
}

fn current_writer(
    shell: &ShellState,
    op: &Operation,
    epoch: u64,
    editor: &Option<Arc<checkpoint::Editor>>,
) -> bool {
    shell.core.active_account.as_ref() == Some(&op.scope.account_id)
        && shell.core.account_epoch == epoch
        && shell.core.active_team.as_ref() == Some(&op.scope.key.team)
        && shell.managed_drafts.account.as_ref() == Some(&op.scope.account_id)
        && shell.managed_drafts.epoch == epoch
        && session::same_transport(
            editor,
            &shell
                .draft_writes
                .writers
                .get(&op.scope.account_id, epoch, &op.scope.key),
        )
}

// This boundary is synchronous after the final await. Even None->Some is a
// writer replacement: an old task cannot project into a newly enrolled editor.
fn project(
    shell: &mut ShellState,
    op: &Operation,
    epoch: u64,
    editor: &Option<Arc<checkpoint::Editor>>,
    checked: &mut Checked,
    queue: &QueueVisit,
    lineage: &Lineage,
) -> bool {
    if !current_writer(shell, op, epoch, editor) {
        return false;
    }
    let Some(current) = shell
        .managed_drafts
        .entry(&op.scope.account_id, &op.scope.key)
    else {
        return false;
    };
    if !lineage.accepts(op, current) || !lineage.accepts(op, &checked.view) {
        return false;
    }
    if let Some(editor) = editor {
        let snapshot = editor.snapshot();
        if !lineage.accepts(op, &snapshot.view) {
            return false;
        }
        if let (Some(live), Some(incoming)) = (&snapshot.view.stored, &checked.view.stored) {
            if live.revision == incoming.revision && live.state != incoming.state {
                return false;
            }
            if live.revision > incoming.revision {
                checked.view = snapshot.view.clone();
            }
        }
        checked.view.uncommitted = snapshot.pending.or(snapshot.view.uncommitted);
    }
    if let (Some(current), Some(incoming)) = (&current.stored, &checked.view.stored)
        && current.revision >= incoming.revision
        && current.state != incoming.state
    {
        return false;
    }
    if checked.confirmed
        && queue.current(shell)
        && let Some(draft) = scheduled_message(op, &checked.read)
    {
        shell.core.scheduled.ensure_scope(epoch, &op.scope.key.team);
        // No filter switch, composer synchronization, chat reconciliation or
        // scheduled::saved toast. accept_remote rejects older/retired versions.
        let advances = shell
            .core
            .scheduled
            .items
            .get(&draft.id)
            .is_none_or(|current| {
                !current.last_updated_ts.is_empty()
                    && super_platinum_core::state::cmp_ts(
                        Some(&draft.last_updated_ts),
                        Some(&current.last_updated_ts),
                    )
                    .is_gt()
            });
        if advances {
            shell.core.scheduled.accept_remote(draft);
        }
    }
    let current = shell
        .managed_drafts
        .entry(&op.scope.account_id, &op.scope.key);
    if let Some(current) = current {
        if checked.view.uncommitted.is_none() {
            checked.view.uncommitted = current.uncommitted.clone();
        }
        if checked.view.blocked.is_none() {
            checked.view.blocked = current.blocked.clone();
        }
    }
    shell.managed_drafts.apply_view(
        &op.scope.account_id,
        epoch,
        op.scope.key.clone(),
        checked.view.clone(),
    );
    true
}

fn scheduled_message(op: &Operation, read: &Readback) -> Option<ScheduledMessage> {
    let Some(Outcome::Scheduled { draft, post_at }) = &read.outcome else {
        return None;
    };
    if !op.known_schedule_check()
        || op.kind != (OperationKind::Schedule { post_at: *post_at })
        || draft.scope != op.scope
        || draft.id != op.base.as_ref()?.id
        || draft.client_msg_id != op.client_msg_id
        || op.snapshot.as_ref() != Some(&draft.snapshot)
        || !super_platinum_core::state::cmp_ts(
            Some(&draft.last_updated_ts),
            Some(&op.base.as_ref()?.last_updated_ts),
        )
        .is_gt()
    {
        return None;
    }
    let row: ScheduledMessage =
        serde_json::from_value(read.raw.as_ref()?.get("draft")?.clone()).ok()?;
    (row.scheduled()
        && row.id == draft.id
        && row.client_msg_id == draft.client_msg_id
        && row.post_at == *post_at
        && row.last_updated_ts == draft.last_updated_ts
        && row.blocks == draft.snapshot.blocks
        && row.destinations == draft.snapshot.destinations
        && row.file_ids == draft.snapshot.file_ids)
        .then_some(row)
}

fn inline_feedback(
    shell: &ShellState,
    op: &Operation,
    epoch: u64,
    confirmed: bool,
    text: &'static str,
    publish: &mut impl FnMut(&'static str) -> bool,
) -> bool {
    !confirmed
        && matches!(op.kind, OperationKind::Schedule { .. })
        && shell.core.active_account.as_ref() == Some(&op.scope.account_id)
        && shell.core.account_epoch == epoch
        && shell.core.active_team.as_ref() == Some(&op.scope.key.team)
        && shell.main_view == crate::state::MainView::Scheduled
        && shell.core.scheduled.filter == super_platinum_core::scheduled::DraftFilter::Drafts
        && shell.thread_root.is_none()
        && shell.overlay.is_none()
        && shell.profile_hover.is_none()
        && !crate::read_shortcuts::bulk::dialog_open(shell)
        && shell
            .managed_drafts
            .entry(&op.scope.account_id, &op.scope.key)
            .and_then(operation)
            .as_ref()
            == Some(op)
        && publish(text)
}

#[cfg(test)]
#[path = "schedule_ui_tests.rs"]
mod tests;
