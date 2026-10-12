//! Typed DOM view for the Dioxus desktop shell.

mod channel_details;
mod channel_membership;
mod channel_resources;
mod chrome;
pub(crate) mod composer;
mod files;
mod huddle;
mod huddle_stage;
mod later;
mod local_drafts;
pub(crate) mod media_image;
mod member_admin;
mod message;
mod new_channel;
mod new_reminder;
mod pins;
pub(crate) mod rich;
pub(crate) mod rich_editor;
pub(crate) mod scheduled;
mod secondary;
pub(crate) mod settings;
pub(crate) mod theme;
mod thread_paging;
mod threads;

use dioxus::prelude::*;

use crate::overlays::overlay_view;
use crate::state::{ChannelTab, ComposerTarget, MainView, Overlay, ShellState};

use chrome::{channel_sidebar, conversation_header, rail_view};
use composer::{
    composer, load_older_if_needed, measure_thread, schedule_timeline_measure, thread_composer,
};
use message::{RowSurface, message_row};
use secondary::{activity_list_panel, dm_list_panel, secondary_view};
use theme::theme_css;

const CSS: &str = concat!(
    include_str!("../styles/base.css"),
    include_str!("../styles/chrome.css"),
    include_str!("../styles/lists.css"),
    include_str!("../styles/later.css"),
    include_str!("../styles/files.css"),
    include_str!("../styles/pins.css"),
    include_str!("../styles/forward.css"),
    include_str!("../styles/scheduled.css"),
    include_str!("../styles/channel-details.css"),
    include_str!("../styles/conversation.css"),
    include_str!("../styles/blocks.css"),
    include_str!("../styles/composer.css"),
    include_str!("../styles/overlays.css"),
    include_str!("../styles/settings.css"),
    include_str!("../styles/profile.css"),
    include_str!("../styles/huddle.css"),
    include_str!("../styles/design-system.css"),
    include_str!("../styles/responsive.css"),
);

