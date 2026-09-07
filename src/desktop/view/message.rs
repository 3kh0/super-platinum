//! One message row, shared by the channel timeline and the thread pane.
//!
//! Slack renders the same message with the same hover toolbar wherever it
//! appears — the channel, a thread, the pane Activity opens beside its list
//! (CDP-verified against the real client). Keeping one component here is what
//! stops the surfaces from drifting apart: the thread pane used to render its
//! own stripped-down row, so a reply opened from Activity had no way to be
//! edited, deleted, or replied to.

use dioxus::prelude::*;

use super::rich::{
    attachment_embeds, message_plain_text, pending_attachment_strip, reactions_row, reply_bar,
    rich_node,
};
use crate::state::{MessageVm, ShellState};

/// Which surface a row is being rendered into.
///
/// The two differ in what a row can *do*, not in how it looks: a thread reply
/// has no "Reply" button, because it is already in the thread it would open,
/// and it carries no reply bar of its own.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowSurface {
    Timeline,
    Thread,
}

impl RowSurface {
    fn in_thread(self) -> bool {
        self == Self::Thread
    }
}

/// The hover toolbar.
///
/// Out of flow and outside `.message-meta`: a toolbar that joins the header row
/// on hover reflows — and wraps to a second line in a narrow pane — resizing
/// the message under the pointer. Compact rows get one this way too.
fn message_tools(
    mut state: Signal<ShellState>,
    message: &MessageVm,
    channel_id: &str,
    surface: RowSurface,
) -> Element {
    let can_reply = !surface.in_thread();
    if !can_reply && !message.is_own {
        // Nothing this reader can do to someone else's reply: an empty pill
        // floating over the message on hover is worse than no pill.
        return rsx! {};
    }
    rsx! {
        span { class: "message-tools",
            if can_reply {
                button {
                    onclick: {
                        let id = message.id.clone();
                        let channel = channel_id.to_owned();
                        move |_| {
                            spawn(crate::bootstrap::open_thread(state, channel.clone(), id.clone()));
                        }
                    },
                    "Reply"
                }
            }
            if message.is_own {
                button {
                    onclick: {
                        let channel = channel_id.to_owned();
                        let ts = message.id.clone();
                        let text = message_plain_text(message);
                        move |_| state.write().start_edit(channel.clone(), ts.clone(), text.clone())
                    },
                    "Edit"
                }
                button {
                    onclick: {
                        let channel = channel_id.to_owned();
                        let ts = message.id.clone();
                        move |_| { spawn(crate::bootstrap::delete_message(state, channel.clone(), ts.clone())); }
                    },
                    "Delete"
                }
            }
        }
    }
}

/// The author line, with the hover card and profile pane wired to the name.
fn message_meta(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
) -> Element {
    rsx! {
        div { class: "message-meta",
            strong {
                onmouseenter: {
                    let user = message.user_id.clone();
                    move |event: MouseEvent| {
                        if let Some(user) = user.clone() {
                            let point = event.data().client_coordinates();
                            crate::profile::show_profile_hover(state, user, point.x, point.y);
                        }
                    }
                },
                onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
                onclick: {
                    let user = message.user_id.clone();
                    move |_| {
                        if let Some(user) = user.clone() {
                            state.write().profile_hover = None;
                            spawn(crate::bootstrap::open_profile(state, user));
                        }
                    }
                },
                "{message.author}"
            }
            if message.is_app { span { class: "app-badge", "APP" } }
            time { "{message.timestamp}" }
            if message.edited { span { "(edited)" } }
            if message.pending {
                span { class: "pending-label",
                    if snapshot.pending_attachments_for_message(&message.id).is_some() {
                        "Uploading…"
                    } else {
                        "Sending…"
                    }
                }
            }
        }
    }
}

