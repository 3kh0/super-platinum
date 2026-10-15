//! Real ShellState replacement; retained Host/Account stay outside it. No native
//! field setter or storage write is used to manufacture restoration evidence.
use super::*;
pub(crate) fn replace_shell(
    mut state: Signal<ShellState>,
    b: &Binding,
    kind: &str,
) -> Result<(), String> {
    if !["owner-replacement-channel", "failed-managed-write-channel"].contains(&b.scenario.as_str())
        || !["same-epoch", "away", "return"].contains(&kind)
    {
        return Err("Wrong adoption replacement case".into());
    }
    let (account, epoch, view, before, channel, thread, channel_files, thread_files, thread_root) = {
        let s = state.peek();
        (
            s.core.active_account.clone(),
            s.core.account_epoch,
            s.draft_writes.view_id,
            json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"viewId":s.draft_writes.view_id,
                "channelDraft":s.core.drafts.items.get(&key(ComposerTarget::Channel)),"threadDraft":s.core.drafts.items.get(&key(ComposerTarget::Thread)),"channel":s.core.composer,"thread":s.core.thread_composer}),
            s.core.composer.clone(),
            s.core.thread_composer.clone(),
            s.core.composer_attachments.clone(),
            s.core.thread_composer_attachments.clone(),
            s.thread_root.clone(),
        )
    };
    if kind != "return" && !b.current(&state.peek()) {
        return Err("Replacement source owner changed".into());
    }
    if kind == "return" && account.as_deref() != Some("00000000-0000-4000-8000-000000000352") {
        return Err("Return source owner changed".into());
    }
    let mut next = ShellState::fixture(crate::media::MediaRegistry::default());
    apply(&mut next);
    next.core.composer = channel;
    next.core.thread_composer = thread;
    next.core.composer_attachments = channel_files;
    next.core.thread_composer_attachments = thread_files;
    next.thread_root = thread_root;
    next.main_view = crate::state::MainView::Home;
    next.core.account_epoch = if kind == "same-epoch" {
        epoch
    } else {
        epoch.checked_add(1).ok_or("Epoch exhausted")?
    };
    if kind == "away" {
        next.core.active_account = Some("00000000-0000-4000-8000-000000000352".into());
    }
    let channel = next
        .core
        .active_channel
        .clone()
        .ok_or("Replacement channel missing")?;
    next.core.pending_scroll_to = Some((
        channel,
        super_platinum_core::domain::PendingScrollTarget::Latest,
    ));
    next.stick_to_bottom = true;
    next.reset_timeline_window();
    if next.draft_writes.view_id == view {
        return Err("Replacement reused view identity".into());
    }
    if kind == "away" {
        b.host.lock().unwrap().retire_selection();
    } else {
        let selected = b
            .host
            .lock()
            .unwrap()
            .select(&next, b.account.clone())
            .map_err(err)?;
        *b.selected.lock().unwrap() = selected;
        *b.selected_view.lock().unwrap() = (next.core.account_epoch, next.draft_writes.view_id);
    }
    {
        // All fallible construction/selection work completed above. Move the
        // whole book, including private pending changes and channel/thread
        // bindings, only at the final synchronous state replacement.
        let mut current = state.write();
        next.core.drafts = std::mem::take(&mut current.core.drafts);
        *current = next;
    }
    b.record(json!({"kind":"shell-replaced","mode":kind,"before":before,"afterEpoch":state.peek().core.account_epoch,"afterViewId":state.peek().draft_writes.view_id,"afterAccount":state.peek().core.active_account}));
    Ok(())
}