pub fn shell() -> Element {
    let mut state = consume_context::<Signal<ShellState>>();
    let measurement_pending = use_signal(|| false);
    let measurement_queued = use_signal(|| false);
    let timeline_lifecycle = use_memo(move || {
        let shell = state.read();
        (
            shell.main_view,
            shell.channel_tab,
            shell.signed_in,
            shell.loading,
            shell.core.active_channel.clone(),
            shell.thread_root.clone(),
            shell.detail_open(),
        )
    });
    use_effect(move || {
        timeline_lifecycle();
        dioxus::document::eval(
            "requestAnimationFrame(()=>{window.superPlatinumTimelineAnchor?.check();window.superPlatinumThreadAnchor?.check();});",
        );
    });
    let theme_style = theme_css(
        &state.read().core.settings,
        state.read().background_uri.as_deref(),
    );
    if !state.read().signed_in && !state.read().loading {
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
    // Unmeasured rows keep a stable estimate. A moving average changes every
    // unknown row above the reader whenever a large image is measured.
    let top_height = snapshot.messages[..timeline_start]
        .iter()
        .map(|message| snapshot.estimated_row_height(&message.id))
        .sum::<f64>();
    let bottom_height = snapshot.messages[timeline_end..]
        .iter()
        .map(|message| snapshot.estimated_row_height(&message.id))
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
        MainView::Home | MainView::Unreads | MainView::Threads | MainView::Scheduled
    );
    let show_dm_panel = snapshot.main_view == MainView::Dms;
    let show_activity_panel = snapshot.main_view == MainView::Activity;
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
    let activity_thread_focus = matches!(snapshot.main_view, MainView::Activity | MainView::Later)
        && snapshot.thread_root.is_some();
    // Unreads and Threads are lists, not conversations. Every other surface
    // shows the conversation it has open — and DMs and Activity stay on their
    // empty state until a row in their own list opens one.
    let show_timeline = match snapshot.main_view {
        MainView::Unreads | MainView::Threads | MainView::Files | MainView::Scheduled => false,
        _ => snapshot.detail_open() && !activity_thread_focus,
    };
    // A conversation is a conversation wherever it is shown: Slack's Activity
    // and DMs panes carry a composer exactly like the channel view does, so a
    // message opened from a notification can be answered where it is read.
    let show_composer = show_timeline;
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
                if !matches!(state.peek().main_view, MainView::Files | MainView::Scheduled) && state.peek().channel_tab == ChannelTab::Messages {
                    state.write().add_attachments(event.data_transfer().files().into_iter().map(|file| file.path()), ComposerTarget::Channel);
                }
            },
            {rail_view(state, snapshot.main_view, dm_unread_total, activity_unread_total, account, account_avatar, snapshot.overlay == Some(Overlay::SelfMenu), snapshot.connection.indicator())}
            if show_channel_sidebar {
                {channel_sidebar(state, &snapshot, workspace_name, shortcut)}
            }
            if show_dm_panel {
                {dm_list_panel(state, &snapshot)}
            }
            if show_activity_panel {
                {activity_list_panel(state, &snapshot)}
            }
            if snapshot.main_view == MainView::Later {
                later::LaterPanel { key:"{snapshot.core.account_epoch}-{snapshot.core.active_team.as_deref().unwrap_or_default()}", state }
            }
            if snapshot.main_view == MainView::Scheduled {
                scheduled::ScheduledPanel {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team.as_deref().unwrap_or_default()}",state}
            }
            if snapshot.main_view == MainView::Files {
                files::FilesPanel {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team.as_deref().unwrap_or_default()}",state}
            }
            // A thread opened from Activity replaces the transcript instead of
            // sitting beside it: Slack shows one surface there, not two.
            if !activity_thread_focus && !matches!(snapshot.main_view, MainView::Files | MainView::Scheduled) {
                section { class: "conversation",
                    if show_timeline {
                        header { class: "conversation-header",
                            {conversation_header(state, &snapshot, channel_name, huddle.as_ref())}
                        }
                        {channel_resources::tabs(state, snapshot.channel_tab)}
                        if snapshot.channel_tab == ChannelTab::Pins {
                            pins::PinsPanel {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team.as_deref().unwrap_or_default()}-{active_channel_id}",state,channel:active_channel_id.to_owned()}
                        } else if snapshot.channel_tab != ChannelTab::Messages {
                            channel_resources::ChannelResources {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team.as_deref().unwrap_or_default()}-{active_channel_id}",state,channel:active_channel_id.to_owned()}
                        } else {
                        section {
                            id: "message-timeline",
                            class: "timeline",
                            "data-timeline-scope":snapshot.timeline_scope(),
                            "data-scroll-pending":snapshot.core.pending_scroll_to.is_some().to_string(),
                            onmounted:move |_| {dioxus::document::eval(&format!("{}();",include_str!("../timeline_anchor.js")));},
                            onscroll: move |_| {
                                if !state.read().selection_pinned {
                                    schedule_timeline_measure(state, measurement_pending, measurement_queued);
                                    spawn(load_older_if_needed(state));
                                }
                            },
                            onwheel: move |_| {
                                let mut shell = state.write();
                                if matches!(shell.core.pending_scroll_to, Some((_, super_platinum_core::domain::PendingScrollTarget::Message(_)))) {
                                    shell.core.pending_scroll_to = None;
                                }
                            },
                            onselectstart: move |_| state.write().pin_selection(),
                            onmouseup: move |_| {
                                state.write().unpin_selection();
                                schedule_timeline_measure(state, measurement_pending, measurement_queued);
                            },
                        div { class: "timeline-spacer", style: "height: {top_height}px" }
                        for (offset, message) in snapshot.messages[timeline_start..timeline_end].iter().enumerate() {
                            div {
                                key: "{message.id}",
                                class: "message-block",
                                "data-row-index":timeline_start + offset,
                                "data-row-id":message.id.clone(),
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
                                {message_row(state, &snapshot, message, active_channel_id, Some(timeline_start + offset), RowSurface::Timeline)}
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
                            if crate::unread::held(&snapshot) {
                                div {class:"unread-hold", role:"status",
                                    span {"Marked unread"}
                                    button {
                                        disabled:crate::unread::busy(&snapshot,active_channel_id),
                                        onclick:{let channel=active_channel_id.to_owned();move |_|crate::unread::change(state,channel.clone(),None)},
                                        "Mark as read"
                                    }
                                }
                            }
                            {composer(state, &snapshot.core.composer.text, channel_name)}
                        }
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
                                } else if snapshot.main_view == MainView::Later {
                                    "Select a saved message to open its conversation."
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
                    // A file dropped on the thread is a reply in it. Stopping
                    // here keeps the shell's handler from filing it into the
                    // channel composer as well.
                    ondrop: move |event| {
                        event.prevent_default();
                        event.stop_propagation();
                        state.write().add_attachments(
                            event.data_transfer().files().into_iter().map(|file| file.path()),
                            ComposerTarget::Thread,
                        );
                    },
                    header {
                        h2 { "Thread" }
                        span { class: "thread-channel", "{thread_channel_label}" }
                        {crate::thread_subscription::control(state, &snapshot)}
                        button {
                            class: "thread-close",
                            onclick: move |_| {
                                let mut shell = state.write();
                                // In Activity the thread *is* the surface, so
                                // closing it returns the pane to its empty
                                // state. Everywhere else the thread is a side
                                // panel and the conversation behind it stays.
                                if matches!(shell.main_view, MainView::Activity | MainView::Later) {
                                    shell.forget_surface();
                                } else {
                                    shell.close_thread();
                                }
                            },
                            {crate::icons::icon(crate::icons::Icon::Close, "icon")}
                        }
                    }
                    div {
                        id: "thread-body",
                        class: "thread-body",
                        "data-timeline-scope":snapshot.thread_scope(),
                        "data-scroll-pending":snapshot.thread_target.is_some(),
                        onmounted:move |_| {dioxus::document::eval(&format!("{}(\"thread-body\",\"superPlatinumThreadAnchor\");",include_str!("../timeline_anchor.js")));},
                        onscroll: move |_| { spawn(measure_thread(state)); },
                        onwheel:move |_| {
                            let mut shell = state.write();
                            shell.thread_target = None;
                            shell.thread_at_bottom = false;
                        },
                        {crate::view::thread_paging::page_control(state, &snapshot, super_platinum_core::thread_history::Direction::Earlier)}
                        for message in snapshot.thread_messages.iter().filter(|m| snapshot.thread_history.contains(root, &m.ts)) {
                            div { key: "thread-{message.id}", "data-row-id":"{message.id}",
                                if message.show_unread_divider { div {class:"unread-divider", span {"New"}} }
                                {message_row(state, &snapshot, message, active_channel_id, None, RowSurface::Thread)}
                            }
                        }
                        {crate::view::thread_paging::page_control(state, &snapshot, super_platinum_core::thread_history::Direction::Newer)}
                        for message in snapshot.thread_messages.iter().filter(|m| !snapshot.thread_history.contains(root, &m.ts)) {
                            div { key: "thread-{message.id}", "data-row-id":"{message.id}",
                                if message.show_unread_divider { div {class:"unread-divider", span {"New"}} }
                                {message_row(state, &snapshot, message, active_channel_id, None, RowSurface::Thread)}
                            }
                        }
                        if let Some(error) = &snapshot.thread_error {
                            div { class:"thread-load-notice", role:"status",
                                p { "{error}" }
                                button {
                                    onclick:{let channel=active_channel_id.to_owned();let root=root.clone();let target=snapshot.thread_target.clone();move |_| {spawn(crate::bootstrap::open_thread_at(state,channel.clone(),root.clone(),target.clone()));}},
                                    "Retry"
                                }
                            }
                        } else if snapshot.thread_messages.is_empty() && snapshot.thread_loading {
                            p { "Loading replies…" }
                        }
                    }
                    if crate::thread_unread::held(&snapshot) {
                        div {class:"unread-hold", role:"status",
                            span {"Marked unread"}
                            button {disabled:crate::thread_unread::busy(&snapshot),onclick:move |_|crate::thread_unread::change(state,None),"Mark as read"}
                        }
                    }
                    {thread_composer(state, &snapshot)}
                }
            }
            if snapshot.profile_user.is_some() {
                {crate::profile::profile_pane(state, &snapshot)}
            }
            if snapshot.group_panel.is_some() {
                {crate::usergroups::pane(state, &snapshot)}
            }
            {huddle_stage::stage(state, &snapshot)}
            if let Some(overlay) = snapshot.overlay { {overlay_view(state, overlay, &snapshot)} }
            if let Some(hover) = snapshot.profile_hover.as_ref() {
                {crate::profile::profile_hover_card(state, &snapshot, hover)}
            }
            // Click dismisses: a status line the reader has already read must
            // never be the thing standing between them and the window.
            if let Some(toast) = snapshot.toast.as_ref() {
                div {
                    class: "toast",
                    role: "status",
                    title: "{toast.text}",
                    onclick: move |_| state.write().toast = None,
                    span { class: "toast-text", "{toast.text}" }
                    button {
                        class: "toast-dismiss",
                        title: "Dismiss",
                        "aria-label": "Dismiss notification",
                        onclick: move |_| state.write().toast = None,
                        {crate::icons::icon(crate::icons::Icon::Close, "icon sm")}
                    }
                }
            }
        }
    }
}
