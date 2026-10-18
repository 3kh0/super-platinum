//! Direct text sends consume a durable local draft before the native field clears.
//! A captured operation is attempted once; account changes cannot redirect it.
use super::{Account, Fault, Owner, View, checkpoint::Editor, dispatch::Outcome};
#[path = "send_check.rs"]
pub(crate) mod check;
#[path = "send_fixture.rs"]
pub(crate) mod fixture;
#[path = "send_source.rs"]
mod source;
use crate::state::{ComposerTarget, PendingDeliveryStatus, PendingSend, ShellState};
use dioxus::prelude::*;
use source::Source;
use std::sync::Arc;
use super_platinum_core::{
    drafts::{
        DraftKey, LocalDraft,
        sync::{ConsumeIntent, ConsumeKind, Workflow},
    },
    slack::models::Message,
};

/// Return true when this action was handled (including an explicit safe stop).
/// Unsupported unmanaged content continues through the existing rich/file path.
pub(crate) fn intercept(state: Signal<ShellState>) -> bool {
    intercept_target(state, ComposerTarget::Channel)
}

pub(crate) fn intercept_target(mut state: Signal<ShellState>, target: ComposerTarget) -> bool {
    #[cfg(feature = "protected-native-fixture")]
    if super::protected::slash_native::intercept(state, target) {
        return true;
    }
    #[cfg(feature = "protected-native-fixture")]
    if super::protected::owned_native::intercept(state, target) {
        return true;
    }

    #[cfg(feature = "protected-native-fixture")]
    if super::protected::native_fixture::intercept(state, target) {
        return true;
    }
    if crate::fixture::is_fixture() && !fixture::enabled() {
        // A named managed-Send fixture never falls through to legacy sending,
        // even when its required explicit opt-in is absent or malformed.
        return fixture::named();
    }
    if super::protected::production::send::intercept(state, target) {
        return true;
    }
    let captured = {
        let mut shell = state.write();
        let Some(account_id) = shell.core.active_account.clone() else {
            return fixture::named();
        };
        let Some(key) = shell.composer_key(target) else {
            return fixture::named();
        };
        let epoch = shell.core.account_epoch;
        if shell.draft_writes.sending.contains(&key) {
            return true;
        }
        if !shell.managed_drafts.ready_for(&account_id, epoch) {
            shell.show_toast("Saved draft status is unavailable. Refresh Drafts before sending.");
            return true;
        }
        if shell.managed_drafts.local_review_pending(&account_id, &key) {
            shell.show_toast("Saved local versions need review in Drafts before sending. Your current draft is still here.");
            return true;
        }
        let local = current_local_for(&shell, target);
        let managed = shell
            .managed_drafts
            .entry(&account_id, &key)
            .filter(|view| view.stored.is_some());
        if managed.is_some_and(|view| !can_consume(view)) {
            shell.show_toast(
                "A saved message needs review in Drafts before sending another from this composer.",
            );
            return true;
        }
        if local.composer.text.trim().is_empty() && local.attachments.is_empty() {
            return true;
        }
        if super::local_snapshot(&key, &local).is_err() {
            if managed.is_none() {
                return fixture::named();
            }
            shell.show_toast("This draft has saved delivery state. Review it in Drafts before sending this content.");
            return true;
        }
        let Ok(binding) = Source::capture(&shell, &key.team) else {
            shell.show_toast("Connect to Slack before sending. Your draft is still here.");
            return true;
        };
        let Ok(account) = binding.account(&account_id) else {
            shell.show_toast("Your draft could not be saved. The message has not been sent.");
            return true;
        };
        shell.checkpoint_composers();
        if !shell
            .draft_writes
            .begin_send(&account_id, epoch, key.clone())
        {
            return true;
        }
        let field_scope = crate::view::composer::field_scope(&shell, target);
        Captured {
            account_id,
            epoch,
            key,
            local,
            account,
            binding,
            field_scope,
            thread_generation: (target == ComposerTarget::Thread)
                .then_some(shell.thread_generation),
        }
    };
    // Component-scoped tasks can disappear when navigating away from a composer.
    // Once consumed, this root task owns the captured account through the receipt.
    dioxus::core::spawn_forever(async move {
        let result = run(state, &captured).await;
        let mut shell = state.write();
        if captured.current(&shell) {
            shell
                .draft_writes
                .finish_send(&captured.account_id, captured.epoch, &captured.key);
            if let Err(error) = result {
                shell.show_toast(match error {
                    Fault::Stale => "The draft changed while saving. Review the current text before sending.",
                    Fault::Pending | Fault::AlternateEdits => "This saved message needs review in Drafts.",
                    _ => "The saved message needs attention in Drafts. It has not been automatically sent again.",
                });
            }
        }
        fixture::ui_completed(&captured.account_id, &captured.key);
    });
    true
}

