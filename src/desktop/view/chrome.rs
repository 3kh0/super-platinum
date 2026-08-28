use dioxus::prelude::*;

use crate::state::{ConnectionStatus, MainView, Overlay, PresenceVm, SelfAccountVm, ShellState};

/// Presence glyphs traced from the real client (`status-member*`, viewBox
/// `0 0 20 20`): a ring when away, a filled disc when active, each gaining the
/// "Z" mark while notifications are snoozed.
fn presence_path(presence: PresenceVm, snoozed: bool) -> &'static str {
    match (presence, snoozed) {
        (PresenceVm::Active, false) => "M14.5 10a4.5 4.5 0 1 1-9 0 4.5 4.5 0 0 1 9 0",
        (PresenceVm::Active, true) => {
            "M11.25 3.5a.75.75 0 0 0 0 1.5h1.847l-2.411 2.756A.75.75 0 0 0 11.25 9h3.5a.75.75 0 0 0 0-1.5h-1.847l2.411-2.756A.75.75 0 0 0 14.75 3.5zM9.557 6.768C10.18 6.055 10 5.5 9.406 5.54a4.5 4.5 0 1 0 5.067 4.96H11.25a2.25 2.25 0 0 1-1.693-3.73"
        }
        (_, false) => "M7 10a3 3 0 1 1 6 0 3 3 0 0 1-6 0m3-4.5a4.5 4.5 0 1 0 0 9 4.5 4.5 0 0 0 0-9",
        (_, true) => {
            "M11.25 3.5a.75.75 0 0 0 0 1.5h1.847l-2.411 2.756A.75.75 0 0 0 11.25 9h3.5a.75.75 0 0 0 0-1.5h-1.847l2.411-2.756A.75.75 0 0 0 14.75 3.5zM7 10a3 3 0 0 1 3-3V5.5a4.5 4.5 0 1 0 4.5 4.5H13a3 3 0 1 1-6 0"
        }
    }
}

/// The rail avatar is notched so the presence badge sits in a hole rather than
/// on top of the picture, exactly as the real client masks its own avatar.
/// Both cut-outs are object-bounding-box paths lifted from Slack's
/// `mask__small-member` / `mask__small-member-dnd` clip paths.
fn avatar_mask_defs() -> Element {
    rsx! {
        svg { class: "avatar-mask-defs", "aria-hidden": "true",
            clipPath { id: "rail-avatar-mask", clip_path_units: "objectBoundingBox",
                path { d: "M1,0 H0 V1 H0.752 C0.701,0.949,0.669,0.878,0.669,0.8 C0.669,0.645,0.795,0.519,0.95,0.519 C0.967,0.519,0.984,0.52,1,0.523 V0" }
            }
            clipPath { id: "rail-avatar-mask-snoozed", clip_path_units: "objectBoundingBox",
                path { d: "M1,0 H0 V1 H0.752 C0.701,0.949,0.669,0.878,0.669,0.8 C0.669,0.674,0.752,0.567,0.867,0.531 C0.888,0.48,0.938,0.444,0.997,0.444 H1 V0" }
            }
        }
    }
}

