use dioxus::prelude::*;
use super_platinum_core::MediaAssetId;

use crate::state::{Overlay, PresenceVm, SettingsSection, ShellState};

pub fn overlay_view(
    mut state: Signal<ShellState>,
    overlay: Overlay,
    snapshot: &ShellState,
) -> Element {
    if overlay == Overlay::SelfMenu {
        return self_menu_view(state, snapshot);
    }
    let (title, input_value, placeholder) = match overlay {
        Overlay::SelfMenu => unreachable!("self menu renders as a rail popover"),
        Overlay::Accounts => ("Accounts", "", ""),
        Overlay::Palette => (
            "Jump to…",
            snapshot.palette_query.as_str(),
            "Channel or person",
        ),
        Overlay::Search => (
            "Search messages",
            snapshot.search_query.as_str(),
            "Search in workspace",
        ),
        Overlay::Settings => ("Settings", "", ""),
        Overlay::Viewer => ("Media viewer", "", ""),
    };
    let compact = overlay == Overlay::Palette;
    rsx! {
        div { class: "scrim", onclick: move |_| state.write().overlay = None }
        section { class: if compact { "modal compact" } else { "modal" }, role: "dialog", "aria-modal": "true",
            if !compact {
                header {
                    h2 { "{title}" }
                    button { onclick: move |_| state.write().overlay = None, "×" }
                }
            }
            if matches!(overlay, Overlay::Palette | Overlay::Search) {
                input {
                    id: "overlay-input",
                    autofocus: true,
                    onmounted: move |_| {
                        dioxus::document::eval(
                            r#"requestAnimationFrame(() => {
                                 const field = document.getElementById('overlay-input');
                                 if (field) { field.focus(); field.select(); }
                               });"#,
                        );
                    },
                    initial_value: "{input_value}",
                    placeholder: "{placeholder}",
                    oninput: move |event| match overlay {
                        Overlay::Palette => {
                            let mut shell = state.write();
                            shell.palette_query = event.value();
                            shell.palette_selected = 0;
                        },
                        Overlay::Search => state.write().search_query = event.value(),
                        _ => {}
                    },
                    onkeydown: move |event| {
                        if overlay == Overlay::Search && event.key() == Key::Enter {
                            spawn(crate::bootstrap::search(state));
                        } else if overlay == Overlay::Palette {
                            match event.key() {
                                Key::ArrowDown => { event.prevent_default(); state.write().move_palette(1); }
                                Key::ArrowUp => { event.prevent_default(); state.write().move_palette(-1); }
                                Key::Enter => {
                                    event.prevent_default();
                                    if state.write().submit_palette() { spawn(crate::bootstrap::refresh_selected_channel(state)); }
                                }
                                _ => {}
                            }
                        }
                    },
                }
                div { class: "results",
                    if overlay == Overlay::Search {
                        if snapshot.search_loading { p { class: "results-empty", "Searching…" } }
                        for result in &snapshot.search_results {
                            button {
                                class: "search-result",
                                onclick: {
                                    let channel = result.channel_id.clone();
                                    let ts = result.ts.clone();
                                    move |_| {
                                        if state.write().open_search_result(&channel, &ts) { spawn(crate::bootstrap::refresh_selected_channel(state)); }
                                    }
                                },
                                strong { "# {result.channel_name} · {result.author}" }
                                span { "{result.text}" }
                                span { class: "search-time", {super_platinum_core::state::format_relative_ts(&result.ts)} }
                            }
                        }
                        if !snapshot.search_loading && snapshot.search_results.is_empty() {
                            p { class: "results-empty", "Press Enter to search messages" }
                        }
                    } else {
                        {palette_results(state, snapshot)}
                    }
                }
            } else if overlay == Overlay::Viewer {
                if let Some(viewer) = snapshot.viewer.as_ref() {
                    // Full-resolution bytes are fetched on open, so the element
                    // has to re-request when they land: an unstamped `src` is
                    // painted once and never asked for again.
                    {
                        let uri = viewer.id.uri_at(snapshot.media_epoch);
                        let ready = snapshot.media.is_ready(&viewer.id);
                        rsx! {
                            div { class: "viewer",
                                if !ready {
                                    p { class: "viewer-loading", "Loading {viewer.name}…" }
                                } else if viewer.mime.starts_with("image/") {
                                    img { key: "viewer-{uri}", src: "{uri}", alt: "{viewer.name}" }
                                } else if viewer.mime.starts_with("video/") {
                                    video { key: "viewer-{uri}", src: "{uri}", controls: true, autoplay: true, "{viewer.name}" }
                                } else {
                                    p { "This file type cannot be previewed in the system WebView." }
                                }
                                a {
                                    class: if ready { "primary" } else { "primary disabled" },
                                    href: "{uri}",
                                    download: "{viewer.name}",
                                    "Download {viewer.name}"
                                }
                            }
                        }
                    }
                }
            } else if overlay == Overlay::Accounts {
                div { class: "account-list",
                    for account in &snapshot.accounts {
                        div { class: if account.active { "account-row active" } else { "account-row" }, key: "account-{account.id}",
                            button {
                                disabled: account.active,
                                onclick: {
                                    let id = account.id.clone();
                                    move |_| { spawn(crate::bootstrap::switch_account(state, id.clone())); }
                                },
                                strong { "{account.label}" }
                                if account.active { span { "Current" } }
                            }
                            button {
                                class: "danger",
                                title: "Remove account",
                                onclick: {
                                    let id = account.id.clone();
                                    move |_| { spawn(crate::bootstrap::remove_account(state, id.clone())); }
                                },
                                "Remove"
                            }
                        }
                    }
                    button {
                        class: "settings-open",
                        onclick: move |_| {
                            let mut shell = state.write();
                            shell.settings_section = SettingsSection::Appearance;
                            shell.overlay = Some(Overlay::Settings);
                        },
                        "Settings"
                    }
                    button {
                        class: "primary",
                        onclick: move |_| { spawn(crate::bootstrap::sign_in(state)); },
                        "Add another account"
                    }
                }
            } else if overlay == Overlay::Settings {
                {crate::view::settings::settings_view(state, snapshot)}
            }
        }
    }
}

