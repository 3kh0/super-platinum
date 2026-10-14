//! App-root registry bindings for the attachment-only dialog and retained rows.
use crate::{
    scheduled::attachments::{Lease, Operations, Owner, native},
    state::ShellState,
};
use dioxus::prelude::*;

pub(crate) fn trigger_id(id: &str) -> String {
    format!("draft-attachments-{id}")
}

pub(crate) fn open(
    mut state: Signal<ShellState>,
    mut operations: Signal<Operations>,
    id: &str,
) -> Option<Lease> {
    let owner = Owner::capture(&state.peek(), id)?;
    let draft = state.peek().core.scheduled.items.get(id)?.clone();
    match operations.write().open(owner, draft) {
        Ok(lease) => Some(lease),
        Err(_) => {
            state
                .write()
                .show_toast("Close this draft's editor or review its pending action first.");
            None
        }
    }
}

pub(crate) fn text_editor(
    mut state: Signal<ShellState>,
    mut operations: Signal<Operations>,
    id: String,
    schedule: bool,
) -> Option<super::super::EditorVisit> {
    let owner = Owner::capture(&state.peek(), &id)?;
    let lease = match operations.write().reserve_other(owner) {
        Ok(lease) => lease,
        Err(_) => {
            state
                .write()
                .show_toast("This draft already has an editor or an action awaiting confirmation.");
            return None;
        }
    };
    Some(super::super::EditorVisit {
        id,
        schedule,
        lease: Some(lease),
        epoch: state.peek().core.account_epoch,
    })
}

fn admitted(state: Signal<ShellState>, lease: &Lease) -> bool {
    native::admitted(state, lease)
}

fn restore_focus(state: Signal<ShellState>, lease: &Lease) {
    if !lease.matches_shell(&state.peek()) {
        return;
    }
    let id = serde_json::to_string(&trigger_id(lease.draft_id())).unwrap();
    dioxus::document::eval(&format!(
        "requestAnimationFrame(()=>document.getElementById({id})?.focus());"
    ));
}

#[component]
pub(crate) fn Editor(
    state: Signal<ShellState>,
    mut operations: Signal<Operations>,
    lease: Lease,
    onclose: EventHandler,
) -> Element {
    let cleanup = lease.clone();
    use_drop(move || {
        crate::scheduled::attachments::cleanup::run(
            operations,
            cleanup.clone(),
            crate::scheduled::attachments::cleanup::Action::Cancel,
        );
    });
    let metadata = lease.clone();
    use_effect(move || native::load_metadata(state, operations, metadata.clone()));
    let Ok(snapshot) = operations.read().snapshot(&lease) else {
        return rsx! {};
    };
    let remove = lease.clone();
    let restore = lease.clone();
    let preview = lease.clone();
    let save = lease.clone();
    let review = lease.clone();
    let cancel = lease.clone();
    let close = lease.clone();
    rsx! {
        super::AttachmentDialog {
            state, lease, snapshot,
            onremove: move |id: String| { if admitted(state, &remove) { let _ = operations.write().remove(&remove, &id); } },
            onrestore: move |id: String| { if admitted(state, &restore) { let _ = operations.write().restore(&restore, &id); } },
            onpreview: move |id| native::preview(state, operations, preview.clone(), id),
            onsave: move |_| native::save(state, operations, save.clone()),
            onreview: move |_| native::review(state, operations, review.clone()),
            oncancel: move |_| { if operations.write().cancel(&cancel).is_ok() { onclose.call(()); restore_focus(state, &cancel); } },
            onclose: move |_| { onclose.call(()); restore_focus(state, &close); },
        }
    }
}

#[component]
pub(crate) fn Inventory(
    state: Signal<ShellState>,
    operations: Signal<Operations>,
    editor_open: bool,
    onopen: EventHandler<Lease>,
) -> Element {
    let shell = state.read();
    let registry = operations.read();
    let attachments = registry.retained(&shell);
    let mutations = registry.other_retained(&shell);
    rsx! {
        for lease in attachments {
            {let id = lease.draft_id().to_owned();
            rsx! { article { class: "scheduled-item",
                div { class: "scheduled-meta", strong { "Attachment changes" } span { "{id}" } }
                p { class: "scheduled-hint", "This edit is retained until its changes are confirmed or canceled before sending." }
                div { class: "scheduled-actions", button { disabled: editor_open, onclick: move |_| { if lease.matches_shell(&state.peek()) { onopen.call(lease.clone()); } }, "Review attachment changes" } }
            }}}
        }
        for lease in mutations {
            if let Ok(snapshot) = registry.other_snapshot(&lease) {
                {let review = lease.clone(); rsx! { super::recovery::MutationRecovery {
                    state, lease, snapshot, onreview: move |_| native::review_other(state, operations, review.clone()),
                }}}
            }
        }
    }
}
