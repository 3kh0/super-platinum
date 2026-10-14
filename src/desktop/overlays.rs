use crate::media::MediaUri;
use dioxus::prelude::*;
use super_platinum_core::MediaAssetId;

use crate::state::{Overlay, PresenceVm, SettingsSection, ShellState};

pub fn overlay_view(
    mut state: Signal<ShellState>,
    overlay: Overlay,
    snapshot: &ShellState,
) -> Element {
    if overlay == Overlay::ButtonConfirmation {
        return crate::view::button_confirmation::button_confirmation(state);
    }
    if overlay == Overlay::MessageExternalSelect {
        return crate::view::message_external::dialog(state);
    }
    if overlay == Overlay::MessageUserSelect {
        return crate::view::message_user::dialog(state);
    }
    if overlay == Overlay::MessageMultiSelect {
        return crate::view::message_multi::dialog(state);
    }
    if overlay == Overlay::AppModal {
        return crate::view::app_modal::app_modal(state);
    }
    if overlay == Overlay::CanvasCreate {
        return rsx! {crate::canvas_create::CreateDialog {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team:?}-{snapshot.shortcut_generation}",state}};
    }
    if overlay == Overlay::Canvas {
        let transport_identity = snapshot
            .core
            .transport
            .as_ref()
            .map(|t| std::sync::Arc::as_ptr(t) as usize);
        return snapshot.canvas.as_ref().map(|target|rsx! {crate::canvas::CanvasDialog {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team:?}-{snapshot.shortcut_generation}-{transport_identity:?}-{target.id}",state,target:target.clone()}}).unwrap_or_else(||rsx!{});
    }
    if overlay == Overlay::NewDm {
        return rsx! { crate::new_dm::NewDmDialog {key:"{snapshot.core.account_epoch}-{snapshot.core.active_team:?}-{snapshot.palette_generation}", state } };
    }
    if overlay == Overlay::SlackList {
        return crate::view::slack_lists::dialog(state);
    }
    if overlay == Overlay::ProfileTitle {
        return crate::view::profile_title::profile_title(state);
    }
    if overlay == Overlay::ProfileEdit {
        return crate::view::profile_edit::profile_edit(state);
    }
    if overlay == Overlay::SelfStatus {
        return crate::view::self_status::self_status(state);
    }
    if overlay == Overlay::SelfMenu {
        return self_menu_view(state, snapshot);
    }
    if overlay == Overlay::ComposerEmoji {
        return rsx! { crate::composer_emoji::Picker { state } };
    }
    if overlay == Overlay::ReactionPicker {
        return rsx! { crate::reaction_picker::ReactionPicker { state } };
    }
    if overlay == Overlay::Forward {
        return rsx! { crate::forward::ForwardDialog {key:"{snapshot.forward_target.as_ref().map(|t|t.id.as_str()).unwrap_or_default()}", state } };
    }
    let (title, input_value, placeholder) = match overlay {
        Overlay::ButtonConfirmation => unreachable!("button confirmation owns its dialog"),
        Overlay::MessageExternalSelect => unreachable!("external select owns its dialog"),
        Overlay::MessageUserSelect => unreachable!("message user select owns its dialog"),
        Overlay::MessageMultiSelect => unreachable!("message multi select owns its dialog"),
        Overlay::AppModal => unreachable!("app form owns its dialog"),
        Overlay::SelfMenu => unreachable!("self menu renders as a rail popover"),
        Overlay::SlackList => unreachable!("list owns its dialog"),
        Overlay::SelfStatus => unreachable!("status editor owns its dialog"),
        Overlay::ProfileTitle => unreachable!("title editor owns its dialog"),
        Overlay::ProfileEdit => unreachable!("profile editor owns its dialog"),
        Overlay::NewDm => unreachable!("recipient picker owns its dialog"),
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
        Overlay::Canvas | Overlay::CanvasCreate => unreachable!("canvas owns its dialog"),
        Overlay::Viewer => (
            snapshot
                .viewer
                .as_ref()
                .map(|v| v.name.as_str())
                .unwrap_or("File preview"),
            "",
            "",
        ),
        Overlay::Forward => unreachable!("forward dialog owns its surface"),
        Overlay::ComposerEmoji => unreachable!("composer emoji owns its dialog"),
        Overlay::ReactionPicker => unreachable!("reaction picker owns its dialog"),
    };
    let compact = overlay == Overlay::Palette;
    rsx! {
        div { class: "scrim", onclick: move |_| state.write().dismiss_overlay() }
        section { class: if compact { "modal compact" } else if overlay == Overlay::Search { "modal search-dialog" } else if overlay == Overlay::Viewer { "modal file-viewer-dialog" } else { "modal" }, role: "dialog", "aria-label": "{title}", "aria-modal": "true",
            id: "shell-overlay-dialog",
            tabindex: "-1",
            "data-keyboard-dialog": "true",
            "data-dialog-generation": snapshot.shortcut_generation,
            "data-dialog-kind": if compact { "palette" } else if overlay == Overlay::Search { "search" } else { "other" },
            if !compact {
                header {
                    h2 { "{title}" }
                    button { "aria-label": "Close", onclick: move |_| state.write().dismiss_overlay(), {crate::icons::icon(crate::icons::Icon::Close, "icon")} }
                }
            }
            if matches!(overlay, Overlay::Palette | Overlay::Search) {
                input {
                    id: "overlay-input",
                    autocomplete: "off",
                    "autocorrect": "off",
                    autocapitalize: "none",
                    spellcheck: "false",
                    "data-focus-at-end": overlay == Overlay::Search && snapshot.search_focus_at_end,
                    "data-dialog-input": "true",
                    "data-dialog-value": "{input_value}",
                    // Keep native typing uncontrolled: round-trip value updates
                    // can overwrite newer keystrokes in the desktop WebView.
                    key: "{overlay:?}-{snapshot.search_input_revision}",
                    initial_value: "{input_value}",
                    placeholder: "{placeholder}",
                    "aria-label": "{placeholder}",
                    oninput: move |event| match overlay {
                        Overlay::Palette => {
                            let mut shell = state.write();
                            shell.set_palette_query(event.value());
                        },
                        Overlay::Search => state.write().set_search_query(event.value()),
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
                                    crate::bootstrap::open_palette_entry(state, None);
                                }
                                _ => {}
                            }
                        }
                    },
                }
                if overlay == Overlay::Search { {crate::view::search::controls(state, snapshot)} }
                div { class: "results", hidden: overlay == Overlay::Search && snapshot.search_filters.is_some(),
                    if overlay == Overlay::Search {
                        if snapshot.search_pager.loading && snapshot.search_results.is_empty() {
                            p { class: "results-empty", role: "status", "Searching…" }
                        }
                        if snapshot.search_pager.page > 0 {
                            p { class: "search-summary", role: "status", "{snapshot.search_results.len()} of {snapshot.search_pager.total} results · {snapshot.search_pager.sort.label()}" }
                        }
                        if snapshot.search_opening.is_some() { p { class: "results-empty", role: "status", "Opening conversation…" } }
                        for result in &snapshot.search_results {
                            button {
                                key: "{result.channel_id}-{result.ts}",
                                class: "search-result",
                                onclick: {
                                    let hit = result.clone();
                                    move |_| crate::bootstrap::open_search_hit(state, hit.clone())
                                },
                                strong { "# {result.channel_name} · {result.author}" }
                                span { "{result.text}" }
                                span { class: "search-time",
                                    {super_platinum_core::state::format_relative_ts(&result.ts)}
                                    if result.thread_ts.is_some() { " · Thread reply" }
                                }
                            }
                        }
                        if !snapshot.search_pager.loading && !snapshot.search_pager.failed && snapshot.search_results.is_empty() {
                            p { class: "results-empty",
                                if snapshot.search_pager.page > 0 { "No messages found. Try a different search." }
                                else { "Press Enter to search. Use in:channel, from:person, or has:file to narrow results." }
                            }
                        }
                    } else {
                        {palette_results(state, snapshot)}
                    }
                }
                if overlay == Overlay::Search && snapshot.search_filters.is_none() && (snapshot.search_pager.has_more() || snapshot.search_pager.failed) {
                    div { class: "search-footer",
                        if snapshot.search_pager.failed {
                            p { role: "status", "Search couldn’t load. Try again." }
                        }
                        button { class: "chip", disabled: snapshot.search_pager.loading,
                            onclick: move |_| {
                                if state.peek().search_pager.page > 0 { spawn(crate::bootstrap::search_more(state)); }
                                else { spawn(crate::bootstrap::search(state)); }
                            },
                            if snapshot.search_pager.loading { "Loading…" }
                            else if snapshot.search_pager.failed { "Retry search" }
                            else { "Load more results" }
                        }
                    }
                }
            } else if overlay == Overlay::Viewer {
                if let Some(viewer) = snapshot.viewer.as_ref() {
                    crate::file_preview::FileViewer {state,viewer:viewer.clone(),media_epoch:snapshot.media_epoch}

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
                        img { src: "{avatar.webview_uri_at(snapshot.media_epoch)}", alt: "" }
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
                        img { class: "profile-status-emoji", src: "{image.webview_uri_at(snapshot.media_epoch)}", alt: "" }
                    } else if !status.glyph.is_empty() {
                        span { class: "profile-status-glyph", "{status.glyph}" }
                    }
                    span { class: "self-menu-status-text", "{status.text}" }
                    button {
                        class: "self-menu-status-clear",
                        title: "Clear status",
                        "aria-label": "Clear status",
                        onclick: move |_| { spawn(crate::bootstrap::clear_self_status(state)); },
                        {crate::icons::icon(crate::icons::Icon::Close, "icon sm")}
                    }
                }
            }
            button {
                class: "self-menu-item",
                role: "menuitem",
                onclick: move |_| crate::self_status::open(state),
                "Set your status"
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
                    span { class: "self-menu-chevron", {crate::icons::icon(crate::icons::Icon::ChevronRight, "icon sm")} }
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

#[cfg(test)]
mod tests {
    use super::self_status;
    use crate::media::MediaRegistry;
    use crate::state::ShellState;

    #[test]
    fn self_menu_status_resolves_custom_emoji_to_an_image() {
        let mut state = ShellState::fixture_variant(MediaRegistry::default(), "self-menu");
        let workspace = state.core.workspaces.get_mut("T1").unwrap();
        workspace.apply_emojis(vec![super_platinum_core::slack::models::Emoji {
            name: "ship".into(),
            value: "https://example.test/ship.png".into(),
            ..Default::default()
        }]);
        let profile = workspace
            .users
            .get_mut("U0")
            .unwrap()
            .profile
            .as_mut()
            .unwrap();
        profile.status_emoji = Some(":ship:".into());

        let status = self_status(&state).expect("status");
        assert!(status.image.is_some());
        assert!(status.glyph.is_empty());
    }
}

fn palette_results(state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let matches = snapshot.palette_matches();
    let query_empty = snapshot.palette_query.trim().is_empty();
    let empty = matches.is_empty();
    let show_recents = query_empty && !empty;
    rsx! {
        if show_recents {
            div { class: "palette-section", "Recent" }
        }
        if snapshot.palette_opening.is_some() { p { class: "results-empty", role: "status", "Opening direct message…" } }
        for (selection, entry) in matches.into_iter().enumerate() {
            {let channel = &entry.row; rsx! {
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
                    let target = entry.target.clone();
                    rsx! {
                        button {
                            class: "{class}",
                            key: "palette-{row_key}",
                            onclick: move |_| {
                                crate::bootstrap::open_palette_target(state, target.clone());
                            },
                            span { class: "palette-icon", "aria-hidden": "true",
                                if is_im {
                                    span { class: "palette-avatar-wrap",
                                        if let Some(avatar) = avatar.as_ref().filter(|avatar| snapshot.media.is_ready(avatar)) {
                                            img {
                                                class: "palette-avatar",
                                                src: "{avatar.webview_uri_at(snapshot.media_epoch)}",
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
                                    {crate::icons::icon(crate::icons::Icon::Lock, "icon sm")}
                                } else {
                                    span { class: "palette-hash", {crate::icons::icon(crate::icons::Icon::Tag, "icon sm")} }
                                }
                            }
                            span { class: "palette-name", "{name}"
                                if is_im && !entry.handle.is_empty() { span { class: "palette-handle", "{entry.handle}" } }
                            }
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
            }}
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
