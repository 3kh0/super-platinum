use super::*;
/// Kept until the exact local settlement commits; never used to rebuild a send.
#[derive(Clone)]
pub(in crate::slack_lists::comments) struct Terminal {
    record: ReplyRecord,
    resolution: Resolution,
    status: String,
    sequence: u64,
    owner: Owner,
    reveal: bool,
}
pub(super) async fn settle(
    state: Signal<ShellState>,
    binding: &draft::Binding,
    _disk: &storage::Disk,
    record: &ReplyRecord,
    sequence: u64,
    result: Result<
        super_platinum_core::slack_lists::comments::ReplyAccepted,
        ListCommentReplyFailure,
    >,
) -> bool {
    let (resolution, status) = match result {
        Ok(accepted) => (
            Resolution::Accepted(serde_json::to_value(accepted.message()).unwrap()),
            "Comment sent.",
        ),
        Err(ListCommentReplyFailure::NotRequested(error)) => {
            rate(state, binding, &error);
            (
                Resolution::NotRequested,
                "Not sent. Your draft is retained; review before sending again.",
            )
        }
        Err(ListCommentReplyFailure::Rejected(raw)) => (
            Resolution::Rejected(raw),
            "Slack rejected the comment. Your draft is retained.",
        ),
        Err(ListCommentReplyFailure::Unconfirmed { error, receipt }) => {
            rate(state, binding, &error);
            (
                Resolution::Uncertain(receipt),
                "Delivery unconfirmed. Refresh comments to check. This attempt will not be sent again.",
            )
        }
    };
    queue(
        state,
        &binding.id,
        Terminal {
            record: record.clone(),
            resolution,
            status: status.into(),
            sequence,
            owner: binding.owner.clone(),
            reveal: true,
        },
    );
    resolve(state, &binding.id).await
}
fn queue(mut state: Signal<ShellState>, id: &str, terminal: Terminal) {
    if let Some(e) = draft::entry_mut(&mut state.write(), id) {
        e.terminal = Some(terminal);
    }
}
/// Caller seals a visible native editor first. Detached original tails are retained
/// independently and cannot be mistaken for a newly mounted editor.
async fn resolve(mut state: Signal<ShellState>, id: &str) -> bool {
    let work = {
        let shell = state.peek();
        let Some(e) = draft::entry(&shell, id) else {
            return false;
        };
        let Some(t) = e.terminal.clone() else {
            return false;
        };
        (e.disk.clone(), t)
    };
    let (disk, terminal) = work;
    let scope = disk.scope.clone();
    let record = terminal.record.clone();
    let resolution = terminal.resolution.clone();
    let result = disk
        .work(move |j| j.resolve(&scope, &record, resolution))
        .await;
    apply_result(&mut state.write(), id, &terminal, result)
}
fn apply_result(
    shell: &mut ShellState,
    id: &str,
    terminal: &Terminal,
    result: Result<Inventory, Error>,
) -> bool {
    let settled = result.is_ok();
    let Some(entry) = draft::entry_mut(shell, id) else {
        return false;
    };
    let cleared = apply(entry, terminal, result);
    if settled
        && terminal.reveal
        && let Resolution::Accepted(raw) = &terminal.resolution
    {
        accepted::publish(shell, &terminal.owner, &terminal.record, raw);
    }
    cleared
}
fn apply(e: &mut draft::Draft, terminal: &Terminal, result: Result<Inventory, Error>) -> bool {
    let mut cleared = false;
    match result {
        Ok(inventory)=>{
            if matches!(terminal.resolution,Resolution::Accepted(_))&&e.sequence==terminal.sequence&&e.blocks==terminal.record.blocks()&&inventory.blocks().is_empty(){
                e.retired_blocks=Some(e.blocks.clone());e.blocks.clear();e.capture=inventory.capture().into();e.sequence+=1;cleared=true;
            }
            draft::adopt(e,inventory);e.terminal=None;e.status=Some(terminal.status.clone());e.error=None;
        },Err(_)=>e.error=Some("The delivery receipt could not be saved. This attempt remains locked; retry local recovery.".into())
    }
    cleared
}

pub(in crate::slack_lists::comments) fn retry_resolution(
    mut state: Signal<ShellState>,
    id: String,
) {
    if draft::entry(&state.peek(), &id).is_none_or(|e| e.busy || e.terminal.is_none()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let binding = {
            let shell = state.peek();
            shell
                .slack_lists
                .comments
                .drafts
                .active
                .as_ref()
                .filter(|b| b.id == id && b.visible_mount(&shell))
                .cloned()
        };
        let native = draft::entry(&state.peek(), &id).is_some_and(|e| e.native.is_some());
        let lease = if native {
            let Some(binding) = binding.as_ref() else {
                return;
            };
            match bridge::capture(state, binding).await {
                Ok(lease) => Some(lease),
                Err(_) => {
                    fail(
                        state,
                        binding,
                        "Finish composing or close the link editor before checking delivery. Your draft is retained.",
                    );
                    return;
                }
            }
        } else {
            None
        };
        if let Some(e) = draft::entry_mut(&mut state.write(), &id) {
            e.error = None;
            e.busy = true;
        }
        // Capture may have found newer native text. Persist it before resolving the old
        // attempt so the journal cannot clear it, even after process recovery.
        if let (Some(binding), Some(lease)) = (binding.as_ref(), lease.as_ref()) {
            if draft::flush(state, binding, lease.blocks()).await.is_err() {
                if let Some(e) = draft::entry_mut(&mut state.write(), &id) {
                    e.busy = false;
                }
                return;
            }
        }
        if lease.is_none() {
            let blocks = {
                let shell = state.peek();
                draft::entry(&shell, &id).map(|e| e.blocks.clone())
            };
            if let Some(blocks) = blocks {
                if draft::flush_id(state, &id, &blocks).await.is_err() {
                    if let Some(e) = draft::entry_mut(&mut state.write(), &id) {
                        e.busy = false;
                    }
                    return;
                }
            }
        }
        let cleared = resolve(state, &id).await;
        if let Some(lease) = lease {
            if cleared {
                if lease.finish().await.is_ok() {
                    if let Some(binding) = binding.as_ref() {
                        draft::rotate(&mut state.write(), binding);
                    }
                }
            } else {
                let _ = lease.release().await;
            }
        }
        if let Some(e) = draft::entry_mut(&mut state.write(), &id) {
            e.busy = false;
        }
    });
}
pub(in crate::slack_lists::comments) fn observe(
    state: Signal<ShellState>,
    owner: &Owner,
    page: &Page,
) {
    let work = {
        let shell = state.peek();
        if !owner.active(&shell) {
            return;
        }
        shell
            .slack_lists
            .comments
            .drafts
            .entries
            .iter()
            .filter_map(|e| {
                if e.disk.account != shell.core.active_account.clone().unwrap_or_default()
                    || e.busy
                    || e.terminal.is_some()
                {
                    return None;
                }
                let attempt = e.inventory.as_ref()?.attempt()?;
                let message = attempt.record().observed(page).ok()??;
                let raw = serde_json::to_value(message).ok()?;
                Some((
                    e.id.clone(),
                    Terminal {
                        record: attempt.record().clone(),
                        resolution: Resolution::Accepted(raw),
                        status: "Comment confirmed by the refreshed thread.".into(),
                        sequence: e.sequence,
                        owner: owner.clone(),
                        reveal: false,
                    },
                ))
            })
            .collect::<Vec<_>>()
    };
    for (id, terminal) in work {
        queue(state, &id, terminal);
        retry_resolution(state, id);
    }
}

#[cfg(test)]
mod tests;
