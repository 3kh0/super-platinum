pub(crate) fn theme_css(
    settings: &super_platinum_core::config::Settings,
    background_uri: Option<&str>,
) -> String {
    let palette = preset_palette(settings.preset);
    let accent = settings
        .colors
        .primary
        .map(|color| color.as_hex())
        .unwrap_or_else(|| palette.accent.into());
    let color = |role, fallback: &str| {
        settings
            .colors
            .get(role)
            .map(|color| color.as_hex())
            .unwrap_or_else(|| fallback.into())
    };
    let hover = color(super_platinum_core::config::ColorRole::Hover, palette.soft);
    let mention = color(
        super_platinum_core::config::ColorRole::Mention,
        palette.mention,
    );
    let success = color(
        super_platinum_core::config::ColorRole::Success,
        palette.success,
    );
    let warning = color(
        super_platinum_core::config::ColorRole::Warning,
        palette.warning,
    );
    let danger = color(
        super_platinum_core::config::ColorRole::Danger,
        palette.danger,
    );
    let background = background_uri.map(|uri| {
        let (fit, dim, opacity) = settings.background.as_ref().map(|background| (
            match background.fit { super_platinum_core::config::BackgroundFit::Cover => "cover", super_platinum_core::config::BackgroundFit::Contain => "contain" },
            background.dim.clamp(0.0, 1.0),
            background.surface_opacity.clamp(0.0, 1.0),
        )).unwrap_or(("cover", 0.45, 0.88));
        format!(".conversation {{ background-image: linear-gradient(rgba(0,0,0,{dim}), rgba(0,0,0,{dim})), url('{uri}'); background-size:{fit}; background-position:center; }} .conversation-header, .composer-wrap {{ background-color: color-mix(in srgb, var(--surface) {}%, transparent); }}", opacity * 100.0)
    }).unwrap_or_default();
    format!(
        ":root {{ color-scheme:dark; --bg:{}; --surface:{}; --surface-raised:{}; --panel:{}; --sidebar:{}; --rail:{}; --border:{}; --border-strong:{}; --text:{}; --muted:{}; --muted-2:{}; --accent:{accent}; --accent-soft:{hover}; --accent-faint:{}; --mention:{mention}; --success:{success}; --warning:{warning}; --danger:{danger}; --link:{}; --avatar-bg:{}; --hover-overlay:rgb(47 140 255 / 9%); --on-accent:#FFFFFF; --on-warning:#07111D; --sidebar-width:{}px; --density-gap:{}px; --panel-radius:{}px; --border-thickness:{}px; }}",
        palette.bg,
        palette.surface,
        palette.raised,
        palette.panel,
        palette.sidebar,
        palette.rail,
        palette.border,
        palette.border_strong,
        palette.text,
        palette.muted,
        palette.muted_2,
        palette.faint,
        palette.link,
        palette.avatar,
        settings.sidebar_width,
        settings.gap,
        settings.panel_radius,
        settings.border_thickness
    ) + &background
}

pub(crate) fn preset_role_color(
    preset: super_platinum_core::config::ThemePreset,
    role: super_platinum_core::config::ColorRole,
) -> &'static str {
    let palette = preset_palette(preset);
    match role {
        super_platinum_core::config::ColorRole::Primary => palette.accent,
        super_platinum_core::config::ColorRole::Hover => palette.soft,
        super_platinum_core::config::ColorRole::Mention => palette.mention,
        super_platinum_core::config::ColorRole::Success => palette.success,
        super_platinum_core::config::ColorRole::Warning => palette.warning,
        super_platinum_core::config::ColorRole::Danger => palette.danger,
    }
}

#[derive(Clone, Copy)]
struct ThemePalette {
    bg: &'static str,
    surface: &'static str,
    raised: &'static str,
    panel: &'static str,
    sidebar: &'static str,
    rail: &'static str,
    border: &'static str,
    border_strong: &'static str,
    text: &'static str,
    muted: &'static str,
    muted_2: &'static str,
    accent: &'static str,
    soft: &'static str,
    faint: &'static str,
    mention: &'static str,
    success: &'static str,
    warning: &'static str,
    danger: &'static str,
    link: &'static str,
    avatar: &'static str,
}

fn preset_palette(preset: super_platinum_core::config::ThemePreset) -> ThemePalette {
    match preset {
        super_platinum_core::config::ThemePreset::Countertop => ThemePalette {
            bg: "#020305",
            surface: "#070A0F",
            raised: "#0C1119",
            panel: "#090D14",
            sidebar: "#05070B",
            rail: "#010204",
            border: "#162231",
            border_strong: "#28435F",
            text: "#F3F7FC",
            muted: "#93A4B7",
            muted_2: "#60758C",
            accent: "#2F8CFF",
            soft: "#0B2848",
            faint: "#071A2C",
            mention: "#42A5FF",
            success: "#42D392",
            warning: "#F5B942",
            danger: "#FF647C",
            link: "#69B1FF",
            avatar: "#123A61",
        },
        super_platinum_core::config::ThemePreset::BlueSteel => ThemePalette {
            bg: "#02060B",
            surface: "#06101A",
            raised: "#0B1825",
            panel: "#08131F",
            sidebar: "#040B12",
            rail: "#010408",
            border: "#163149",
            border_strong: "#275276",
            text: "#F1F7FD",
            muted: "#8FA7BC",
            muted_2: "#5F7B92",
            accent: "#4DA3FF",
            soft: "#0A3159",
            faint: "#072038",
            mention: "#72B7FF",
            success: "#42D9A0",
            warning: "#F4C15D",
            danger: "#FF6B82",
            link: "#7FC0FF",
            avatar: "#124268",
        },
        super_platinum_core::config::ThemePreset::PaperBag => ThemePalette {
            bg: "#000000",
            surface: "#050608",
            raised: "#0A0E13",
            panel: "#070A0E",
            sidebar: "#020304",
            rail: "#000000",
            border: "#18232E",
            border_strong: "#2A4057",
            text: "#F5F8FC",
            muted: "#96A6B7",
            muted_2: "#64788C",
            accent: "#1687FF",
            soft: "#062B50",
            faint: "#041B32",
            mention: "#45A4FF",
            success: "#40D697",
            warning: "#F6BD4B",
            danger: "#FF637B",
            link: "#61ADFF",
            avatar: "#0E365B",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_dark_and_blue_led() {
        for preset in super_platinum_core::config::ThemePreset::ALL {
            let mut settings = super_platinum_core::config::Settings::default();
            settings.preset = preset;
            let css = theme_css(&settings, None);
            assert!(css.contains("color-scheme:dark"));
            assert!(css.contains("--link:#"));
            assert!(!css.contains("#E9E4DA"));
            assert!(!css.contains("#E8875B"));
        }
    }
}
