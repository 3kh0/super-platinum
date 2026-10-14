//! Typed DOM view for the Dioxus desktop shell.

use crate::media::MediaUri;
pub(crate) mod app_modal;
mod app_modal_users;
pub(crate) mod button_confirmation;
mod channel_details;
mod channel_membership;
mod channel_notifications;
mod channel_resources;
mod channel_window_check;
mod chrome;
mod completion;
pub(crate) mod composer;
pub(crate) mod files;
mod global_notifications;
mod huddle;
mod huddle_stage;
mod later;
mod local_drafts;
pub(crate) mod media_image;
mod member_admin;
mod message;
mod message_button;
mod message_checkboxes;
mod message_date;
pub(crate) mod message_multi;
mod message_overflow;
mod message_radio;
mod message_select;
mod message_time;
pub(crate) mod message_user;
mod new_channel;
mod new_reminder;
mod notification_schedule;
mod pins;
mod presentation_notifications;
pub(crate) mod profile_edit;
pub(crate) mod profile_title;
pub(crate) mod rich;
pub(crate) mod rich_editor;
pub(crate) mod scheduled;
pub(crate) mod search;
mod secondary;
mod selection;
mod selection_check;
pub(crate) mod self_status;
mod send_outcomes_check;
pub(crate) mod settings;
pub(crate) mod slack_lists;
pub(crate) mod theme;
mod thread_paging;
mod thread_window;
mod thread_window_check;
mod threads;
mod user_choices;

use dioxus::prelude::*;

use crate::overlays::overlay_view;
use crate::state::{ChannelTab, ComposerTarget, MainView, Overlay, ShellState};

use chrome::{channel_sidebar, conversation_header, rail_view};
use composer::{composer, load_older_if_needed, schedule_timeline_measure, thread_composer};
use message::{RowSurface, message_row};
use secondary::{activity_list_panel, dm_list_panel, secondary_view};
use theme::theme_css;

