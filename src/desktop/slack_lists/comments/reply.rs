//! One captured rich draft, one durable attempt, one network dispatch.
use super::*;
use crate::view::slack_lists::comments::bridge;
use super_platinum_core::{
    slack::{api, transport::ListCommentReplyFailure},
    slack_lists::comments::{
        ReplyIntent, ReplyPolicy, ReplyRecord,
        journal::{Inventory, Resolution},
    },
};
mod policy;
mod resolution;
pub(super) use resolution::{Terminal, observe, retry_resolution};

fn binding_target<'a>(shell: &'a ShellState, binding: &draft::Binding) -> Option<&'a Target> {
    binding
        .current(shell)
        .then(|| shell.slack_lists.comments.entry.as_ref().map(|e| &e.target))
        .flatten()
}
fn inputs(
    shell: &ShellState,
    binding: &draft::Binding,
) -> Result<
    (
        super_platinum_core::slack::SlackClient,
        super_platinum_core::config::WorkspaceSession,
    ),
    Error,
> {
    if !binding.current(shell) {
        return Err(unavailable());
    }
    if crate::fixture::is_fixture() {
        return Ok((shell.core.client.clone(), fixture::compose::session(shell)?));
    }
    let ctx = binding.owner.scope.request(shell).ok_or_else(unavailable)?;
    Ok((ctx.client, ctx.session))
}
pub(crate) fn send(mut state: Signal<ShellState>, binding: draft::Binding) {
    if !draft::presentation(&state.peek())
        .is_some_and(|p| p.binding.scope() == binding.scope() && p.can_send)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        // No Rust render before the actual original node is sealed.
        let lease = match bridge::capture(state, &binding).await {
            Ok(v) => v,
            Err(_) => {
                fail(
                    state,
                    &binding,
                    "The original draft could not be captured. Nothing was sent.",
                );
                return;
            }
        };
        if !binding.current(&state.peek()) {
            return;
        }
        if let Some(e) = draft::entry_mut(&mut state.write(), &binding.id) {
            if e.busy {
                return;
            }
            e.busy = true;
            e.error = None;
        }
        let result = prepare(state, &binding, lease.blocks()).await;
        match result {
            Ok((disk, record, inventory, intent, witness, sequence)) => {
                // Durable lock precedes transport; a restored record cannot recreate intent.
                let copy = record.clone();
                let expected = inventory;
                let locked = disk.work(move |j| j.begin(&expected, copy)).await;
                let Ok(locked) = locked else {
                    fail(
                        state,
                        &binding,
                        "The send receipt could not be saved. Nothing was sent; retry local recovery.",
                    );
                    return;
                };
                if let Some(e) = draft::entry_mut(&mut state.write(), &binding.id) {
                    draft::adopt(e, locked);
                }
                let result = if !dispatch_current(&state.peek(), &binding, sequence, &witness) {
                    Err(ListCommentReplyFailure::NotRequested(unavailable()))
                } else {
                    let prepared = {
                        inputs(&state.peek(), &binding)
                            .and_then(|(c, w)| api::list_record_comment_reply(&c, &w, &intent))
                    };
                    match prepared {
                        Err(e) => Err(ListCommentReplyFailure::NotRequested(e)),
                        Ok(request) => {
                            if crate::fixture::is_fixture() {
                                fixture::compose::post(state, &request, intent).await
                            } else {
                                let context = { binding.owner.scope.request(&state.peek()) };
                                match context {
                                    Some(ctx) => {
                                        ctx.transport
                                            .execute_list_comment_reply(request, intent, || {
                                                dispatch_current(
                                                    &state.peek(),
                                                    &binding,
                                                    sequence,
                                                    &witness,
                                                )
                                            })
                                            .await
                                    }
                                    None => {
                                        Err(ListCommentReplyFailure::NotRequested(unavailable()))
                                    }
                                }
                            }
                        }
                    }
                };
                let cleared =
                    resolution::settle(state, &binding, &disk, &record, sequence, result).await;
                if cleared {
                    let finished = lease.finish().await.is_ok();
                    if finished {
                        draft::rotate(&mut state.write(), &binding);
                    }
                } else {
                    let _ = lease.release().await;
                }
            }
            Err(error) => {
                rate(state, &binding, &error);
                fail(
                    state,
                    &binding,
                    "The item, permissions or saved draft could not be checked. Nothing was sent; your draft is retained.",
                );
                let _ = lease.release().await;
            }
        }
        if let Some(e) = draft::entry_mut(&mut state.write(), &binding.id) {
            e.busy = false;
        }
    });
}
async fn prepare(
    mut state: Signal<ShellState>,
    binding: &draft::Binding,
    blocks: &[Value],
) -> Result<
    (
        storage::Disk,
        ReplyRecord,
        Inventory,
        ReplyIntent,
        policy::Witness,
        u64,
    ),
    Error,
