use dioxus::prelude::*;

use crate::state::{Overlay, ShellState};

pub fn overlay_view(
    mut state: Signal<ShellState>,
    overlay: Overlay,
    snapshot: &ShellState,
) -> Element {
    let (title, input_value, placeholder) = match overlay {
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
        Overlay::Profile => ("Profile", "", ""),
        Overlay::Viewer => ("Media viewer", "", ""),
    };
    let workspace = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|team| snapshot.core.workspaces.get(team));
    let profile_user_id = snapshot.profile_user.clone();
    let profile = profile_user_id
        .as_ref()
        .and_then(|user| workspace.and_then(|ws| ws.users.get(user)));
    let profile_name = profile
        .and_then(|user| {
            user.profile.as_ref().and_then(|profile| {
                profile
                    .display_name
                    .clone()
                    .filter(|name| !name.is_empty())
                    .or_else(|| profile.real_name.clone())
            })
        })
        .or_else(|| profile.and_then(|user| user.real_name.clone()))
        .or_else(|| profile_user_id.clone())
        .unwrap_or_else(|| "Profile details".into());
    let profile_title = profile
        .and_then(|user| user.profile.as_ref())
        .and_then(|profile| profile.title.clone())
        .unwrap_or_default();
    let profile_status = profile
        .and_then(|user| user.profile.as_ref())
        .and_then(|profile| profile.status_text.clone())
        .unwrap_or_default();
    let profile_status_emoji = profile
        .and_then(|user| user.profile.as_ref())
        .and_then(|profile| profile.status_emoji.clone())
        .map(|emoji| emoji.trim_matches(':').to_owned())
        .filter(|emoji| !emoji.is_empty())
        .map(|name| super_platinum_core::state::emoji_glyph(&name))
        .unwrap_or_default();
    let profile_email = profile
        .and_then(|user| user.profile.as_ref())
        .and_then(|profile| profile.email.clone())
        .unwrap_or_default();
    let profile_pronouns = profile
        .and_then(|user| user.profile.as_ref())
        .and_then(|profile| profile.pronouns.clone())
        .unwrap_or_default();
    let presence = profile_user_id
        .as_ref()
        .and_then(|user| workspace.and_then(|ws| ws.presence.get(user).copied()))
        .map(crate::model::PresenceVm::from_core)
        .unwrap_or_default();
    let profile_avatar = profile.and_then(|user| {
        super_platinum_core::state::user_profile_image_url(user).map(|url| {
            snapshot.media.register(
                super_platinum_core::MediaAssetKind::Avatar,
                url,
                "image/jpeg",
                true,
            )
        })
    });
    let profile_initials = profile_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "S".into());
    let local_time = profile
        .and_then(|user| super_platinum_core::state::format_user_local_time(user.tz_offset))
        .unwrap_or_default();
    let deactivated = profile.is_some_and(|user| user.deleted);
    rsx! {
        div { class: "scrim", onclick: move |_| state.write().overlay = None }
        section { class: "modal", role: "dialog", "aria-modal": "true",
            header {
                h2 { "{title}" }
                button { onclick: move |_| state.write().overlay = None, "×" }
            }
            if matches!(overlay, Overlay::Palette | Overlay::Search) {
                input {
                    autofocus: true,
                    value: "{input_value}",
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
                        for (selection, index) in snapshot.palette_matches().into_iter().enumerate() {
                            if let Some(channel) = snapshot.channels.get(index).cloned() {
                                {
                                    let glyph = if channel.is_im {
                                        "●"
                                    } else if channel.is_mpim {
                                        "👥"
                                    } else if channel.is_private {
                                        "🔒"
                                    } else {
                                        "#"
                                    };
                                    let name = channel.name.clone();
                                    let unread = channel.unread;
                                    rsx! {
                                        button {
                                            class: if selection == snapshot.palette_selected { "palette-result selected" } else { "palette-result" },
                                            onclick: move |_| {
                                                state.write().select_channel(index);
                                                state.write().overlay = None;
                                                spawn(crate::bootstrap::refresh_selected_channel(state));
                                            },
                                            "{glyph} {name}"
                                            if unread { span { " · unread" } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if overlay == Overlay::Viewer {
                if let Some(viewer) = snapshot.viewer.as_ref() {
                    div { class: "viewer",
                        if viewer.mime.starts_with("image/") {
                            img { src: "{viewer.id.uri()}", alt: "{viewer.name}" }
                        } else if viewer.mime.starts_with("video/") {
                            video { src: "{viewer.id.uri()}", controls: true, autoplay: true, "{viewer.name}" }
                        } else {
                            p { "This file type cannot be previewed in the system WebView." }
                        }
                        a { class: "primary", href: "{viewer.id.uri()}", download: "{viewer.name}", "Download {viewer.name}" }
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
                        class: "primary",
                        onclick: move |_| { spawn(crate::bootstrap::sign_in(state)); },
                        "Add another account"
                    }
                }
            } else if overlay == Overlay::Settings {
                div { class: "settings-grid",
                    label { "Theme"
                        select {
                            value: match snapshot.core.settings.preset { super_platinum_core::config::ThemePreset::Countertop => "countertop", super_platinum_core::config::ThemePreset::BlueSteel => "blue_steel", super_platinum_core::config::ThemePreset::PaperBag => "paper_bag" },
                            onchange: move |event| {
                                state.write().set_theme_preset(&event.value());
                                spawn(crate::bootstrap::persist_settings(state));
                            },
                            option { value: "countertop", "Jet Black" }
                            option { value: "blue_steel", "Midnight Blue" }
                            option { value: "paper_bag", "Obsidian" }
                        }
                    }
                    label { "Density"
                        select {
                            value: if snapshot.core.settings.gap <= 6.0 { "compact" } else { "comfortable" },
                            onchange: move |event| {
                                state.write().set_density(&event.value());
                                spawn(crate::bootstrap::persist_settings(state));
                            },
                            option { value: "comfortable", "Comfortable" }
                            option { value: "compact", "Compact" }
                        }
                    }
                    label { "Sidebar width"
                        input {
                            r#type: "range",
                            min: "180",
                            max: "520",
                            value: "{snapshot.core.settings.sidebar_width}",
                            oninput: move |event| {
                                if let Ok(width) = event.value().parse() { state.write().set_sidebar_width(width); }
                            },
                            onchange: move |_| { spawn(crate::bootstrap::persist_settings(state)); }
                        }
                    }
                    label { "Panel radius"
                        input { r#type: "range", min: "0", max: "20", step: "1", value: "{snapshot.core.settings.panel_radius}", oninput: move |event| if let Ok(value) = event.value().parse() { state.write().set_panel_radius(value) }, onchange: move |_| { spawn(crate::bootstrap::persist_settings(state)); } }
                    }
                    label { "Border thickness"
                        input { r#type: "range", min: "0", max: "4", step: "0.5", value: "{snapshot.core.settings.border_thickness}", oninput: move |event| if let Ok(value) = event.value().parse() { state.write().set_border_thickness(value) }, onchange: move |_| { spawn(crate::bootstrap::persist_settings(state)); } }
                    }
                    for role in super_platinum_core::config::ColorRole::ALL {
                        label { key: "role-{role:?}", "{role.label()} color"
                            input {
                                r#type: "color",
                                value: "{role_color(snapshot, role)}",
                                onchange: move |event| { state.write().set_role_color(role, &event.value()); spawn(crate::bootstrap::persist_settings(state)); }
                            }
                        }
                    }
                    button { onclick: move |_| { state.write().reset_role_colors(); spawn(crate::bootstrap::persist_settings(state)); }, "Reset custom colors" }
                    label { "Background"
                        span { class: "background-actions",
                            label { class: "background-picker",
                                "Choose image"
                                input {
                                    r#type: "file",
                                    accept: "image/png,image/jpeg,image/webp",
                                    onchange: move |event| {
                                        if let Some(file) = event.files().into_iter().next() {
                                            spawn(crate::appearance::import_background(state, file.path()));
                                        }
                                    }
                                }
                            }
                            if snapshot.core.settings.background.is_some() {
                                button { onclick: move |_| { spawn(crate::appearance::clear_background(state)); }, "Remove" }
                            }
                        }
                    }
                    if let Some(background) = snapshot.core.settings.background.as_ref() {
                        label { "Background fit"
                            select { value: if background.fit == super_platinum_core::config::BackgroundFit::Contain { "contain" } else { "cover" }, onchange: move |event| { state.write().set_background_fit(&event.value()); spawn(crate::bootstrap::persist_settings(state)); }, option { value: "cover", "Cover" } option { value: "contain", "Contain" } }
                        }
                        label { "Background dim"
                            input { r#type: "range", min: "0", max: "1", step: "0.05", value: "{background.dim}", oninput: move |event| if let Ok(value) = event.value().parse() { state.write().set_background_dim(value) }, onchange: move |_| { spawn(crate::bootstrap::persist_settings(state)); } }
                        }
                        label { "Surface opacity"
                            input { r#type: "range", min: "0", max: "1", step: "0.05", value: "{background.surface_opacity}", oninput: move |event| if let Ok(value) = event.value().parse() { state.write().set_surface_opacity(value) }, onchange: move |_| { spawn(crate::bootstrap::persist_settings(state)); } }
                        }
                    }
                }
            } else {
                div { class: "profile-card",
                    div { class: "profile-avatar",
                        if let Some(avatar) = profile_avatar.as_ref() {
                            img {
                                key: "profile-{snapshot.media_epoch}-{avatar.uri()}",
                                src: "{avatar.uri()}",
                                alt: "{profile_name}"
                            }
                        } else {
                            "{profile_initials}"
                        }
                    }
                    h3 { "{profile_name}" }
                    if deactivated { p { class: "profile-status", "Deactivated account" } }
                    p { class: "profile-presence", "{presence.label()}" }
                    if !profile_pronouns.is_empty() { p { "{profile_pronouns}" } }
                    if !profile_title.is_empty() { p { "{profile_title}" } }
                    if !profile_status.is_empty() || !profile_status_emoji.is_empty() {
                        p { class: "profile-status",
                            if !profile_status_emoji.is_empty() { span { "{profile_status_emoji} " } }
                            "{profile_status}"
                        }
                    }
                    if !local_time.is_empty() { p { "{local_time}" } }
                    if !profile_email.is_empty() { span { "{profile_email}" } }
                    if let Some(user) = profile_user_id.clone() {
                        button {
                            class: "primary profile-dm",
                            onclick: move |_| {
                                // Prefer opening an existing IM channel with this user.
                                let index = {
                                    state.read().channels.iter().position(|channel| {
                                        channel.is_im && channel.user_id.as_deref() == Some(user.as_str())
                                    })
                                };
                                if let Some(index) = index {
                                    state.write().select_channel(index);
                                    state.write().overlay = None;
                                    spawn(crate::bootstrap::refresh_selected_channel(state));
                                } else {
                                    state.write().toast = Some("No direct message channel is loaded yet.".into());
                                }
                            },
                            "Message"
                        }
                    }
                }
            }
        }
    }
}

fn role_color(snapshot: &ShellState, role: super_platinum_core::config::ColorRole) -> String {
    snapshot
        .core
        .settings
        .colors
        .get(role)
        .map(|color| color.as_hex())
        .unwrap_or_else(|| {
            crate::view::theme::preset_role_color(snapshot.core.settings.preset, role).into()
        })
}