const CSS: &str = concat!(
    include_str!("../styles/base.css"),
    include_str!("../styles/chrome.css"),
    include_str!("../styles/lists.css"),
    include_str!("../styles/later.css"),
    include_str!("../styles/files.css"),
    include_str!("../styles/canvas.css"),
    include_str!("../styles/bookmarks.css"),
    include_str!("../styles/pins.css"),
    include_str!("../styles/forward.css"),
    include_str!("../styles/scheduled.css"),
    include_str!("../styles/channel-details.css"),
    include_str!("../styles/channel-notifications.css"),
    include_str!("../styles/notification-schedule.css"),
    include_str!("../styles/presentation-notifications.css"),
    include_str!("../styles/global-notifications.css"),
    include_str!("../styles/conversation.css"),
    include_str!("../styles/blocks.css"),
    include_str!("../styles/message-overflow.css"),
    include_str!("../styles/composer.css"),
    include_str!("../styles/overlays.css"),
    include_str!("../styles/app-modal.css"),
    include_str!("../styles/button-confirmation.css"),
    include_str!("../styles/self-status.css"),
    include_str!("../styles/profile-edit.css"),
    include_str!("../styles/profile-title.css"),
    include_str!("../styles/slack-lists.css"),
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
    let thread_measurement_pending = use_signal(|| false);
    let thread_measurement_queued = use_signal(|| false);
    use_future(move || {
        selection::run(
            state,
            measurement_pending,
            measurement_queued,
            thread_measurement_pending,
            thread_measurement_queued,
        )
    });
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
    let timeline_start = snapshot.timeline_start.min(snapshot.channel_rows.len());
    let timeline_end = snapshot
        .timeline_end
        .max(timeline_start)
        .min(snapshot.channel_rows.len());
    let (top_height, bottom_height) = snapshot
        .channel_geometry
        .spacer_heights(timeline_start, timeline_end);
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
    // DM rail is a channel highlight-count projection, independent of the
    // Activity category map and whether Inbox includes DMs.
    let dm_unread_total = active_workspace
        .map(|workspace| workspace.dm_rail_badge_count() as usize)
        .unwrap_or(0);
    let activity_unread_total =
        active_workspace.and_then(activity_badge_total).unwrap_or(0) as usize;
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
        .map(|avatar| avatar.webview_uri_at(snapshot.media_epoch));

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
                        if snapshot.channel_tab == ChannelTab::Bookmarks {
                            crate::bookmarks::BookmarksPanel {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team:?}-{active_channel_id}",state,channel:active_channel_id.to_owned()}
                        } else if snapshot.channel_tab == ChannelTab::Pins {
                            pins::PinsPanel {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team.as_deref().unwrap_or_default()}-{active_channel_id}",state,channel:active_channel_id.to_owned()}
                        } else if snapshot.channel_tab != ChannelTab::Messages {
                            channel_resources::ChannelResources {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team.as_deref().unwrap_or_default()}-{active_channel_id}",state,channel:active_channel_id.to_owned()}
                        } else {
                        section {
                            id: "message-timeline",
                            class: "timeline",
                            "data-timeline-scope":snapshot.timeline_scope(),
                            "data-window-start":timeline_start,
                            "data-window-end":timeline_end,
                            "data-scroll-pending":snapshot.core.pending_scroll_to.is_some().to_string(),
                            onmounted:move |_| {dioxus::document::eval(&format!("{}();",include_str!("../timeline_anchor.js")));spawn(send_outcomes_check::run(state));dioxus::core::spawn_forever(channel_window_check::run(state));},
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
                            "data-selection-pinned":snapshot.selection_pinned.to_string(),
                        div { class: "timeline-spacer", style: "height: {top_height}px" }
                        for (offset, message) in snapshot.messages.iter().enumerate() {
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
                                {message_row(state, &snapshot, message, active_channel_id, Some(timeline_start + offset), RowSurface::Timeline, Some(crate::message_buttons::source::Bag::Timeline))}
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
                                    "Select a saved item to open it."
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
                            "aria-label": "Close thread",
                            title: "Close thread",
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
                        "data-thread-projection-generation":snapshot.thread_window.projection_generation.to_string(),
                        "data-scroll-pending":snapshot.thread_target.is_some(),
                        onmounted:move |_| {dioxus::document::eval(&format!("{}(\"thread-body\",\"superPlatinumThreadAnchor\");",include_str!("../timeline_anchor.js")));spawn(thread_window_check::run(state));},
                        onscroll: move |_| {thread_window::schedule(state,thread_measurement_pending,thread_measurement_queued);},
                        "data-selection-pinned":snapshot.thread_window.selection_pinned.to_string(),
                        onwheel:move |_| {
                            let mut shell = state.write();
                            shell.thread_window.user_navigation();
                            shell.thread_target = None;
                            shell.thread_at_bottom = false;
                        },
                        onpointerdown:move |_| {
                            let mut shell = state.write();
                            shell.thread_window.user_navigation();
                            shell.thread_target = None;
                            shell.thread_at_bottom = false;
                        },
                        ontouchmove:move |_| {
                            let mut shell = state.write();
                            shell.thread_window.user_navigation();
                            shell.thread_target = None;
                            shell.thread_at_bottom = false;
                        },
                        {crate::view::thread_paging::page_control(state, &snapshot, super_platinum_core::thread_history::Direction::Earlier)}
                        {thread_window::rows(state,&snapshot,active_channel_id,true)}
                        {crate::view::thread_paging::page_control(state, &snapshot, super_platinum_core::thread_history::Direction::Newer)}
                        {thread_window::rows(state,&snapshot,active_channel_id,false)}
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
            div {
                id: "shell-overlays",
                style: "display: contents",
                onmounted: move |_| {
                    dioxus::document::eval("window.superPlatinumOverlayKeyboard?.observe(document.getElementById('shell-overlays'));");
                },
                if let Some(overlay) = snapshot.overlay { {overlay_view(state, overlay, &snapshot)} }
            }
            if let Some(hover) = snapshot.profile_hover.as_ref() {
                {crate::profile::profile_hover_card(state, &snapshot, hover)}
            }
            // Click dismisses: a status line the reader has already read must
            // never be the thing standing between them and the window.
            if let Some(toast) = snapshot.toast.as_ref() {
                div {
                    id: "status-toast",
                    class: "toast",
                    role: "status",
                    onmounted: move |_| { dioxus::document::eval(include_str!("toast_position.js")); },
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

/// Preserve the existing unknown/legacy UI without pretending it is the Inbox
/// formula. Known Inbox with no complete count map must not show the raw sum.
pub(super) fn activity_badge_total(
    workspace: &super_platinum_core::state::Workspace,
) -> Option<u32> {
    if workspace.activity_inbox_enabled() == Some(true) {
        workspace.activity_badge_count()
    } else {
        workspace.activity_unread_count
    }
}
