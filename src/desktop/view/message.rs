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
    if message.pending {
        return rsx! {};
    }
    let pinned = crate::pins::is_pinned(&state.read(), channel_id, message);
    let pin_busy = crate::pins::visible_state(&state.read())
        .is_some_and(|p| p.is_busy(channel_id, &message.ts));
    let saved_key = super_platinum_core::later::SavedKey::message(channel_id, &message.id);
    let saved = crate::later::visible_state(&state.read())
        .is_some_and(|later| later.items.contains_key(&saved_key));
    let saving = crate::later::visible_state(&state.read())
        .is_some_and(|later| later.busy.contains(&saved_key));
    rsx! {
        span { class: "message-tools",
            ondoubleclick: |event| event.stop_propagation(),
            button {
                title: "Add a reaction", "aria-label": "Add a reaction",
                onclick: {
                    let channel = channel_id.to_owned();
                    let ts = message.id.clone();
                    move |_| crate::reaction_picker::open(state, channel.clone(), ts.clone())
                },
                {crate::icons::icon(crate::icons::Icon::AddReaction, "icon sm")}
            }
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
            button {
                title:if saved {"Remove from Later"} else {"Save for later"},
                "aria-label":if saved {"Remove from Later"} else {"Save for later"},
                class:if saved {"is-saved"}else{""},
                disabled:saving,
                onclick:move |_|crate::later::change(state,saved_key.clone(),if saved {super_platinum_core::later::SavedAction::Remove}else{super_platinum_core::later::SavedAction::Save}),
                {crate::icons::icon(crate::icons::Icon::Bookmark,"icon sm")}
            }
            button {
                title:if pinned {"Unpin from conversation"}else{"Pin to conversation"},
                "aria-label":if pinned {"Unpin from conversation"}else{"Pin to conversation"},
                class:if pinned {"is-saved"}else{""}, disabled:pin_busy,
                onclick:{let channel=channel_id.to_owned();let ts=message.ts.clone();move |_|crate::pins::change(state,channel.clone(),ts.clone(),!pinned)},
                {crate::icons::icon(crate::icons::Icon::Pin,"icon sm")}
            }
            button {
                title:"Forward message", "aria-label":"Forward message",
                onclick:{let channel=channel_id.to_owned();let message=message.clone();move |_|crate::forward::open(state,channel.clone(),message.clone())},
                {crate::icons::icon(crate::icons::Icon::Forward,"icon sm")}
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
                ondoubleclick: |event| event.stop_propagation(),
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
            ondoubleclick: |event| event.stop_propagation(),
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
                super::media_image::MediaImage {
                    state, id:avatar.clone(), media_epoch:snapshot.media_epoch,
                    alt:message.author.clone(), compact:true,
                    fallback:rsx! { "{message.avatar_initials}" }
                }
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
    let compact = message.compact && !message.pending;
    let class = if in_thread {
        if compact {
            "message thread-message compact"
        } else if message.pending {
            "message thread-message pending"
        } else {
            "message thread-message"
        }
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
    // Double-clicking a channel message opens its thread with the reply box
    // focused. A thread reply is already where that would go, a row being
    // edited needs its clicks for the editor, and a pending message has no
    // `ts` Slack would recognise as a thread root yet.
    let opens_thread = !in_thread && !editing && !message.pending;
    rsx! {
        article {
            class: "{class}",
            tabindex: "0",
            "aria-label":"{message.author}, {message.timestamp}",
            "data-message-index": if let Some(index) = index { "{index}" },
            "data-message-id": "{message.id}",
            ondoubleclick: {
                let channel = channel_id.to_owned();
                let root = message.id.clone();
                move |_| {
                    if opens_thread {
                        spawn(crate::bootstrap::open_thread(state, channel.clone(), root.clone()));
                        super::composer::focus_field_when_mounted("thread-composer");
                    }
                }
            },
            if compact {
                div { class: "compact-gutter", title: "{message.timestamp}", "{message.timestamp}" }
            } else {
                {message_avatar(state, snapshot, message)}
            }
            {message_tools(state, message, channel_id, surface)}
            div { class: "message-content",
                if crate::pins::is_pinned(snapshot,channel_id,message) {
                    div {class:"message-pin-label",{crate::icons::icon(crate::icons::Icon::Pin,"icon sm")} "Pinned"}
                }
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
                        if compact && message.edited {span {class:"message-edited"," (edited)"}}
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
                        ondoubleclick: |event| event.stop_propagation(),
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