> {
    draft::flush(state, binding, blocks).await?;
    if !binding.current(&state.peek()) {
        return Err(unavailable());
    }
    let (owner, serial, target, _) =
        controller::begin_preflight(&mut state.write()).ok_or_else(unavailable)?;
    let acquired = async {
        let loaded = network::acquire(state, &owner, serial, &target, None).await?;
        if loaded.page.target().root_ts() != target.root_ts() {
            return Err(unavailable());
        }
        let (policy, witness) =
            policy::acquire(state, binding, serial, loaded.page.target()).await?;
        let root = loaded.page.root().ok_or_else(unavailable)?;
        let shell = state.peek();
        if !binding.current(&shell) {
            return Err(unavailable());
        }
        let e = draft::entry(&shell, &binding.id).ok_or_else(unavailable)?;
        if e.blocks != blocks || e.inventory.as_ref().is_none_or(|i| i.attempt().is_some()) {
            return Err(unavailable());
        }
        // Native monotonic local timestamp, independent of selected conversation.
        let (_, client_ts) = ShellState::optimistic_identity("list-comment");
        let intent = ReplyIntent::capture_rich(
            &owner.scope.team,
            &owner.scope.stable_owner().2,
            loaded.page.target(),
            root,
            &policy,
            blocks.to_vec(),
            &uuid::Uuid::new_v4().to_string(),
            &client_ts,
        )
        .map_err(|e| Error::Api(e.to_string()))?;
        Ok((
            e.disk.clone(),
            intent.record().clone(),
            e.inventory.clone().unwrap(),
            intent,
            witness,
            e.sequence,
        ))
    }
    .await;
    if owner.reading(&state.peek(), serial) {
        state
            .write()
            .slack_lists
            .comments
            .entry
            .as_mut()
            .unwrap()
            .loading = false;
    }
    acquired
}
fn dispatch_current(
    shell: &ShellState,
    binding: &draft::Binding,
    sequence: u64,
    witness: &policy::Witness,
) -> bool {
    binding.current(shell)
        && witness.current(shell, binding)
        && cooldown(shell) <= now()
        && draft::entry(shell, &binding.id).is_some_and(|e| {
            e.busy
                && e.sequence == sequence
                && !e.composing
                && e.inventory.as_ref().is_some_and(|i| i.attempt().is_some())
        })
}
fn fail(mut state: Signal<ShellState>, binding: &draft::Binding, message: &str) {
    if let Some(e) = draft::entry_mut(&mut state.write(), &binding.id) {
        e.busy = false;
        e.error = Some(message.into());
    }
}
fn rate(mut state: Signal<ShellState>, binding: &draft::Binding, error: &Error) {
    if binding.owner.scope.stable_current(&state.peek()) {
        let deadline = match error {
            Error::RateLimited {
                retry_after_secs: None,
            }
            | Error::HttpStatus {
                status: 429,
                retry_after_secs: None,
            } => now().saturating_add(30),
            _ => retry_at(error),
        }
        .max(cooldown(&state.peek()));
        if deadline > now() {
            let mut shell = state.write();
            shell.slack_lists.comments.cooldown =
                Some((binding.owner.scope.receipt_only(), deadline));
            if let Some(e) = shell
                .slack_lists
                .comments
                .entry
                .as_mut()
                .filter(|e| e.owner.serial == binding.owner.serial)
            {
                e.retry_at = deadline;
            }
            drop(shell);
            controller::arm_retry(state, binding.owner.serial);
        }
    }
}
