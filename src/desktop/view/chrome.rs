use dioxus::prelude::*;

use crate::state::{MainView, Overlay, ShellState};

pub(crate) fn rail_view(
    mut state: Signal<ShellState>,
    active: MainView,
    dm_unread: usize,
    activity_unread: usize,
    account_initial: &str,
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
            button {
                class: "rail-avatar",
                title: "Accounts",
                onclick: move |_| state.write().overlay = Some(Overlay::Accounts),
                "{account_initial}"
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
                            state.write().toast = Some(format!("Could not open huddle: {error}"));
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
