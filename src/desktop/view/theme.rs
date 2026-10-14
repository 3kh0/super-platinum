pub(crate) mod check;

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
    let on_accent = on_fill(&accent);
    let on_accent_hover = on_rgb(mix(rgb(&accent), [1.0; 3], 0.82));
    let on_mention = on_fill(&mention);
    let on_danger = on_fill(&danger);
    let on_danger_hover = on_rgb(mix(rgb(&danger), [1.0; 3], 0.85));
    let background = background_uri.map(|uri| {
        let (fit, dim, opacity) = settings.background.as_ref().map(|background| (
            match background.fit { super_platinum_core::config::BackgroundFit::Cover => "cover", super_platinum_core::config::BackgroundFit::Contain => "contain" },
            background.dim.clamp(0.0, 1.0),
            background.surface_opacity.clamp(0.0, 1.0),
        )).unwrap_or(("cover", 0.45, 0.88));
        // Wallpaper remains decorative beneath reading/editing surfaces. This
        // floor protects the smallest palette text, including hovered messages,
        // even over a white image at zero dim. Uncovered areas retain user dimming.
        let text_opacity = opacity.max(TEXT_SURFACE_OPACITY);
        format!(".conversation {{ background-image: linear-gradient(rgba(0,0,0,{dim}), rgba(0,0,0,{dim})), url('{uri}'); background-size:{fit}; background-position:center; background-repeat:no-repeat; }} .timeline {{ background-color: color-mix(in srgb, var(--bg) {}%, transparent); }} .conversation-header, .composer-wrap {{ background-color: color-mix(in srgb, var(--surface) {}%, transparent); }}", text_opacity * 100.0, text_opacity * 100.0)
    }).unwrap_or_default();
    format!(
        ":root {{ color-scheme:dark; --bg:{}; --surface:{}; --surface-raised:{}; --panel:{}; --sidebar:{}; --rail:{}; --border:{}; --border-strong:{}; --text:{}; --muted:{}; --muted-2:{}; --accent:{accent}; --accent-soft:{hover}; --accent-faint:{}; --mention:{mention}; --success:{success}; --warning:{warning}; --danger:{danger}; --link:{}; --avatar-bg:{}; --hover-overlay:color-mix(in srgb, var(--text) 6%, transparent); --on-accent:{on_accent}; --on-accent-hover:{on_accent_hover}; --on-mention:{on_mention}; --on-danger:{on_danger}; --on-danger-hover:{on_danger_hover}; --on-warning:#07111D; --sidebar-width:{}px; --density-gap:{}px; --panel-radius:{}px; --border-thickness:{}px; }}",
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

const TEXT_SURFACE_OPACITY: f32 = 0.96;

// Theme inputs are canonical six-digit RGB from HexColor::as_hex or these presets.
fn rgb(hex: &str) -> [f64; 3] {
    [1, 3, 5].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("theme RGB") as f64 / 255.0)
}
fn mix(foreground: [f64; 3], background: [f64; 3], opacity: f64) -> [f64; 3] {
    std::array::from_fn(|i| foreground[i] * opacity + background[i] * (1.0 - opacity))
}
fn luminance(rgb: [f64; 3]) -> f64 {
    rgb.into_iter()
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(c, weight)| {
            weight
                * if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
        })
        .sum()
}
fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn on_rgb(fill: [f64; 3]) -> &'static str {
    if contrast([0.0; 3], fill) >= contrast([1.0; 3], fill) {
        "#000000"
    } else {
        "#FFFFFF"
    }
}
fn on_fill(fill: &str) -> &'static str {
    on_rgb(rgb(fill))
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
            bg: "#0A0A0A",
            surface: "#111111",
            raised: "#1A1A1A",
            panel: "#141414",
            sidebar: "#111111",
            rail: "#0A0A0A",
            border: "#292929",
            border_strong: "#454545",
            text: "#EDEDED",
            muted: "#A1A1A1",
            muted_2: "#929292",
            accent: "#0070F3",
            soft: "#173457",
            faint: "#101D2B",
            mention: "#52A8FF",
            success: "#46A758",
            warning: "#FFB224",
            danger: "#FF6369",
            link: "#52A8FF",
            avatar: "#262626",
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
            muted_2: "#8195A8",
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
            muted_2: "#8192A5",
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

    #[test]
    fn luminance_matches_reference_black_white_and_srgb_breakpoint() {
        assert_eq!(contrast([0.0; 3], [1.0; 3]), 21.0);
        assert_eq!(contrast(rgb("#777777"), rgb("#777777")), 1.0);
        assert!((luminance(rgb("#777777")) - 0.1844749945).abs() < 1e-9);
        assert!((luminance([0.04045; 3]) - 0.00313080495).abs() < 1e-10);
    }

    fn token(css: &str, key: &str) -> [f64; 3] {
        rgb(css
            .split_once(&format!("--{key}:"))
            .unwrap()
            .1
            .split(';')
            .next()
            .unwrap())
    }
    fn assert_fill_contract(css: &str) {
        for key in ["accent", "mention", "danger"] {
            assert!(
                contrast(token(css, key), token(css, &format!("on-{key}"))) >= 4.5,
                "{key}"
            );
        }
        for (key, opacity) in [("accent", 0.82), ("danger", 0.85)] {
            assert!(
                contrast(
                    mix(token(css, key), [1.0; 3], opacity),
                    token(css, &format!("on-{key}-hover"))
                ) >= 4.5,
                "{key} hover"
            );
        }
    }
    #[test]
    fn emitted_fill_foregrounds_cover_presets_and_arbitrary_colors() {
        use super_platinum_core::config::{HexColor, Settings, ThemePreset};
        for preset in ThemePreset::ALL {
            let mut settings = Settings::default();
            settings.preset = preset;
            assert_fill_contract(&theme_css(&settings, None));
        }
        // Include both sides of the black/white crossover and saturated fills;
        // checking emitted tokens also exercises custom-role override wiring.
        for bytes in [
            [0, 0, 0],
            [255, 255, 255],
            [112, 112, 112],
            [116, 116, 116],
            [117, 117, 117],
            [118, 118, 118],
            [119, 119, 119],
            [0, 0, 255],
            [255, 0, 0],
            [0, 255, 0],
            [255, 255, 0],
            [0, 255, 255],
            [255, 0, 255],
            [25, 86, 160],
        ] {
            let mut settings = Settings::default();
            let fill = HexColor::from_rgb(bytes[0], bytes[1], bytes[2]);
            settings.colors.primary = Some(fill);
            settings.colors.mention = Some(fill);
            settings.colors.danger = Some(fill);
            let css = theme_css(&settings, None);
            assert_fill_contract(&css);
            assert_eq!(token(&css, "accent"), rgb(&fill.as_hex()));
        }
    }
    #[test]
    fn small_text_pairs_cover_selection_placeholder_shortcut_and_link_actions() {
        for preset in super_platinum_core::config::ThemePreset::ALL {
            let p = preset_palette(preset);
            let muted = rgb(p.muted_2);
            // Actual final CSS: selected DM timestamp, list-find placeholder,
            // shortcut hint (no additional opacity), storage ticks and text links.
            for background in [
                mix(rgb(p.text), rgb(p.sidebar), 0.09),
                mix(rgb(p.surface), rgb(p.raised), 0.76),
                mix(rgb(p.bg), rgb(p.raised), 0.68),
                rgb(p.surface),
                rgb(p.raised),
            ] {
                assert!(
                    contrast(muted, background) >= 4.5,
                    "{preset:?}: {background:?}"
                );
            }
            for background in [p.soft, p.raised, p.surface, p.bg] {
                assert!(
                    contrast(rgb(p.link), rgb(background)) >= 4.5,
                    "{preset:?}: link/{background}"
                );
            }
        }
    }
    #[test]
    fn wallpaper_extremes_preserve_small_text_on_all_three_text_surfaces() {
        use super_platinum_core::config::{
            BackgroundFit, BackgroundSettings, Settings, ThemePreset,
        };
        for preset in ThemePreset::ALL {
            let p = preset_palette(preset);
            for requested in [0.0, 0.5, 0.96, 1.0] {
                let mut settings = Settings::default();
                settings.preset = preset;
                settings.background = Some(BackgroundSettings {
                    file_name: "wallpaper.png".into(),
                    fit: BackgroundFit::Cover,
                    dim: 0.0,
                    surface_opacity: requested,
                });
                let css = theme_css(&settings, Some("super-platinum-media://background"));
                // Read the actual emitted percentage for timeline and the shared
                // header/composer rule instead of asserting a fixed CSS string.
                for (selector, fill, variable) in [
                    (".timeline { background-color:", p.bg, "bg"),
                    (
                        ".conversation-header, .composer-wrap { background-color:",
                        p.surface,
                        "surface",
                    ),
                ] {
                    let rule = css
                        .split_once(selector)
                        .unwrap()
                        .1
                        .split('}')
                        .next()
                        .unwrap();
                    let alpha: f64 = rule
                        .split_once(&format!("var(--{variable}) "))
                        .unwrap()
                        .1
                        .split('%')
                        .next()
                        .unwrap()
                        .parse::<f64>()
                        .unwrap()
                        / 100.0;
                    assert!(alpha >= f64::from(requested) && alpha <= 1.0);
                    // White is worst for these light foregrounds; black and
                    // saturated primaries cover the other compositing extremes.
                    for wallpaper in [
                        [0.0; 3],
                        [1.0; 3],
                        [1.0, 0.0, 0.0],
                        [0.0, 1.0, 0.0],
                        [0.0, 0.0, 1.0],
                    ] {
                        for hover in [0.0, 0.06] {
                            let background =
                                mix(rgb(p.text), mix(rgb(fill), wallpaper, alpha), hover);
                            for foreground in [p.text, p.muted, p.muted_2, p.link] {
                                assert!(
                                    contrast(rgb(foreground), background) >= 4.5,
                                    "{preset:?}: {foreground}/{background:?}"
                                );
                            }
                            assert!(
                                contrast(mix(rgb(p.muted), background, 0.78), background) >= 3.0,
                                "{preset:?}: composer icon"
                            );
                        }
                    }
                }
            }
        }
    }
}
