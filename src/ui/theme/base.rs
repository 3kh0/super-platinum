use super::*;

pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SCROLLBAR_GUTTER: f32 = 12.0;

pub const TEXT_SM: f32 = 12.0;
pub const TEXT_MD: f32 = 14.0;
pub const TEXT_LG: f32 = 15.0;

pub const SIDEBAR_WIDTH: f32 = 240.0;
pub const THREAD_WIDTH: f32 = 340.0;
pub const PANEL_HEADER_HEIGHT: f32 = 42.0;
pub const PANEL_CLOSE_SIZE: f32 = 24.0;

pub const CONTROL_RADIUS: f32 = 6.0;

pub const MSG_AVATAR: f32 = 28.0;
pub const MSG_AVATAR_RADIUS: f32 = 6.0;

pub const SIDEBAR_ICON: f32 = 15.0; // svg glyph size
pub const SIDEBAR_ICON_SLOT: f32 = 22.0; // fixed leading column so labels align
pub const SIDEBAR_AVATAR: f32 = 18.0; // dm avatar / group-count chip
pub const SIDEBAR_AVATAR_RADIUS: f32 = 4.0;
pub const PRESENCE_DOT: f32 = 7.0; // online/offline indicator on avatars
pub const PING_BADGE_H: f32 = 16.0; // normalized ping badge height

#[derive(Clone, Copy)]
pub(super) struct Vars {
    pub(super) palette: Palette,
    pub(super) accent: [Color; 5],
    pub(super) gap: f32,
    pub(super) panel_radius: f32,
    pub(super) border_thickness: f32,
    pub(super) background_active: bool,
    pub(super) surface_opacity: f32,
    pub(super) scrollbars_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub base: Color,
    pub panel: Color,
    pub elevated: Color,
    pub elevated_high: Color,
    pub text: [Color; 5],
    pub border: Color,
    pub hover_overlay: Color,
    pub active_overlay: Color,
    pub primary: Color,
    pub primary_hover: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub mention: Color,
    pub broadcast: Color,
}

impl Default for Vars {
    fn default() -> Self {
        let palette = resolve_palette(ThemePreset::default(), RoleColorOverrides::default());
        Vars {
            accent: accent_ramp(palette.primary, palette.primary_hover),
            palette,
            gap: 8.0,
            panel_radius: 8.0,
            border_thickness: 1.0,
            background_active: false,
            surface_opacity: 1.0,
            scrollbars_visible: false,
        }
    }
}

static VARS: LazyLock<RwLock<Vars>> = LazyLock::new(|| RwLock::new(Vars::default()));

pub(super) fn vars() -> Vars {
    *VARS.read().expect("theme vars poisoned")
}

pub fn apply(settings: &Settings) {
    let palette = resolve_palette(settings.preset, settings.colors);
    let scrollbars_visible = vars().scrollbars_visible;
    *VARS.write().expect("theme vars poisoned") = Vars {
        accent: accent_ramp(palette.primary, palette.primary_hover),
        palette,
        gap: settings.gap,
        panel_radius: settings.panel_radius,
        border_thickness: settings.border_thickness,
        background_active: settings.background.is_some(),
        surface_opacity: settings.background.as_ref().map_or(1.0, |background| {
            background.surface_opacity.clamp(0.65, 1.0)
        }),
        scrollbars_visible,
    };
}

pub fn set_scrollbars_visible(visible: bool) {
    VARS.write()
        .expect("theme vars poisoned")
        .scrollbars_visible = visible;
}

pub fn accent() -> Color {
    vars().accent[1]
}
pub fn accent_bright() -> Color {
    vars().accent[0]
}
/// Translucent accent used to paint the text-selection highlight.
pub fn selection() -> Color {
    Color {
        a: 0.30,
        ..vars().accent[1]
    }
}
pub fn accent_3() -> Color {
    vars().accent[2]
}
pub fn accent_4() -> Color {
    vars().accent[3]
}
pub fn accent_5() -> Color {
    vars().accent[4]
}
pub fn gap() -> f32 {
    vars().gap
}
pub fn panel_radius() -> f32 {
    vars().panel_radius
}
pub fn border_thickness() -> f32 {
    vars().border_thickness
}