struct SelfStatusVm {
    text: String,
    glyph: String,
    image: Option<MediaAssetId>,
}

fn self_menu_view(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let account = &snapshot.self_account;
    let currently_active = account.presence == PresenceVm::Active;
    let presence_target = if currently_active { "away" } else { "active" };
    let set_away = currently_active;
    let workspace_name = if account.workspace_name.is_empty() {
        "this workspace".to_owned()
    } else {
        account.workspace_name.clone()
    };
    let self_user = account.user_id.clone();
    let avatar_ready = account
        .avatar
        .as_ref()
        .is_some_and(|avatar| snapshot.media.is_ready(avatar));
    let status = self_status(snapshot);
    let tz_offset = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team))
        .and_then(|workspace| workspace.users.get(&account.user_id))
        .and_then(|user| user.tz_offset)
        .unwrap_or(0);
    let until_tomorrow = crate::bootstrap::minutes_until_local_hour(
        super_platinum_core::state::now_secs(),
        tz_offset,
        8,
    );
    let notifications_label = if account.snoozed { "Paused" } else { "On" };
    let presence_label = if currently_active { "Active" } else { "Away" };
    rsx! {
        div { class: "scrim", onclick: move |_| {
            let mut shell = state.write();
            shell.overlay = None;
            shell.self_menu_notifications_open = false;
        } }
        section {
            class: "self-menu",
            role: "menu",
            "aria-label": "Account menu",
            div { class: "self-menu-identity",
                span { class: "self-menu-avatar",
                    if let Some(avatar) = account.avatar.as_ref().filter(|_| avatar_ready) {
                        img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "" }
                    } else {
                        "{account.initials}"
                    }
                }
                div { class: "self-menu-who",
                    strong { "{account.name}" }
                    span { class: "self-menu-presence",
                        span {
                            class: if currently_active { "profile-presence-dot active" } else { "profile-presence-dot away" },
                            "aria-label": "{presence_label}"
                        }
                        "{presence_label}"
                    }
                }
            }
            if let Some(status) = status {
                div { class: "self-menu-status",
                    if let Some(image) = status.image.as_ref().filter(|image| snapshot.media.is_ready(image)) {
                        img { class: "profile-status-emoji", src: "{image.uri_at(snapshot.media_epoch)}", alt: "" }
                    } else if !status.glyph.is_empty() {
                        span { class: "profile-status-glyph", "{status.glyph}" }
                    }
                    span { class: "self-menu-status-text", "{status.text}" }
                    button {
                        class: "self-menu-status-clear",
                        title: "Clear status",
                        "aria-label": "Clear status",
                        onclick: move |_| { spawn(crate::bootstrap::clear_self_status(state)); },
                        "×"
                    }
                }
            }
            button {
                class: "self-menu-item",
                role: "menuitem",
                onclick: move |_| { spawn(crate::bootstrap::set_self_presence(state, set_away)); },
                span { "Set yourself as " strong { "{presence_target}" } }
            }
            button {
                class: "self-menu-item",
                role: "menuitem",
                "aria-haspopup": "true",
                "aria-expanded": if snapshot.self_menu_notifications_open { "true" } else { "false" },
                onclick: move |_| state.write().toggle_self_menu_notifications(),
                span { "Notifications" }
                span { class: "self-menu-item-meta",
                    "{notifications_label}"
                    span { class: "self-menu-chevron", "›" }
                }
            }
            if snapshot.self_menu_notifications_open {
                if account.snoozed {
                    button {
                        class: "self-menu-item sub",
                        role: "menuitem",
                        onclick: move |_| { spawn(crate::bootstrap::resume_notifications(state)); },
                        "Resume notifications"
                    }
                }
                button {
                    class: "self-menu-item sub",
                    role: "menuitem",
                    onclick: move |_| { spawn(crate::bootstrap::pause_notifications(state, 30)); },
                    "Pause for 30 minutes"
                }
                button {
                    class: "self-menu-item sub",
                    role: "menuitem",
                    onclick: move |_| { spawn(crate::bootstrap::pause_notifications(state, 60)); },
                    "Pause for 1 hour"
                }
                button {
                    class: "self-menu-item sub",
                    role: "menuitem",
                    onclick: move |_| { spawn(crate::bootstrap::pause_notifications(state, 120)); },
                    "Pause for 2 hours"
                }
                button {
                    class: "self-menu-item sub",
                    role: "menuitem",
                    onclick: move |_| { spawn(crate::bootstrap::pause_notifications(state, until_tomorrow)); },
                    "Pause until tomorrow"
                }
            }
            hr { class: "self-menu-rule" }
            button {
                class: "self-menu-item",
                role: "menuitem",
                onclick: move |_| {
                    {
                        let mut shell = state.write();
                        shell.overlay = None;
                        shell.self_menu_notifications_open = false;
                    }
                    spawn(crate::bootstrap::open_profile(state, self_user.clone()));
                },
                "Profile"
            }
            button {
                class: "self-menu-item",
                role: "menuitem",
                onclick: move |_| state.write().open_preferences(),
                "Preferences"
            }
            hr { class: "self-menu-rule" }
            button {
                class: "self-menu-item",
                role: "menuitem",
                onclick: move |_| { spawn(crate::bootstrap::sign_out_workspace(state)); },
                "Sign out of {workspace_name}"
            }
        }
    }
}

