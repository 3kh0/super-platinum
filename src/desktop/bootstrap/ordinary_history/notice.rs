use super::*;
use dioxus::prelude::*;
pub(crate) fn render(state: Signal<ShellState>, s: &ShellState) -> Element {
    if unread_owned(s) {
        return rsx! {};
    }
    let (Some(team), Some(channel)) = (&s.core.active_team, &s.core.active_channel) else {
        return rsx! {};
    };
    let Some(e) = entry(s, team, channel) else {
        return rsx! {};
    };
    let empty = s
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(channel))
        .is_none_or(|b| b.messages.is_empty());
    if e.loading || (!e.error && !e.paused && !(empty && e.pages.has_more())) {
        return rsx! {};
    }
    let restart = e.restart_required;
    let copy = if restart {
        "More history could not be loaded. Reload to continue."
    } else if e.error {
        if e.initial {
            "Conversation history could not be loaded."
        } else {
            "Older messages could not be loaded."
        }
    } else {
        "More conversation history is available."
    };
    let label = if restart {
        "Reload"
    } else if e.error {
        "Retry"
    } else {
        "Continue"
    };
    rsx! { div {class:"unread-hold",role:"status",id:"ordinary-history-notice",
        span {"{copy}"}
        button {class:"btn-subtle",id:"ordinary-history-continue",onclick:move |_|{if restart {spawn(crate::bootstrap::refresh_selected_channel(state));} else {spawn(resume(state));}},"{label}"}
    }}
}