pub fn resolve_palette(preset: ThemePreset, overrides: RoleColorOverrides) -> Palette {
    let mut palette = match preset {
        ThemePreset::Countertop => Palette {
            base: hex(0x171615),
            panel: hex(0x24211F),
            elevated: hex(0x302D29),
            elevated_high: hex(0x3D3934),
            text: [
                hex(0xF4EFE6),
                hex(0xC8BEB0),
                hex(0xA69C90),
                hex(0x7E756C),
                hex(0x625B54),
            ],
            border: hex(0x4A443D),
            hover_overlay: rgba(0xD8C7B4, 0.10),
            active_overlay: rgba(0xE8A17D, 0.20),
            primary: hex(0xE8875B),
            primary_hover: hex(0xF0A07B),
            success: hex(0x62B184),
            warning: hex(0xD9A441),
            danger: hex(0xD86B62),
            mention: hex(0xEE9B6E),
            broadcast: hex(0xE5B84F),
        },
        ThemePreset::BlueSteel => Palette {
            base: hex(0x15191D),
            panel: hex(0x20262C),
            elevated: hex(0x2B333B),
            elevated_high: hex(0x37424C),
            text: [
                hex(0xEDF2F5),
                hex(0xB7C1C9),
                hex(0x96A3AD),
                hex(0x6F7D88),
                hex(0x53606A),
            ],
            border: hex(0x44515C),
            hover_overlay: rgba(0xAFC4D1, 0.10),
            active_overlay: rgba(0x72A9C4, 0.22),
            primary: hex(0x72A9C4),
            primary_hover: hex(0x91BED3),
            success: hex(0x62B38B),
            warning: hex(0xD5A64B),
            danger: hex(0xD66D72),
            mention: hex(0x7DB7D3),
            broadcast: hex(0xD8B45A),
        },
        ThemePreset::PaperBag => Palette {
            base: hex(0xE9E4DA),
            panel: hex(0xF7F3EC),
            elevated: hex(0xDED7CC),
            elevated_high: hex(0xD2C9BC),
            text: [
                hex(0x292622),
                hex(0x514B44),
                hex(0x665F57),
                hex(0x81786E),
                hex(0x9B9185),
            ],
            border: hex(0xC8BEB0),
            hover_overlay: rgba(0x5B4D42, 0.08),
            active_overlay: rgba(0xB84F3D, 0.16),
            primary: hex(0xB84F3D),
            primary_hover: hex(0x943E31),
            success: hex(0x347A55),
            warning: hex(0x9A6B13),
            danger: hex(0xA43E3E),
            mention: hex(0xA84B3B),
            broadcast: hex(0x8E681A),
        },
    };

    palette.primary = override_color(overrides.primary, palette.primary);
    palette.primary_hover = override_color(overrides.hover, palette.primary_hover);
    palette.mention = override_color(overrides.mention, palette.mention);
    palette.success = override_color(overrides.success, palette.success);
    palette.warning = override_color(overrides.warning, palette.warning);
    palette.danger = override_color(overrides.danger, palette.danger);
    palette
}

fn override_color(value: Option<HexColor>, fallback: Color) -> Color {
    value.map_or(fallback, color_from_hex)
}

pub fn color_from_hex(value: HexColor) -> Color {
    let [r, g, b] = value.rgb();
    Color::from_rgb8(r, g, b)
}

fn hex(value: u32) -> Color {
    Color::from_rgb8(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}

fn rgba(value: u32, alpha: f32) -> Color {
    Color {
        a: alpha,
        ..hex(value)
    }
}

fn accent_ramp(primary: Color, hover: Color) -> [Color; 5] {
    [
        hover,
        primary,
        mix(primary, Color::BLACK, 0.12),
        mix(primary, Color::BLACK, 0.24),
        mix(primary, Color::BLACK, 0.36),
    ]
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color {
        r: from.r + (to.r - from.r) * amount,
        g: from.g + (to.g - from.g) * amount,
        b: from.b + (to.b - from.b) * amount,
        a: from.a + (to.a - from.a) * amount,
    }
}

pub fn snack_theme() -> Theme {
    let palette = vars().palette;
    Theme::custom(
        "Snack".to_owned(),
        Seed {
            background: palette.base,
            text: palette.text[0],
            primary: accent(),
            success: palette.success,
            warning: palette.warning,
            danger: palette.danger,
        },
    )
}

pub fn bg_base() -> Color {
    vars().palette.base
}
pub fn bg_panel() -> Color {
    surface(vars().palette.panel)
}
pub fn bg_elev() -> Color {
    surface(vars().palette.elevated)
}
pub fn bg_elev_hi() -> Color {
    surface(vars().palette.elevated_high)
}
pub fn text_1() -> Color {
    vars().palette.text[0]
}
pub fn text_2() -> Color {
    vars().palette.text[1]
}
pub fn text_3() -> Color {
    vars().palette.text[2]
}
pub fn text_4() -> Color {
    vars().palette.text[3]
}
pub fn text_5() -> Color {
    vars().palette.text[4]
}
pub fn online() -> Color {
    vars().palette.success
}
pub fn ping() -> Color {
    vars().palette.danger
}
pub fn mention_fg() -> Color {
    vars().palette.mention
}
pub fn mention_bg() -> Color {
    Color {
        a: 0.24,
        ..mention_fg()
    }
}
pub fn broadcast_fg() -> Color {
    vars().palette.broadcast
}
pub fn broadcast_bg() -> Color {
    Color {
        a: 0.24,
        ..broadcast_fg()
    }
}
pub fn muted() -> Color {
    text_5()
}
pub fn sidebar_fg() -> Color {
    text_3()
}
pub fn hover() -> Color {
    vars().palette.hover_overlay
}
pub fn active_overlay() -> Color {
    vars().palette.active_overlay
}
pub fn border() -> Color {
    vars().palette.border
}
pub fn role_color(role: ColorRole) -> Color {
    let palette = vars().palette;
    match role {
        ColorRole::Primary => palette.primary,
        ColorRole::Hover => palette.primary_hover,
        ColorRole::Mention => palette.mention,
        ColorRole::Success => palette.success,
        ColorRole::Warning => palette.warning,
        ColorRole::Danger => palette.danger,
    }
}
pub fn on_accent() -> Color {
    contrasting_text(accent_5())
}

pub fn contrasting_text(background: Color) -> Color {
    let luminance = |channel: f32| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    let relative = 0.2126 * luminance(background.r)
        + 0.7152 * luminance(background.g)
        + 0.0722 * luminance(background.b);
    // Black and white have equal WCAG contrast at roughly 0.179 luminance.
    if relative > 0.179 {
        Color::from_rgb8(0x18, 0x16, 0x14)
    } else {
        Color::from_rgb8(0xFA, 0xF7, 0xF2)
    }
}

pub(super) fn surface(color: Color) -> Color {
    let current = vars();
    if current.background_active {
        Color {
            a: current.surface_opacity,
            ..color
        }
    } else {
        color
    }
}