fn self_status(snapshot: &ShellState) -> Option<SelfStatusVm> {
    let team = snapshot.core.active_team.as_ref()?;
    let workspace = snapshot.core.workspaces.get(team)?;
    let user = workspace.users.get(&snapshot.self_account.user_id)?;
    let profile = user.profile.as_ref()?;
    let current = profile.status_expiration.unwrap_or(0) == 0
        || profile.status_expiration.unwrap_or(0) > super_platinum_core::state::now_secs();
    if !current {
        return None;
    }
    let text = profile.status_text.clone().unwrap_or_default();
    let status_name = profile
        .status_emoji
        .as_deref()
        .unwrap_or_default()
        .trim_matches(':');
    let image = workspace
        .custom_emoji_url(status_name)
        .map(|url| snapshot.media.register_emoji(status_name, url));
    let glyph = if image.is_none() && super_platinum_core::state::is_standard_emoji(status_name) {
        super_platinum_core::state::emoji_glyph(status_name)
    } else {
        String::new()
    };
    if text.trim().is_empty() && glyph.is_empty() && image.is_none() {
        return None;
    }
    Some(SelfStatusVm { text, glyph, image })
}

fn palette_results(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let matches = snapshot.palette_matches();
    let query_empty = snapshot.palette_query.trim().is_empty();
    let empty = matches.is_empty();
    let show_recents = query_empty && !empty;
    let lock_src = crate::icons::lock_uri();
    rsx! {
        if show_recents {
            div { class: "palette-section", "Recent" }
        }
        for (selection, index) in matches.into_iter().enumerate() {
            if let Some(channel) = snapshot.channels.get(index) {
                {
                    let selected = selection == snapshot.palette_selected;
                    let class = palette_row_class(channel.unread, selected);
                    let name = channel.name.clone();
                    let is_im = channel.is_im;
                    let is_mpim = channel.is_mpim;
                    let is_private = channel.is_private;
                    let initials = channel.avatar_initials.clone();
                    let avatar = channel.avatar.clone();
                    let presence = channel.presence;
                    let mention_count = channel.mention_count;
                    let unread = channel.unread;
                    let unread_count = channel.unread_count;
                    let member_count = channel.member_count;
                    let show_dm_badge = is_im && unread && mention_count == 0 && unread_count > 0;
                    let row_key = channel.id.clone();
                    rsx! {
                        button {
                            class: "{class}",
                            key: "palette-{row_key}",
                            onclick: move |_| {
                                state.write().select_channel(index, crate::state::ChannelOpen::Global);
                                state.write().overlay = None;
                                spawn(crate::bootstrap::refresh_selected_channel(state));
                            },
                            span { class: "palette-icon",
                                if is_im {
                                    span { class: "palette-avatar-wrap",
                                        if let Some(avatar) = avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                                            img {
                                                class: "palette-avatar",
                                                src: "{avatar.uri_at(snapshot.media_epoch)}",
                                                alt: "{name}"
                                            }
                                        } else {
                                            span { class: "palette-avatar placeholder", "{initials}" }
                                        }
                                        span { class: if presence == crate::model::PresenceVm::Active { "palette-presence active" } else { "palette-presence" } }
                                    }
                                } else if is_mpim {
                                    span { class: "palette-mpdm", "{member_count.unwrap_or(0)}" }
                                } else if is_private {
                                    img { class: "icon sm", src: "{lock_src}", alt: "private" }
                                } else {
                                    span { class: "palette-hash", "#" }
                                }
                            }
                            span { class: "palette-name", "{name}" }
                            if mention_count > 0 {
                                span { class: "ping-badge",
                                    if mention_count > 99 { "99+" } else { "{mention_count}" }
                                }
                            } else if show_dm_badge {
                                span { class: "ping-badge",
                                    if unread_count > 99 { "99+" } else { "{unread_count}" }
                                }
                            } else if unread && !is_im {
                                i { class: "unread-dot" }
                            }
                        }
                    }
                }
            }
        }
        if empty && query_empty {
            p { class: "results-empty", "No recent conversations yet" }
        } else if empty {
            p { class: "results-empty", "No matching conversations" }
        }
    }
}

fn palette_row_class(unread: bool, selected: bool) -> &'static str {
    match (unread, selected) {
        (true, true) => "palette-result unread selected",
        (true, false) => "palette-result unread",
        (false, true) => "palette-result selected",
        (false, false) => "palette-result",
    }
}
