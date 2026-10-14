//! Persistent per-draft explanation and explicit read-only review for earlier
//! scheduled mutations. There is intentionally no force-send or force-delete.
use crate::{
    scheduled::attachments::{
        Lease,
        other::{Action, Phase, Snapshot},
    },
    state::ShellState,
};
use dioxus::prelude::*;

#[component]
pub(crate) fn MutationRecovery(
    state: Signal<ShellState>,
    lease: Lease,
    snapshot: Snapshot,
    onreview: EventHandler,
) -> Element {
    if !lease.matches_shell(&state.read()) {
        return rsx! {};
    }
    let title = match snapshot.action {
        Action::Edit => "Draft edit",
        Action::Unschedule => "Schedule change",
        Action::Send => "Send message",
        Action::Delete => "Delete draft",
    };
    rsx! {
        article { class: "scheduled-item", "aria-label": "Unconfirmed draft action",
            div { class: "scheduled-meta", strong { "{title}" } span { "{lease.draft_id()}" } }
            if snapshot.phase == Phase::Running {
                p { class: "scheduled-hint", role: "status", "The action is still being checked. Closing its editor does not cancel a request already sent." }
            } else {
                p { class: "later-error", role: "alert", "The result could not be confirmed. This draft is reserved so the action cannot be sent again." }
                if snapshot.can_confirm_by_readback {
                    p { class: "scheduled-hint", "Review checks whether Slack has the complete saved changes. It does not send the changes again." }
                } else {
                    p { class: "scheduled-hint", "Review the message in Slack. A missing draft does not prove that it was sent or deleted, so this app cannot unlock the action from draft presence alone." }
                }
                if snapshot.cooling_down {
                    p { class: "scheduled-hint", role: "status", "Slack asked us to wait before checking again." }
                }
                div { class: "scheduled-actions",
                    button { disabled: snapshot.reviewing || snapshot.cooling_down, onclick: move |_| onreview.call(()), if snapshot.reviewing { "Checking..." } else { "Review current draft" } }
                }
            }
        }
    }
}