pub(crate) fn rail_view(
    mut state: Signal<ShellState>,
    active: MainView,
    dm_unread: usize,
    activity_unread: usize,
    account: &SelfAccountVm,
    avatar_uri: Option<String>,
    self_menu_open: bool,
    connection: Option<ConnectionStatus>,
) -> Element {
    let home_active = matches!(
        active,
        MainView::Home | MainView::Unreads | MainView::Threads
    );
    let home_src = crate::icons::home_uri();
    let dms_src = crate::icons::dms_uri();
    let bell_src = crate::icons::bell_uri();
    rsx! {
        nav { class: "rail",
            button {
                class: if home_active { "rail-btn active" } else { "rail-btn" },
                title: "Home",
                onclick: move |_| { spawn(crate::bootstrap::load_main_view(state, MainView::Home)); },
                img { class: "icon", src: "{home_src}", alt: "Home" }
            }
            button {
                class: if active == MainView::Dms { "rail-btn active" } else { "rail-btn" },
                title: "Direct messages",
                onclick: move |_| { spawn(crate::bootstrap::load_main_view(state, MainView::Dms)); },
                img { class: "icon", src: "{dms_src}", alt: "DMs" }
                if dm_unread > 0 {
                    span { class: "rail-badge",
                        if dm_unread > 99 { "99+" } else { "{dm_unread}" }
                    }
                }
            }
            button {
                class: if active == MainView::Activity { "rail-btn active" } else { "rail-btn" },
                title: "Activity",
                onclick: move |_| { spawn(crate::bootstrap::load_main_view(state, MainView::Activity)); },
                img { class: "icon", src: "{bell_src}", alt: "Activity" }
                if activity_unread > 0 {
                    span { class: "rail-badge",
                        if activity_unread > 99 { "99+" } else { "{activity_unread}" }
                    }
                }
            }
            div { class: "rail-spacer" }
            // Sits above the account picture, the way Telegram parks its own
            // connection spinner: present only while something is wrong, so the
            // rail does not gain a permanent widget that means nothing.
            if let Some(connection) = connection {
                div {
                    class: "rail-connection",
                    role: "status",
                    title: "{connection.label()}",
                    "aria-label": "{connection.label()}",
                    span { class: "rail-connection-spinner" }
                }
            }
            button {
                class: if account.snoozed { "rail-avatar snoozed" } else { "rail-avatar" },
                title: "{account.name} — {account.status_label()}",
                "aria-label": "{account.name}, {account.status_label()}",
                "aria-haspopup": "menu",
                "aria-expanded": if self_menu_open { "true" } else { "false" },
                onclick: move |_| state.write().toggle_self_menu(),
                {avatar_mask_defs()}
                // Initials until the bytes land: an `img` with nothing behind
                // it paints the platform's broken-image icon.
                if let Some(uri) = avatar_uri {
                    img { class: "rail-avatar-image", src: "{uri}", alt: "" }
                } else {
                    span { class: "rail-avatar-initials", "{account.initials}" }
                }
                span {
                    class: match (account.presence, account.snoozed) {
                        (PresenceVm::Active, true) => "rail-presence active snoozed",
                        (PresenceVm::Active, false) => "rail-presence active",
                        (_, true) => "rail-presence snoozed",
                        (_, false) => "rail-presence",
                    },
                    svg { view_box: "0 0 20 20", "aria-hidden": "true",
                        path { fill: "currentColor", fill_rule: "evenodd", clip_rule: "evenodd", d: presence_path(account.presence, account.snoozed) }
                    }
                }
            }
        }
    }
}

