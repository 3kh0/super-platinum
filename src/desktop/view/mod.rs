//! Typed DOM view for the Dioxus desktop shell.

mod chrome;
pub(crate) mod composer;
pub(crate) mod rich;
mod secondary;
pub(crate) mod theme;

use dioxus::prelude::*;

use crate::overlays::overlay_view;
use crate::state::{MainView, ShellState};

use chrome::{channel_sidebar, conversation_header, rail_view};
use composer::{composer, load_older_if_needed, measure_thread, measure_timeline};
use rich::{
    attachment_embeds, message_plain_text, pending_attachment_strip, reactions_row, reply_bar,
    rich_node,
};
use secondary::{activity_list_panel, dm_list_panel, secondary_view};
use theme::theme_css;

const CSS: &str = concat!(
    include_str!("../styles/base.css"),
    include_str!("../styles/chrome.css"),
    include_str!("../styles/lists.css"),
    include_str!("../styles/conversation.css"),
    include_str!("../styles/blocks.css"),
    include_str!("../styles/composer.css"),
    include_str!("../styles/overlays.css"),
    include_str!("../styles/profile.css"),
    include_str!("../styles/responsive.css"),
);

pub fn shell() -> Element {
    let mut state = consume_context::<Signal<ShellState>>();
    let theme_style = theme_css(
        &state.read().core.settings,
        state.read().background_uri.as_deref(),
    );
    if !state.read().signed_in {
        return rsx! {
            style { {CSS} }
            style { {theme_style} }
            main { class: "login-screen",
                div { class: "login-card",
                    div { class: "brand-mark", dangerous_inner_html: crate::icons::brand_mark_html() }
                    h1 { "Sign in to Super Platinum" }
                    p { "Connect a Slack workspace to get started." }
                    button {
                        class: "primary",
                        onclick: move |_| { spawn(crate::bootstrap::sign_in(state)); },
                        "Sign in with Slack"
                    }
                }
            }
        };
    }
    if state.read().loading {
        return rsx! {
            style { {CSS} }
            style { {theme_style} }
            main { class: "loading-screen",
                div { class: "spinner" }
                p { "Opening your workspace…" }
            }
        };
    }

    let snapshot = state.read();
    let channel_name = snapshot
        .channels
        .get(snapshot.active_channel)
        .map(|channel| channel.name.as_str())
        .unwrap_or("general");
    let timeline_start = snapshot.timeline_start.min(snapshot.messages.len());
    let timeline_end = snapshot
        .timeline_end
        .max(timeline_start)
        .min(snapshot.messages.len());
    let average_height = if snapshot.row_heights.is_empty() {
        66.0
    } else {
        snapshot.row_heights.values().sum::<f64>() / snapshot.row_heights.len() as f64
    };
    let top_height = if snapshot.row_heights.is_empty() {
        0.0
    } else {
        snapshot.messages[..timeline_start]
            .iter()
            .map(|message| {
                snapshot
                    .row_heights
                    .get(&message.id)
                    .copied()
                    .unwrap_or(average_height)
            })
            .sum::<f64>()
    };
    let bottom_height = snapshot.messages[timeline_end..]
        .iter()
        .map(|message| {
            snapshot
                .row_heights
                .get(&message.id)
                .copied()
                .unwrap_or(average_height)
        })
        .sum::<f64>();
    let active_workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    let active_channel_id = snapshot
        .channels
        .get(snapshot.active_channel)
        .map(|channel| channel.id.as_str())
        .unwrap_or_default();
    let typing_names = active_workspace
        .map(|workspace| workspace.typing_names(active_channel_id))
        .unwrap_or_default();
    let typing_text = if typing_names.is_empty() {
        String::new()
    } else {
        format!(
            "{} {} typing…",
            typing_names.join(", "),
            if typing_names.len() == 1 { "is" } else { "are" }
        )
    };
    let huddle = active_workspace
        .and_then(|workspace| workspace.active_huddle(active_channel_id))
        .cloned();

    let workspace_name = snapshot
        .workspaces
        .get(snapshot.active_workspace)
        .map(|workspace| workspace.name.as_str())
        .unwrap_or("Super Platinum");
    let shortcut = if cfg!(target_os = "macos") {
        "⌘K"
    } else {
        "Ctrl K"
    };
    let dm_unread_total = active_workspace
        .map(|workspace| {
            workspace
                .channels
                .values()
                .filter(|channel| (channel.is_im || channel.is_mpim) && !channel.is_archived)
                .filter(|channel| workspace.unread_total(channel) > 0)
                .count()
        })
        .unwrap_or(0);
    let activity_unread_total = active_workspace
        .and_then(|workspace| workspace.activity_unread_count)
        .unwrap_or(0) as usize;
    let show_channel_sidebar = matches!(
        snapshot.main_view,
        MainView::Home | MainView::Unreads | MainView::Threads
    );
    let show_dm_panel = snapshot.main_view == MainView::Dms;
    let show_activity_panel = snapshot.main_view == MainView::Activity;
    let active_is_dm = snapshot
        .channels
        .get(snapshot.active_channel)
        .is_some_and(|channel| channel.is_im || channel.is_mpim);
    // The thread pane names its conversation next to the "Thread" title, the
    // way Slack does, so a thread opened from Activity still says where it is.
    let thread_channel_label = snapshot
        .channels
        .get(snapshot.active_channel)
        .map(|channel| {
            if channel.is_im || channel.is_mpim {
                channel.name.clone()
            } else {
                format!("#{}", channel.name)
            }
        })
        .unwrap_or_default();
    // Activity's right side holds exactly one surface, the way Slack does it: a
    // thread item opens the thread across the whole pane, anything else opens
    // the channel around the message.
    let activity_thread_focus =
        snapshot.main_view == MainView::Activity && snapshot.thread_root.is_some();
    let show_timeline = match snapshot.main_view {
        MainView::Home => true,
        MainView::Dms => active_is_dm,
        // Activity's main column stays empty until an item is opened.
        MainView::Activity => snapshot.activity_detail_open && !activity_thread_focus,
        MainView::Unreads | MainView::Threads => false,
    };
    // Activity keeps list + optional main channel/thread; composer only for real chats.
    let show_composer = match snapshot.main_view {
        MainView::Home => true,
        MainView::Dms => active_is_dm,
        _ => false,
    };
    let account = &snapshot.self_account;
    let account_avatar = account
        .avatar
        .as_ref()
        .filter(|avatar| snapshot.media.is_ready(avatar))
        .map(|avatar| avatar.uri_at(snapshot.media_epoch));

    rsx! {
        style { {CSS} }
        style { {theme_style} }
        main {
            class: "app-shell",
            ondragover: move |event| event.prevent_default(),
            ondrop: move |event| {
                event.prevent_default();
                state.write().add_attachments(event.data_transfer().files().into_iter().map(|file| file.path()));
            },
            {rail_view(state, snapshot.main_view, dm_unread_total, activity_unread_total, account, account_avatar)}
            if show_channel_sidebar {
                {channel_sidebar(state, &snapshot, workspace_name, shortcut)}
            }
            if show_dm_panel {
                {dm_list_panel(state, &snapshot)}
            }
            if show_activity_panel {
                {activity_list_panel(state, &snapshot)}
            }
            // A thread opened from Activity replaces the transcript instead of
            // sitting beside it: Slack shows one surface there, not two.
            if !activity_thread_focus {
                section { class: "conversation",
                    if show_timeline {
                        header { class: "conversation-header",
                            {conversation_header(state, &snapshot, channel_name, huddle.as_ref())}
                        }
                        section {
                            id: "message-timeline",
                            class: "timeline",
                            onscroll: move |_| {
                                if !state.read().selection_pinned {
                                    spawn(measure_timeline(state));
                                    spawn(load_older_if_needed(state));
                                }
                            },
                            onselectstart: move |_| state.write().pin_selection(),
                            onmouseup: move |_| {
                                state.write().unpin_selection();
                                spawn(measure_timeline(state));
                            },
                        div { class: "timeline-spacer", style: "height: {top_height}px" }
                        for (offset, message) in snapshot.messages[timeline_start..timeline_end].iter().enumerate() {
                            div {
                                key: "{message.id}",
                                class: "message-block",
                            if let Some(label) = message.date_label.as_ref() {
                                div { class: "date-separator",
                                    span { "{label}" }
                                }
                            }
                            if message.show_unread_divider {
                                div { class: "unread-divider",
                                    span { "New" }
                                }
                            }
                            article {
                                class: if snapshot.message_arrivals.contains_key(&message.id) {
                                    if message.compact { "message compact entering" } else { "message entering" }
                                } else if message.compact {
                                    "message compact"
                                } else if message.pending {
                                    "message pending"
                                } else {
                                    "message"
                                },
                                "data-message-index": "{timeline_start + offset}",
                                "data-message-id": "{message.id}",
                                if message.compact {
                                    div { class: "compact-gutter", title: "{message.timestamp}", "{message.timestamp}" }
                                } else {
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
                                                if let Some(user) = user.clone() { spawn(crate::bootstrap::open_profile(state, user)); }
                                            }
                                        },
                                        // Initials until the bytes land: an `img`
                                        // with nothing behind it paints the
                                        // platform's broken-image icon.
                                        if let Some(avatar) = message.avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                                            img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "{message.author}" }
                                        } else { "{message.avatar_initials}" }
                                    }
                                }
                                // Out of flow, and outside `.message-meta`: a
                                // toolbar that joins the header row on hover
                                // reflows — and wraps to a second line in a
                                // narrow pane — resizing the message under the
                                // pointer. Compact rows get one this way too.
                                span { class: "message-tools",
                                    button {
                                        onclick: {
                                            let id = message.id.clone();
                                            let channel = snapshot.channels.get(snapshot.active_channel).map(|c| c.id.clone()).unwrap_or_default();
                                            move |_| {
                                                spawn(crate::bootstrap::open_thread(state, channel.clone(), id.clone()));
                                            }
                                        },
                                        "Reply"
                                    }
                                    if message.is_own {
                                        button {
                                            onclick: {
                                                let channel = snapshot.channels.get(snapshot.active_channel).map(|c| c.id.clone()).unwrap_or_default();
                                                let ts = message.id.clone();
                                                let text = message_plain_text(message);
                                                move |_| state.write().start_edit(channel.clone(), ts.clone(), text.clone())
                                            },
                                            "Edit"
                                        }
                                        button {
                                            onclick: {
                                                let channel = snapshot.channels.get(snapshot.active_channel).map(|c| c.id.clone()).unwrap_or_default();
                                                let ts = message.id.clone();
                                                move |_| { spawn(crate::bootstrap::delete_message(state, channel.clone(), ts.clone())); }
                                            },
                                            "Delete"
                                        }
                                    }
                                }
                                div { class: "message-content",
                                    if !message.compact {
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
                                    if snapshot.core.editing.as_ref().is_some_and(|(channel, ts)| snapshot.channels.get(snapshot.active_channel).is_some_and(|c| &c.id == channel) && ts == &message.id) {
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
                                            {reactions_row(state, &snapshot.media, snapshot.media_epoch, active_channel_id, message)}
                                        }
                                    }
                                    if message.reply_count > 0 {
                                        {reply_bar(state, &snapshot.media, snapshot.media_epoch, active_channel_id, message)}
                                    }
                                }
                            }
                            }
                        }
                        div { class: "timeline-spacer", style: "height: {bottom_height}px" }
                        if !typing_text.is_empty() {
                            p { class: "typing-indicator", "{typing_text}" }
                        }
                        }
                        if snapshot.chat_paused && show_composer {
                            div { class: "chat-paused", "Posting is paused in this conversation" }
                        }
                        if show_composer {
                            {composer(state, &snapshot.core.composer.text, channel_name)}
                        }
                    } else if matches!(snapshot.main_view, MainView::Unreads | MainView::Threads) {
                        header { class: "conversation-header",
                            h2 {
                                if snapshot.main_view == MainView::Unreads { "Unreads" }
                                else { "Threads" }
                            }
                        }
                        {secondary_view(state, &snapshot)}
                    } else {
                        div { class: "main-empty",
                            div { class: "bubble" }
                            p {
                                if snapshot.main_view == MainView::Dms {
                                    "Select a conversation to start messaging."
                                } else if snapshot.main_view == MainView::Activity {
                                    "Select a notification to view the details."
                                } else {
                                    "Select a conversation."
                                }
                            }
                        }
                    }
                }
            }
            if let Some(root) = snapshot.thread_root.as_ref() {
                aside {
                    class: if activity_thread_focus { "thread-pane wide" } else { "thread-pane" },
                    header {
                        h2 { "Thread" }
                        span { class: "thread-channel", "{thread_channel_label}" }
                        button {
                            class: "thread-close",
                            onclick: move |_| {
                                let mut shell = state.write();
                                shell.close_thread();
                                // Closing a thread opened from Activity returns to
                                // the empty pane, not to the channel behind it.
                                if shell.main_view == MainView::Activity {
                                    shell.activity_detail_open = false;
                                    shell.core.activity.selected = None;
                                }
                            },
                            "×"
                        }
                    }
                    div {
                        id: "thread-body",
                        class: "thread-body",
                        onscroll: move |_| { spawn(measure_thread(state)); },
                        for message in &snapshot.thread_messages {
                            article { class: "message thread-message", key: "thread-{message.id}",
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
                                                spawn(crate::bootstrap::open_profile(state, user));
                                            }
                                        }
                                    },
                                    if let Some(avatar) = message.avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                                        img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "{message.author}" }
                                    } else {
                                        "{message.avatar_initials}"
                                    }
                                }
                                div { class: "message-content",
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
                                    div { class: "message-body", for node in &message.body { {rich_node(node, snapshot.media_epoch, state)} } }
                                    if !message.attachments.is_empty() {
                                        {attachment_embeds(state, snapshot.media_epoch, &message.attachments)}
                                    }
                                    if let Some(attachments) = snapshot.pending_attachments_for_message(&message.id) {
                                        {pending_attachment_strip(attachments, snapshot.upload_ui_epoch)}
                                    }
                                    if !message.reactions.is_empty() {
                                        div { class: "message-actions",
                                            {reactions_row(state, &snapshot.media, snapshot.media_epoch, active_channel_id, message)}
                                        }
                                    }
                                }
                            }
                        }
                        if snapshot.thread_messages.is_empty() { p { "Loading replies to {root}…" } }
                    }
                    footer { class: "thread-composer",
                        textarea {
                            id: "thread-composer",
                            initial_value: "{snapshot.core.thread_composer.text}",
                            placeholder: "Reply…",
                            oninput: move |event| {
                                let value = event.value();
                                let end = value.len();
                                let mut shell = state.write();
                                shell.core.thread_composer.text = value;
                                shell.core.thread_composer.set_selection(end, end);
                            },
                            onkeydown: move |event| {
                                if event.key() == Key::Enter && !event.modifiers().shift() {
                                    event.prevent_default();
                                    spawn(crate::bootstrap::send_thread_composer(state));
                                }
                            }
                        }
                    }
                }
            }
            if snapshot.profile_user.is_some() {
                {crate::profile::profile_pane(state, &snapshot)}
            }
            if let Some(overlay) = snapshot.overlay { {overlay_view(state, overlay, &snapshot)} }
            if let Some(hover) = snapshot.profile_hover.as_ref() {
                {crate::profile::profile_hover_card(state, &snapshot, hover)}
            }
            if let Some(toast) = snapshot.toast.as_ref() { div { class: "toast", "{toast}" } }
        }
    }
}
