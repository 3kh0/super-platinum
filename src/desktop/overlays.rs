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
        Overlay::Viewer => ("Media viewer", "", ""),
    };
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
                        {palette_results(state, snapshot)}
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
            }
        }
    }
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
                                state.write().select_channel(index);
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