fn message_avatar(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
) -> Element {
    rsx! {
        button {
            class: "avatar",
            onmouseenter: {
                let user = message.user_id.clone();
                move |event: MouseEvent| {
                    if let Some(user) = user.clone() {
                        let point = event.data().client_coordinates();
                        crate::profile::show_profile_hover(state, user, point.x, point.y);
                    }
                }
            },
            onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
            onclick: {
                let user = message.user_id.clone();
                move |_| {
                    if let Some(user) = user.clone() {
                        state.write().profile_hover = None;
                        spawn(crate::bootstrap::open_profile(state, user));
                    }
                }
            },
            // Initials until the bytes land: an `img` with nothing behind it
            // paints the platform's broken-image icon.
            if let Some(avatar) = message.avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "{message.author}" }
            } else {
                "{message.avatar_initials}"
            }
            if let Some(team) = &message.external_team {
                span {
                    class: "avatar-team-icon",
                    title: "{team.name}",
                    "aria-label": "{team.name}",
                    if let Some(icon) = team.icon.as_ref().filter(|icon| snapshot.media.is_ready(icon)) {
                        img { src: "{icon.uri_at(snapshot.media_epoch)}", alt: "" }
                    } else {
                        "{team.initials}"
                    }
                }
            }
        }
    }
}

/// Renders one message.
///
/// `index` is the row's position in the timeline, used by the scroll measurer;
/// thread replies are not part of that window and pass `None`.
pub(crate) fn message_row(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
    channel_id: &str,
    index: Option<usize>,
    surface: RowSurface,
) -> Element {
    let in_thread = surface.in_thread();
    let compact = message.compact && !in_thread;
    let class = if in_thread {
        "message thread-message"
    } else if snapshot.message_arrivals.contains_key(&message.id) {
        if compact {
            "message compact entering"
        } else {
            "message entering"
        }
    } else if compact {
        "message compact"
    } else if message.pending {
        "message pending"
    } else {
        "message"
    };
    let editing = snapshot
        .core
        .editing
        .as_ref()
        .is_some_and(|(channel, ts)| channel == channel_id && ts == &message.id);
    rsx! {
        article {
            class: "{class}",
            "data-message-index": if let Some(index) = index { "{index}" },
            "data-message-id": "{message.id}",
            if compact {
                div { class: "compact-gutter", title: "{message.timestamp}", "{message.timestamp}" }
            } else {
                {message_avatar(state, snapshot, message)}
            }
            {message_tools(state, message, channel_id, surface)}
            div { class: "message-content",
                if !compact {
                    {message_meta(state, snapshot, message)}
                }
                if editing {
                    div { class: "message-editor",
                        textarea {
                            initial_value: "{snapshot.core.edit_composer.text}",
                            oninput: move |event| {
                                let value = event.value();
                                let end = value.len();
                                let mut shell = state.write();
                                shell.core.edit_composer.text = value;
                                shell.core.edit_composer.set_selection(end, end);
                            }
                        }
                        button {
                            onclick: move |_| { spawn(crate::bootstrap::save_edit(state)); },
                            "Save"
                        }
                        button { onclick: move |_| state.write().cancel_edit(), "Cancel" }
                    }
                } else {
                    div { class: "message-body",
                        for node in &message.body { {rich_node(node, snapshot.media_epoch, state)} }
                    }
                    if !message.attachments.is_empty() {
                        {attachment_embeds(state, snapshot.media_epoch, &message.attachments)}
                    }
                    if let Some(attachments) = snapshot.pending_attachments_for_message(&message.id) {
                        {pending_attachment_strip(attachments, snapshot.upload_ui_epoch)}
                    }
                }
                if !message.reactions.is_empty() {
                    div { class: "message-actions",
                        {reactions_row(state, &snapshot.media, snapshot.media_epoch, channel_id, message)}
                    }
                }
                if !in_thread && message.reply_count > 0 {
                    {reply_bar(state, &snapshot.media, snapshot.media_epoch, channel_id, message)}
                }
            }
        }
    }
}