struct Captured {
    account_id: String,
    epoch: u64,
    key: DraftKey,
    local: LocalDraft,
    account: Arc<Account>,
    binding: Source,
    field_scope: String,
    thread_generation: Option<u64>,
}
impl Captured {
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account.as_deref() == Some(&self.account_id)
            && shell.core.account_epoch == self.epoch
            && self.binding.current(shell)
    }
    fn unchanged(&self, shell: &ShellState) -> bool {
        self.binding.current(shell)
            && self
                .thread_generation
                .is_none_or(|generation| generation == shell.thread_generation)
            && shell.composer_key(target_for(&self.key)).as_ref() == Some(&self.key)
            && super::same_local(
                &current_local_for(shell, target_for(&self.key)),
                &self.local,
            )
    }
}
fn target_for(key: &DraftKey) -> ComposerTarget {
    if key.thread.is_some() {
        ComposerTarget::Thread
    } else {
        ComposerTarget::Channel
    }
}
#[cfg(test)]
fn current_local(shell: &ShellState) -> LocalDraft {
    current_local_for(shell, ComposerTarget::Channel)
}
fn current_local_for(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
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
fn can_consume(view: &View) -> bool {
    view.blocked.is_none()
        && view.uncommitted.is_none()
        && view.stored.as_ref().is_some_and(|stored| {
            matches!(stored.state.workflow(), Workflow::Idle)
                && stored.state.consumption().is_none()
        })
}

async fn run(mut state: Signal<ShellState>, captured: &Captured) -> Result<(), Fault> {
    let owner = captured.account.owner(captured.key.clone());
    let mut view = owner.load().await?;
    if !captured.unchanged(&state.peek()) {
        return Err(Fault::Stale);
    }
    let mut enrollment = None;
    let mut captured_editor = None;
    let preparation = async {
        if view.stored.is_none() {
            enrollment = Some(owner.enroll(captured.local.clone(), None).await?.revision);
            view = owner.load().await?;
        }
        if !can_consume(&view) {
            return Err(Fault::Pending);
        }
        let editor = {
            let mut shell = state.write();
            if !captured.current(&shell) {
                return Err(Fault::Stale);
            }
            shell.managed_drafts.apply_view(
                &captured.account_id,
                captured.epoch,
                captured.key.clone(),
                view.clone(),
            );
            let editor = shell.draft_writes.editor(
                &captured.account,
                captured.epoch,
                captured.key.clone(),
                view,
            )?;
            captured_editor = Some(editor.clone());
            if !captured.unchanged(&shell) {
                return Err(Fault::Stale);
            }
            editor
        };
        let consumed = editor
            .consume(captured.local.clone(), ConsumeKind::Send)
            .await?;
        Ok::<_, Fault>((editor, consumed))
    }
    .await;
    let (editor, consumed) = match preparation {
        Ok(prepared) => prepared,
        Err(error) => {
            if let Some(enrolled_revision) = enrollment {
                let released =
                    abort_enrollment(&owner, enrolled_revision, captured_editor.as_ref()).await;
                // Persistence belongs to the captured owner even if this visit
                // disappeared. Feedback never redirects to the replacement shell.
                let retained = if released.is_err() {
                    owner.load().await.ok()
                } else {
                    None
                };
                let mut shell = state.write();
                if captured.current(&shell) {
                    if let Ok(released) = released {
                        let current = shell
                            .managed_drafts
                            .entry(&captured.account_id, &captured.key)
                            .and_then(|view| view.stored.as_ref())
                            .map(|stored| stored.revision);
                        if let Some(current) = current.filter(|current| *current <= released) {
                            shell.managed_drafts.remove_entry(
                                &captured.account_id,
                                captured.epoch,
                                &captured.key,
                                current,
                            );
                        }
                    } else if let Some(view) = retained {
                        shell.managed_drafts.apply_view(
                            &captured.account_id,
                            captured.epoch,
                            captured.key.clone(),
                            view,
                        );
                    }
                }
            }
            return Err(error);
        }
    };
    let intent = consumed
        .state
        .consumption()
        .cloned()
        .ok_or(Fault::Pending)?;
    let pending = {
        let mut shell = state.write();
        if captured.current(&shell) {
            shell.managed_drafts.apply_view(
                &captured.account_id,
                captured.epoch,
                captured.key.clone(),
                editor.snapshot().view,
            );
            let same_visit = captured
                .thread_generation
                .is_none_or(|generation| generation == shell.thread_generation);
            let cleared = clear_consumed_at(&mut shell, &captured.key, &captured.local, same_visit);
            let pending = append_pending(&mut shell, &intent, &captured.local);
            drop(shell);
            if cleared {
                crate::view::composer::clear_delivered_field(
                    if captured.key.thread.is_some() {
                        "thread-composer"
                    } else {
                        "channel-composer"
                    },
                    &captured.field_scope,
                    &captured.local.composer.text,
                );
            }
            pending
        } else {
            None
        }
    };
    // The user action is now durably bound to this account and destination.
    // No credentials or active composer are looked up after this boundary.
    let (receipt, outcome) = match dispatch_once(&owner, &captured.binding, &intent).await {
        Ok(result) => result,
        Err(error) => {
            let mut shell = state.write();
            if captured.current(&shell)
                && let Some(pending) = pending.as_ref()
            {
                apply_outcome(
                    &mut shell,
                    pending,
                    &intent,
                    &captured.local,
                    &Outcome::Uncertain,
                    false,
                );
            }
            return Err(error);
        }
    };
    let latest = owner.load().await;
    {
        let mut shell = state.write();
        if captured.current(&shell) {
            if let Ok(latest) = latest.as_ref() {
                shell.managed_drafts.apply_view(
                    &captured.account_id,
                    captured.epoch,
                    captured.key.clone(),
                    latest.clone(),
                );
            }
            if let Some(pending) = pending.as_ref() {
                apply_outcome(
                    &mut shell,
                    pending,
                    &intent,
                    &captured.local,
                    &outcome,
                    receipt.applied,
                );
            }
        }
    }
    if receipt.applied && matches!(outcome, Outcome::Sent { .. }) {
        if captured.current(&state.peek()) {
            crate::bootstrap::persist_workspace(&state, &captured.key.team);
        }
        release_editor(state, captured, &editor).await?;
    }
    latest?;
    Ok(())
}

/// Undo only a fresh enrollment that never reached consumption. Existing owner
/// release checks refuse an outstanding operation, alternate, or changed row.
async fn abort_enrollment(
    owner: &Owner,
    enrolled_revision: u64,
    editor: Option<&Arc<Editor>>,
) -> Result<u64, Fault> {
    if let Some(editor) = editor {
        editor.release().await
    } else {
        owner
            .release(enrolled_revision)
            .await
            .map(|_| enrolled_revision)
    }
}

async fn dispatch_once(
    owner: &Owner,
    binding: &Source,
    intent: &ConsumeIntent,
) -> Result<(super::dispatch::DispatchResult, Outcome), Fault> {
    dispatch_once_with(owner, intent, |operation| async move {
        binding.execute(&operation, now_ms()).await
    })
    .await
}

async fn dispatch_once_with<F, Fut>(
    owner: &Owner,
    intent: &ConsumeIntent,
    send: F,
) -> Result<(super::dispatch::DispatchResult, Outcome), Fault>
where
    F: FnOnce(super_platinum_core::drafts::sync::Operation) -> Fut,
    Fut: std::future::Future<Output = super::network::Attempt>,
{
    // Only pure preparation CAS may repeat. The closure is never called twice.
    let mut send = Some(send);
    for _ in 0..4 {
        let view = owner.load().await?;
        let stored = view.stored.ok_or(Fault::Unmanaged)?;
        if stored.state.consumption() != Some(intent) || stored.state.pending().is_some() {
            return Err(Fault::Pending);
        }
        let mut attempt = None;
        if send.is_none() {
            return Err(Fault::Pending);
        }
        let result = owner
            .dispatch(stored.revision, |operation| {
                let attempt = &mut attempt;
                let send = send.take();
                async move {
                    let Some(send) = send else {
                        return Outcome::Uncertain;
                    };
                    let result = send(operation).await;
                    let outcome = result.outcome.clone();
                    *attempt = Some(result);
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
                return Ok((receipt, attempt.outcome));
            }
        }
    }
    Err(Fault::Stale)
}

async fn release_editor(
    mut state: Signal<ShellState>,
    captured: &Captured,
    editor: &Arc<Editor>,
) -> Result<(), Fault> {
    // Implemented by the checkpoint owner: serialized local-only handoff keeps
    // typing during release ordered with all later ordinary local writes.
    let released_revision = editor.release().await?;
    let mut shell = state.write();
    if captured.current(&shell) {
        if let Some(revision) = shell
            .managed_drafts
            .entry(&captured.account_id, &captured.key)
            .and_then(|view| view.stored.as_ref())
            .map(|stored| stored.revision)
            .filter(|revision| *revision <= released_revision)
        {
            shell.managed_drafts.remove_entry(
                &captured.account_id,
                captured.epoch,
                &captured.key,
                revision,
            );
        }
        // Keep the completed writer until its queued input is handed off.
    }
    Ok(())
}

/// The paired durable clear already happened. Mutate only the matching in-memory
/// copy; checkpointing an empty field here would race newer queued typing.
#[cfg(test)]
fn clear_consumed(shell: &mut ShellState, key: &DraftKey, expected: &LocalDraft) -> bool {
    clear_consumed_at(shell, key, expected, true)
}
fn clear_consumed_at(
    shell: &mut ShellState,
    key: &DraftKey,
    expected: &LocalDraft,
    same_visit: bool,
) -> bool {
    if shell
        .core
        .drafts
        .items
        .get(key)
        .is_some_and(|local| super::same_local(local, expected))
    {
        shell.core.drafts.items.remove(key);
    }
    if same_visit
        && shell.composer_key(target_for(key)).as_ref() == Some(key)
        && super::same_local(&current_local_for(shell, target_for(key)), expected)
    {
        if key.thread.is_some() {
            shell.core.thread_composer = Default::default();
            shell.core.thread_composer_attachments.clear();
        } else {
            shell.core.composer = Default::default();
            shell.core.composer_attachments.clear();
        }
        true
    } else {
        false
    }
}

#[path = "send_projection.rs"]
mod projection;
use projection::{append_pending, apply_outcome};
pub(crate) use projection::{
    append_pending as protected_append_pending, apply_outcome as protected_apply_outcome,
};

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "send_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "send_flow_tests.rs"]
mod flow_tests;
