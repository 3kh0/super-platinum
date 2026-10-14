//! A separate attachment-only dialog. It never mounts/reads the rich text editor.
//! Parent callbacks use the retained app-root Operations registry and exact Lease.
use crate::{
    icons::{Icon, icon},
    scheduled::attachments::{Lease, Snapshot},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::{scheduled::edit::EditPhase, slack::models::Message};
pub(crate) mod host;
pub(crate) mod recovery;

#[component]
pub(crate) fn AttachmentDialog(
    state: Signal<ShellState>,
    lease: Lease,
    snapshot: Snapshot,
    onremove: EventHandler<String>,
    onrestore: EventHandler<String>,
    onpreview: EventHandler<String>,
    onsave: EventHandler,
    onreview: EventHandler,
    oncancel: EventHandler,
    onclose: EventHandler,
) -> Element {
    let shell = state.read();
    if !lease.matches_shell(&shell) {
        return rsx! {};
    }
    let editing = snapshot.phase == EditPhase::Editing;
    let retained = matches!(snapshot.phase, EditPhase::InFlight | EditPhase::Uncertain);
    let busy = matches!(snapshot.phase, EditPhase::Prepared | EditPhase::InFlight);
    let destination = super::destination_label(&shell, &snapshot.destinations);
    let message = Message {
        blocks: snapshot.blocks.clone(),
        ..Default::default()
    };
    let preview = shell
        .core
        .active_team
        .as_ref()
        .and_then(|team| shell.core.workspaces.get(team))
        .map(|workspace| {
            crate::message_vm::message_vm_with_pending(workspace, &message, &shell.media, false)
        });
    let close = move |_| {
        if retained {
            onclose.call(());
        } else {
            oncancel.call(());
        }
    };
    rsx! {
        div { class: "scrim", onclick: close }
        section {
            id: "draft-attachments-dialog", class: "modal schedule-dialog", role: "dialog", "aria-modal": "true",
            "aria-labelledby": "draft-attachments-title", "aria-busy": busy,
            onmounted: move |_| super::super::channel_details::focus_dialog("draft-attachments-dialog"),
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.stop_propagation();
                    if retained { onclose.call(()); } else { oncancel.call(()); }
                }
            },
            header {
                h2 { id: "draft-attachments-title", "Manage attachments" }
                button { "aria-label": "Close attachment editor", onclick: close,
                    {icon(Icon::Close, "icon")}
                }
            }
            p { class: "scheduled-hint", "To {destination}" }
            if snapshot.post_at > 0 {
                p { class: "scheduled-hint",
                    "Scheduled for {super_platinum_core::state::format_ts_date_label(&snapshot.post_at.to_string())} at {super_platinum_core::state::format_ts_hm(&snapshot.post_at.to_string())}"
                }
            } else {
                p { class: "scheduled-hint", "Not scheduled" }
            }
            div { class: "scheduled-content", "aria-label": "Message, read only",
                if let Some(preview) = preview {
                    for node in &preview.body {
                        {super::super::rich::rich_node(node, shell.media_epoch, state)}
                    }
                }
            }
            p { class: "scheduled-hint", "Removing an attachment changes this draft. The file stays in Slack." }
            div { role: "list", "aria-label": "Draft attachments",
                for row in snapshot.rows {
                    {let remove_id = row.id.clone(); let restore_id = row.id.clone(); let preview_id = row.id.clone();
                    rsx! {
                        article { key: "{row.id}", class: "scheduled-item", role: "listitem",
                            div { class: "scheduled-content", strong { "{row.title}" } }
                            p { class: "scheduled-hint", "{row.detail}" }
                            if row.removed { p { class: "scheduled-hint", "Will be removed from this draft" } }
                            div { class: "scheduled-actions",
                                button { onclick: move |_| onpreview.call(preview_id.clone()), "Preview" }
                                if row.removed {
                                    button { disabled: !editing, onclick: move |_| onrestore.call(restore_id.clone()), "Undo removal" }
                                } else {
                                    button { disabled: !editing, onclick: move |_| onremove.call(remove_id.clone()), "Remove from draft" }
                                }
                            }
                        }
                    }}
                }
            }
            if let Some(error) = snapshot.error { p { class: "later-error", role: "alert", "{error}" } }
            match snapshot.phase {
                EditPhase::Prepared => rsx! { p { class: "scheduled-hint", role: "status", "Checking the current draft..." } },
                EditPhase::InFlight => rsx! { p { class: "scheduled-hint", role: "status", "Saving attachment changes. You can close this window while the result is checked." } },
                EditPhase::Uncertain => rsx! { p { class: "later-error", role: "alert", "The save result is unknown. Review the current draft before making more changes. Your changes will not be sent again." } },
                EditPhase::Conflict => rsx! { p { class: "later-error", role: "alert", "The draft changed in Slack or could not be verified. No attachment update was sent. Close and reopen to review the current draft." } },
                EditPhase::Confirmed => rsx! { p { class: "scheduled-hint", role: "status", "The current draft has these attachment changes." } },
                EditPhase::Editing => rsx! {},
            }
            footer {
                button { onclick: close, if retained || snapshot.phase == EditPhase::Confirmed { "Close" } else { "Cancel" } }
                if snapshot.phase == EditPhase::Uncertain {
                    button { class: "primary", disabled: snapshot.reviewing || snapshot.review_cooling_down, onclick: move |_| onreview.call(()), if snapshot.reviewing { "Checking..." } else { "Review current draft" } }
                } else if editing || busy {
                    button { class: "primary", disabled: !editing || !snapshot.changed || snapshot.review_cooling_down, onclick: move |_| onsave.call(()), if busy { "Saving..." } else { "Save changes" } }
                }
            }
        }
    }
}