pub(crate) fn channel_sidebar(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    workspace_name: &str,
    shortcut: &str,
) -> Element {
    let search_src = crate::icons::search_uri();
    let unreads_src = crate::icons::unreads_uri();
    let reply_src = crate::icons::reply_uri();
    let lock_src = crate::icons::lock_uri();
    let has_unreads = snapshot.channels.iter().any(|channel| channel.unread);
    rsx! {
        aside { class: "sidebar",
            div { class: "sidebar-title",
                button {
                    title: "Accounts",
                    onclick: move |_| state.write().overlay = Some(Overlay::Accounts),
                    "{workspace_name}"
                }
            }
            button {
                class: "jump-to",
                title: "Jump to…",
                onclick: move |_| state.write().overlay = Some(Overlay::Palette),
                img { class: "icon sm", src: "{search_src}", alt: "" }
                span { "Jump to…" }
                span { class: "hint", "{shortcut}" }
            }
            nav { class: "sidebar-pages",
                button {
                    class: if snapshot.main_view == MainView::Unreads {
                        if has_unreads { "nav-row active unread" } else { "nav-row active" }
                    } else if has_unreads {
                        "nav-row unread"
                    } else {
                        "nav-row"
                    },
                    onclick: move |_| { spawn(crate::bootstrap::load_main_view(state, MainView::Unreads)); },
                    img { class: "icon sm", src: "{unreads_src}", alt: "" }
                    span { "Unreads" }
                }
                button {
                    class: if snapshot.main_view == MainView::Threads { "nav-row active" } else { "nav-row" },
                    onclick: move |_| { spawn(crate::bootstrap::load_main_view(state, MainView::Threads)); },
                    img { class: "icon sm", src: "{reply_src}", alt: "" }
                    span { "Threads" }
                }
            }
            hr { class: "sidebar-divider" }
            nav { class: "channel-list",
                if snapshot.workspaces.len() > 1 {
                    div { class: "section-label", "Workspaces" }
                    for (index, workspace) in snapshot.workspaces.iter().enumerate() {
                        button {
                            class: if index == snapshot.active_workspace { "workspace-row active" } else { "workspace-row" },
                            key: "ws-{workspace.id}",
                            onclick: move |_| {
                                if state.write().select_workspace(index) {
                                    spawn(crate::bootstrap::refresh_selected_channel(state));
                                }
                            },
                            span { class: if index == snapshot.active_workspace { "ws-dot on" } else { "ws-dot" } }
                            span { "{workspace.name}" }
                        }
                    }
                }
                for section in &snapshot.sidebar_sections {
                    div { class: "section-label", key: "section-{section.id}", "{section.title}" }
                    for index in section.channel_indices.iter().copied() {
                        if let Some(channel) = snapshot.channels.get(index) {
                            button {
                                class: if index == snapshot.active_channel && snapshot.main_view == MainView::Home {
                                    "channel active"
                                } else if channel.unread {
                                    "channel unread"
                                } else {
                                    "channel"
                                },
                                key: "channel-{channel.id}",
                                onclick: move |_| {
                                    state.write().select_channel(index);
                                    spawn(crate::bootstrap::refresh_selected_channel(state));
                                },
                                span { class: "channel-icon",
                                    if channel.is_im {
                                        if let Some(avatar) = channel.avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                                            img { class: "sidebar-avatar", src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "{channel.name}" }
                                        } else {
                                            span { class: "sidebar-avatar placeholder", "{channel.avatar_initials}" }
                                        }
                                    } else if channel.is_mpim {
                                        span { class: "mpdm-count", "{channel.member_count.unwrap_or(0)}" }
                                    } else if channel.is_private {
                                        img { class: "icon sm", src: "{lock_src}", alt: "private" }
                                    } else {
                                        span { "#" }
                                    }
                                }
                                span { class: "channel-name", "{channel.name}" }
                                if channel.mention_count > 0 {
                                    span { class: "ping-badge",
                                        if channel.mention_count > 99 { "99+" } else { "{channel.mention_count}" }
                                    }
                                } else if channel.unread {
                                    i { class: "unread-dot" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn conversation_header(
    mut state: Signal<ShellState>,
    snapshot: &ShellState,
    channel_name: &str,
    huddle: Option<&super_platinum_core::slack::models::Room>,
) -> Element {
    let channel = snapshot.channels.get(snapshot.active_channel);
    let title = if channel.is_some_and(|c| c.is_im || c.is_mpim) {
        channel
            .map(|c| c.name.clone())
            .unwrap_or_else(|| channel_name.to_owned())
    } else {
        format!("#{channel_name}")
    };
    let is_dm = channel.is_some_and(|c| c.is_im);
    let dm_user = channel.and_then(|c| c.user_id.clone());
    let dm_user_name = dm_user.clone();
    let dm_user_hover_avatar = dm_user.clone();
    let dm_user_hover_name = dm_user.clone();
    let dm_avatar = channel.and_then(|c| c.avatar.clone());
    let dm_initials = channel
        .map(|c| c.avatar_initials.clone())
        .unwrap_or_else(|| "?".into());
    let huddle_count = huddle.map(|h| h.participants.len()).unwrap_or(0);
    rsx! {
        if is_dm {
            button {
                class: "header-avatar",
                onmouseenter: move |event: MouseEvent| {
                    if let Some(user) = dm_user_hover_avatar.clone() {
                        let point = event.data().client_coordinates();
                        crate::profile::show_profile_hover(state, user, point.x, point.y);
                    }
                },
                onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
                onclick: move |_| {
                    if let Some(user) = dm_user.clone() {
                        spawn(crate::bootstrap::open_profile(state, user));
                    }
                },
                if let Some(avatar) = dm_avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                    img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "{title}" }
                } else {
                    "{dm_initials}"
                }
            }
        }
        h2 {
            if is_dm {
                button {
                    class: "header-name",
                    onmouseenter: move |event: MouseEvent| {
                        if let Some(user) = dm_user_hover_name.clone() {
                            let point = event.data().client_coordinates();
                            crate::profile::show_profile_hover(state, user, point.x, point.y);
                        }
                    },
                    onmouseleave: move |_| crate::profile::schedule_profile_hover_close(state),
                    onclick: move |_| {
                        if let Some(user) = dm_user_name.clone() {
                            spawn(crate::bootstrap::open_profile(state, user));
                        }
                    },
                    "{title}"
                }
            } else {
                "{title}"
            }
        }
        if let Some(huddle) = huddle {
            button {
                class: "huddle-button",
                onclick: {
                    let link = huddle.huddle_link.clone();
                    move |_| {
                        if let Some(link) = link.as_ref()
                            && let Err(error) = crate::media::open_external(link)
                        {
                            state.write().show_toast(format!("Could not open huddle: {error}"));
                        }
                    }
                },
                if huddle_count > 0 {
                    "Huddle · {huddle_count}"
                } else {
                    "Huddle"
                }
            }
        }
    }
}
