//! Settings overlay: appearance controls and the Storage cache manager.

use dioxus::prelude::*;

use super_platinum_core::config::CacheSizeLimit;
use super_platinum_core::{MediaCacheKind, state::format_file_size};

use crate::state::{SettingsSection, ShellState, StorageUsageVm};

pub(crate) fn settings_view(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    rsx! {
        div { class: "settings-tabs", role: "tablist",
            button {
                class: if snapshot.settings_section == SettingsSection::Appearance { "settings-tab active" } else { "settings-tab" },
                role: "tab",
                "aria-selected": (snapshot.settings_section == SettingsSection::Appearance).to_string(),
                onclick: move |_| state.write().set_settings_section(SettingsSection::Appearance),
                "Appearance"
            }
            button {
                class: if snapshot.settings_section == SettingsSection::Storage { "settings-tab active" } else { "settings-tab" },
                role: "tab",
                "aria-selected": (snapshot.settings_section == SettingsSection::Storage).to_string(),
                onclick: move |_| {
                    state.write().set_settings_section(SettingsSection::Storage);
                    spawn(crate::bootstrap::refresh_storage(state));
                },
                "Storage"
            }
        }
        if snapshot.settings_section == SettingsSection::Storage {
            {storage_panel(state, snapshot)}
        } else {
            {appearance_panel(state, snapshot)}
        }
    }
}

fn appearance_panel(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    rsx! {
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

fn storage_panel(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let usage = snapshot.storage.usage;
    let total = usage.total();
    let selected_bytes = snapshot.storage.selected_picture_bytes();
    let can_clear = selected_bytes > 0;
    let hole = if snapshot.storage.scanning && total == 0 {
        "Measuring…".to_owned()
    } else {
        format_file_size(total)
    };
    let clear_label = format!("Clear selected cache {}", format_file_size(selected_bytes));
    rsx! {
        div { class: "storage-panel",
            div { class: "storage-hero",
                {storage_donut(usage, snapshot.storage.scanning, hole)}
            }
            div { class: "storage-list",
                for kind in MediaCacheKind::ALL {
                    {
                        let selected = snapshot.storage.selected[kind.index()];
                        let bytes = usage.kind(kind);
                        let class = if selected {
                            format!("storage-row selected {}", kind_class(kind))
                        } else {
                            format!("storage-row {}", kind_class(kind))
                        };
                        rsx! {
                            button {
                                key: "cache-{kind:?}",
                                class: "{class}",
                                role: "checkbox",
                                "aria-checked": selected.to_string(),
                                onclick: move |_| state.write().toggle_cache_kind(kind),
                                span { class: "storage-swatch", "aria-hidden": "true",
                                    if selected { {crate::icons::icon(crate::icons::Icon::Check, "storage-check-icon")} }
                                }
                                span { class: "storage-name",
                                    "{kind.label()} "
                                    span { class: "storage-pct", {percent(bytes, total)} }
                                }
                                span { class: "storage-size", {format_file_size(bytes)} }
                            }
                        }
                    }
                }
                button {
                    class: if can_clear { "storage-clear" } else { "storage-clear disabled" },
                    disabled: !can_clear,
                    onclick: move |_| { spawn(crate::bootstrap::clear_picture_cache(state)); },
                    "{clear_label}"
                }
                p { class: "storage-hint",
                    "All media will stay on Slack and can be re-downloaded if you need them again."
                }
            }
            div { class: "storage-list",
                div { class: "storage-row workspace",
                    span { class: "storage-swatch", "aria-hidden": "true" }
                    span { class: "storage-name",
                        "Workspace history "
                        span { class: "storage-pct", {percent(usage.workspace, total)} }
                    }
                    span { class: "storage-size", {format_file_size(usage.workspace)} }
                    button {
                        class: "storage-inline-clear",
                        onclick: move |_| { spawn(crate::bootstrap::clear_workspace_cache(state)); },
                        "Clear"
                    }
                }
            }
            div { class: "storage-limit",
                p { class: "storage-limit-label", "Maximum cache size" }
                input {
                    r#type: "range",
                    min: "0",
                    max: "4",
                    step: "1",
                    value: "{snapshot.core.settings.cache_limit.index()}",
                    "aria-valuetext": "{snapshot.core.settings.cache_limit.label()}",
                    oninput: move |event| {
                        if let Ok(index) = event.value().parse::<u8>() {
                            state.write().set_cache_limit(CacheSizeLimit::from_index(index));
                        }
                    },
                    onchange: move |_| { spawn(crate::bootstrap::apply_cache_limit(state)); },
                }
                div { class: "storage-limit-stops",
                    for limit in CacheSizeLimit::ALL {
                        span {
                            class: if snapshot.core.settings.cache_limit == limit { "active" } else { "" },
                            "{limit.label()}"
                        }
                    }
                }
                p { class: "storage-hint",
                    "If your cache exceeds this limit, the oldest unused media will be removed from your device."
                }
            }
        }
    }
}

fn storage_donut(usage: StorageUsageVm, scanning: bool, hole: String) -> Element {
    let empty = usage.total() == 0;
    let class = if empty {
        "storage-donut empty"
    } else {
        "storage-donut"
    };
    let style = donut_style(usage);
    rsx! {
        div { class: "{class}", style: "{style}",
            div { class: "storage-donut-hole",
                strong { "{hole}" }
                span { if scanning && !empty { "updating" } else { "cached" } }
            }
        }
    }
}

fn donut_style(usage: StorageUsageVm) -> String {
    let total = usage.total();
    if total == 0 {
        return String::new();
    }
    let t = total as f32;
    let a = usage.avatars as f32 / t * 100.0;
    let e = a + usage.emoji as f32 / t * 100.0;
    let i = e + usage.icons as f32 / t * 100.0;
    let o = i + usage.other as f32 / t * 100.0;
    format!(
        "background: conic-gradient(\
            var(--storage-avatars) 0 {a:.3}%, \
            var(--storage-emoji) {a:.3}% {e:.3}%, \
            var(--storage-icons) {e:.3}% {i:.3}%, \
            var(--storage-other) {i:.3}% {o:.3}%, \
            var(--storage-workspace) {o:.3}% 100%)"
    )
}

fn percent(part: u64, total: u64) -> String {
    if total == 0 {
        return "0%".into();
    }
    format!("{:.1}%", part as f64 / total as f64 * 100.0)
}

fn kind_class(kind: MediaCacheKind) -> &'static str {
    match kind {
        MediaCacheKind::Avatars => "avatars",
        MediaCacheKind::Emoji => "emoji",
        MediaCacheKind::Icons => "icons",
        MediaCacheKind::Other => "other",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StoragePanel;

    #[test]
    fn selected_bytes_follow_the_checkboxes() {
        let mut panel = StoragePanel {
            usage: StorageUsageVm {
                avatars: 100,
                emoji: 40,
                icons: 10,
                other: 5,
                workspace: 999,
            },
            selected: [true, true, false, false],
            scanning: false,
            fixture: true,
        };
        assert_eq!(panel.selected_picture_bytes(), 140);
        panel.selected[1] = false;
        assert_eq!(panel.selected_picture_bytes(), 100);
    }

    #[test]
    fn donut_is_empty_when_nothing_is_cached() {
        assert!(donut_style(StorageUsageVm::default()).is_empty());
        assert_eq!(percent(0, 0), "0%");
        assert_eq!(percent(25, 100), "25.0%");
    }
}
