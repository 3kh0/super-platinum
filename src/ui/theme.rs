use std::sync::{LazyLock, RwLock};

use iced::theme::palette::Seed;
use iced::widget::{button, container, scrollable, slider, text_input, toggler as toggler_widget};
use iced::{Background, Border, Color, Element, Length, Shadow, Theme, Vector};

use crate::config::{ColorRole, HexColor, RoleColorOverrides, Settings, ThemePreset};

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
struct Vars {
    palette: Palette,
    accent: [Color; 5],
    gap: f32,
    panel_radius: f32,
    border_thickness: f32,
    background_active: bool,
    surface_opacity: f32,
    scrollbars_visible: bool,
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

fn vars() -> Vars {
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

fn surface(color: Color) -> Color {
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

pub fn root(_theme: &Theme) -> container::Style {
    container::Style {
        background: (!vars().background_active).then(|| Background::Color(bg_base())),
        text_color: Some(text_3()),
        ..container::Style::default()
    }
}

pub fn root_base(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_base())),
        ..container::Style::default()
    }
}

pub fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_panel())),
        text_color: Some(text_3()),
        border: Border {
            color: border(),
            width: border_thickness(),
            radius: panel_radius().into(),
        },
        ..container::Style::default()
    }
}

pub fn profile_card(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_panel())),
        text_color: Some(text_2()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.38,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..container::Style::default()
    }
}

pub fn profile_badge(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(text_1())),
        text_color: Some(bg_base()),
        border: Border::default().rounded(4.0),
        ..container::Style::default()
    }
}

pub fn sidebar(theme: &Theme) -> container::Style {
    panel(theme)
}

pub fn channel_row(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (bg, text_color) = match (active, status) {
            (true, button::Status::Pressed) => (
                Some(Background::Color(Color {
                    a: 0.28,
                    ..active_overlay()
                })),
                text_1(),
            ),
            (true, _) => (Some(Background::Color(active_overlay())), text_1()),
            (false, button::Status::Pressed) => (
                Some(Background::Color(Color { a: 0.16, ..hover() })),
                text_1(),
            ),
            (false, button::Status::Hovered) => (Some(Background::Color(hover())), text_2()),
            (false, _) => (None, text_3()),
        };
        button::Style {
            background: bg,
            text_color,
            border: Border::default().rounded(CONTROL_RADIUS),
            ..button::Style::default()
        }
    }
}

pub fn ping_badge(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ping())),
        text_color: Some(text_1()),
        border: Border::default().rounded(999.0),
        ..container::Style::default()
    }
}

pub fn app_badge(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.06,
            ..text_1()
        })),
        text_color: Some(Color::from_rgb(0.725, 0.729, 0.741)),
        border: Border::default().rounded(3.0),
        ..container::Style::default()
    }
}

pub fn vip_badge(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.92,
            ..text_1()
        })),
        text_color: Some(bg_base()),
        border: Border::default().rounded(4.0),
        ..container::Style::default()
    }
}

pub fn sidebar_icon(
    color: Color,
) -> impl Fn(&Theme, iced::widget::svg::Status) -> iced::widget::svg::Style {
    move |_theme, _status| iced::widget::svg::Style { color: Some(color) }
}

/// Floating Slack-style action bar overlaid on a hovered message.
pub fn action_bar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev())),
        text_color: Some(text_2()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.28,
                ..bg_base()
            },
            offset: Vector::new(0.0, 1.0),
            blur_radius: 6.0,
        },
        ..container::Style::default()
    }
}

/// Compact icon/text button used inside the [`action_bar`].
pub fn action_button(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: hovered.then_some(Background::Color(hover())),
        text_color: if hovered { text_1() } else { text_3() },
        border: Border::default().rounded(CONTROL_RADIUS - 3.0),
        ..button::Style::default()
    }
}

pub fn image_viewer_panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.965,
            ..bg_base()
        })),
        text_color: Some(text_1()),
        border: Border {
            color: Color {
                a: 0.55,
                ..border()
            },
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.55,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 8.0),
            blur_radius: 28.0,
        },
        ..container::Style::default()
    }
}

pub fn image_viewer_controls(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.88,
            ..bg_elev()
        })),
        text_color: Some(text_1()),
        border: Border {
            color: Color {
                a: 0.75,
                ..border()
            },
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.40,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 2.0),
            blur_radius: 12.0,
        },
        ..container::Style::default()
    }
}

pub fn image_viewer_button(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    let pressed = matches!(status, button::Status::Pressed);
    button::Style {
        background: (hovered || pressed).then_some(Background::Color(if pressed {
            bg_elev_hi()
        } else {
            hover()
        })),
        text_color: if hovered { text_1() } else { text_2() },
        border: Border::default().rounded(8.0),
        ..button::Style::default()
    }
}

pub fn reaction_chip(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev())),
        text_color: Some(text_2()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..container::Style::default()
    }
}

pub fn chat_paused_pill(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: Some(Background::Color(if hovered {
            bg_elev_hi()
        } else {
            bg_elev()
        })),
        text_color: text_1(),
        border: Border {
            color: border(),
            width: 1.0,
            radius: 999.0.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.35,
                ..bg_base()
            },
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..button::Style::default()
    }
}

pub const UNREAD_DIVIDER_THICKNESS: f32 = 4.0;

pub fn unread_divider_line(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(accent())),
        border: Border::default().rounded(UNREAD_DIVIDER_THICKNESS),
        ..container::Style::default()
    }
}

pub fn unread_divider_pill(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(accent())),
        text_color: Some(bg_base()),
        border: Border::default().rounded(999.0),
        ..container::Style::default()
    }
}

pub fn date_separator_label(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_base())),
        text_color: Some(text_2()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn inline_mention(broadcast: bool) -> impl Fn(&Theme) -> container::Style {
    move |_theme| {
        let (background, text_color) = if broadcast {
            (broadcast_bg(), broadcast_fg())
        } else {
            (mention_bg(), mention_fg())
        };
        container::Style {
            background: Some(Background::Color(background)),
            text_color: Some(text_color),
            border: Border::default().rounded(4.0),
            ..container::Style::default()
        }
    }
}

pub fn inline_mention_button(broadcast: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (background, text_color) = if broadcast {
            (broadcast_bg(), broadcast_fg())
        } else {
            (mention_bg(), mention_fg())
        };
        button::Style {
            background: Some(Background::Color(
                if matches!(status, button::Status::Hovered) {
                    Color {
                        a: 0.78,
                        ..background
                    }
                } else {
                    background
                },
            )),
            text_color,
            border: Border::default().rounded(4.0),
            ..button::Style::default()
        }
    }
}

pub fn reaction_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered);
        let (background, border, text_color) = if active {
            (
                Color {
                    a: 0.20,
                    ..accent()
                },
                accent(),
                accent_bright(),
            )
        } else if hovered {
            (bg_elev_hi(), border(), text_1())
        } else {
            (bg_elev(), border(), text_2())
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color,
            border: Border {
                color: border,
                width: 1.0,
                radius: CONTROL_RADIUS.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn toggler(_theme: &Theme, status: toggler_widget::Status) -> toggler_widget::Style {
    let toggled = matches!(
        status,
        toggler_widget::Status::Active { is_toggled: true }
            | toggler_widget::Status::Hovered { is_toggled: true }
    );
    toggler_widget::Style {
        background: Background::Color(if toggled { accent() } else { bg_elev_hi() }),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: Background::Color(text_1()),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(text_2()),
        border_radius: None,
        padding_ratio: 0.15,
    }
}

pub fn file_attachment(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev())),
        text_color: Some(text_2()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..container::Style::default()
    }
}

pub fn link_button(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: None,
        text_color: if hovered { accent_bright() } else { accent() },
        ..button::Style::default()
    }
}

pub fn panel_close_button(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: matches!(status, button::Status::Hovered)
            .then_some(Background::Color(bg_elev())),
        border: Border::default().rounded(CONTROL_RADIUS),
        ..button::Style::default()
    }
}

pub fn secondary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: Some(Background::Color(if hovered {
            bg_elev_hi()
        } else {
            bg_elev()
        })),
        text_color: text_2(),
        border: Border {
            color: border(),
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..button::Style::default()
    }
}

pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: Some(Background::Color(if hovered {
            accent_4()
        } else {
            accent_5()
        })),
        text_color: on_accent(),
        border: Border::default().rounded(CONTROL_RADIUS),
        ..button::Style::default()
    }
}

pub fn input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused { .. } => accent(),
        text_input::Status::Hovered => Color {
            a: 0.35,
            ..border()
        },
        _ => border(),
    };
    text_input::Style {
        background: Background::Color(bg_elev()),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        icon: text_4(),
        placeholder: text_4(),
        value: text_1(),
        selection: Color {
            a: 0.30,
            ..accent()
        },
    }
}

pub fn editor(
    _theme: &Theme,
    status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    use iced::widget::text_editor;
    let border_color = match status {
        text_editor::Status::Focused { .. } => accent(),
        text_editor::Status::Hovered => Color {
            a: 0.35,
            ..border()
        },
        _ => border(),
    };
    text_editor::Style {
        background: Background::Color(bg_elev()),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        placeholder: text_4(),
        value: text_1(),
        selection: Color {
            a: 0.30,
            ..accent()
        },
    }
}

pub fn composer_editor(
    _theme: &Theme,
    _status: iced::widget::text_editor::Status,
) -> iced::widget::text_editor::Style {
    iced::widget::text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: text_4(),
        value: text_1(),
        selection: Color {
            a: 0.30,
            ..accent()
        },
    }
}

pub fn presence_online(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(online())),
        border: Border {
            color: bg_panel(),
            width: 1.5,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn presence_offline(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_panel())),
        border: Border {
            color: text_4(),
            width: 1.5,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn avatar_placeholder(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev_hi())),
        text_color: Some(accent_bright()),
        border: Border::default().rounded(SIDEBAR_AVATAR_RADIUS),
        ..container::Style::default()
    }
}

pub fn account_avatar_placeholder(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(accent_5())),
        text_color: Some(text_1()),
        border: Border::default().rounded(CONTROL_RADIUS),
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_distinct_surfaces_and_modes() {
        let countertop = resolve_palette(ThemePreset::Countertop, RoleColorOverrides::default());
        let blue = resolve_palette(ThemePreset::BlueSteel, RoleColorOverrides::default());
        let paper = resolve_palette(ThemePreset::PaperBag, RoleColorOverrides::default());
        assert_ne!(countertop.base, blue.base);
        assert!(countertop.base.r < 0.2);
        assert!(paper.base.r > 0.8);
    }

    #[test]
    fn semantic_overrides_replace_only_the_requested_role() {
        let baseline = resolve_palette(ThemePreset::Countertop, RoleColorOverrides::default());
        let custom = HexColor::from_rgb(0x12, 0x34, 0x56);
        let palette = resolve_palette(
            ThemePreset::Countertop,
            RoleColorOverrides {
                danger: Some(custom),
                ..RoleColorOverrides::default()
            },
        );
        assert_eq!(palette.danger, color_from_hex(custom));
        assert_eq!(palette.primary, baseline.primary);
    }

    #[test]
    fn contrasting_text_changes_for_light_and_dark_colors() {
        assert_eq!(
            contrasting_text(Color::WHITE),
            Color::from_rgb8(0x18, 0x16, 0x14)
        );
        assert_eq!(
            contrasting_text(Color::BLACK),
            Color::from_rgb8(0xFA, 0xF7, 0xF2)
        );
    }

    #[test]
    fn scrollbar_interaction_is_axis_specific() {
        assert_eq!(
            scrollbar_state(
                true,
                scrollable::Status::Active {
                    is_horizontal_scrollbar_disabled: false,
                    is_vertical_scrollbar_disabled: false,
                },
            ),
            ((false, false), (false, false))
        );
        assert_eq!(
            scrollbar_state(
                true,
                scrollable::Status::Hovered {
                    is_horizontal_scrollbar_hovered: false,
                    is_vertical_scrollbar_hovered: false,
                    is_horizontal_scrollbar_disabled: false,
                    is_vertical_scrollbar_disabled: false,
                },
            ),
            ((true, false), (true, false))
        );
        assert_eq!(
            scrollbar_state(
                false,
                scrollable::Status::Hovered {
                    is_horizontal_scrollbar_hovered: false,
                    is_vertical_scrollbar_hovered: true,
                    is_horizontal_scrollbar_disabled: false,
                    is_vertical_scrollbar_disabled: false,
                },
            ),
            ((false, false), (true, true))
        );
        assert_eq!(
            scrollbar_state(
                false,
                scrollable::Status::Dragged {
                    is_horizontal_scrollbar_dragged: true,
                    is_vertical_scrollbar_dragged: false,
                    is_horizontal_scrollbar_disabled: false,
                    is_vertical_scrollbar_disabled: false,
                },
            ),
            ((true, true), (false, false))
        );
        assert_eq!(
            scrollbar_interaction(scrollable::Status::Active {
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            }),
            (false, false)
        );
        assert_eq!(
            scrollbar_interaction(scrollable::Status::Hovered {
                is_horizontal_scrollbar_hovered: false,
                is_vertical_scrollbar_hovered: true,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            }),
            (false, true)
        );
        assert_eq!(
            scrollbar_interaction(scrollable::Status::Dragged {
                is_horizontal_scrollbar_dragged: true,
                is_vertical_scrollbar_dragged: false,
                is_horizontal_scrollbar_disabled: false,
                is_vertical_scrollbar_disabled: false,
            }),
            (true, false)
        );
    }
}

pub fn rail_icon_placeholder(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev_hi())),
        text_color: Some(text_4()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        ..container::Style::default()
    }
}

pub fn rail_button(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: hovered.then_some(Background::Color(hover())),
        text_color: if hovered { accent_bright() } else { text_3() },
        border: Border::default().rounded(CONTROL_RADIUS),
        ..button::Style::default()
    }
}

pub fn activity_count_pill(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.92,
            ..text_1()
        })),
        text_color: Some(bg_base()),
        border: Border::default().rounded(999.0),
        ..container::Style::default()
    }
}

pub fn activity_count_badge(_theme: &Theme) -> container::Style {
    activity_count_pill(_theme)
}

pub fn activity_channel_chip(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.06,
            ..text_1()
        })),
        text_color: Some(text_2()),
        border: Border::default().rounded(4.0),
        ..container::Style::default()
    }
}

pub fn tooltip_bubble(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev_hi())),
        text_color: Some(text_1()),
        border: Border::default().rounded(CONTROL_RADIUS - 2.0),
        ..container::Style::default()
    }
}

pub fn activity_unread_bar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(accent_bright())),
        border: Border::default().rounded(2.0),
        ..container::Style::default()
    }
}

pub fn activity_read_bar(_theme: &Theme) -> container::Style {
    container::Style::default()
}

pub fn activity_row(
    active: bool,
    unread: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = if active {
            Some(Background::Color(active_overlay()))
        } else if hovered {
            Some(Background::Color(hover()))
        } else if unread {
            Some(Background::Color(Color {
                a: 0.05,
                ..text_1()
            }))
        } else {
            None
        };
        button::Style {
            background,
            text_color: text_1(),
            border: Border::default().rounded(CONTROL_RADIUS),
            ..button::Style::default()
        }
    }
}

pub fn rail_nav_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered);
        let background = if selected {
            Some(Background::Color(Color {
                a: 0.20,
                ..active_overlay()
            }))
        } else if hovered {
            Some(Background::Color(hover()))
        } else {
            None
        };
        button::Style {
            background,
            text_color: if selected || hovered {
                text_1()
            } else {
                text_4()
            },
            border: Border::default().rounded(CONTROL_RADIUS),
            ..button::Style::default()
        }
    }
}

pub fn rail_nav_icon(selected: bool) -> Color {
    if selected { text_1() } else { text_4() }
}

pub fn account_menu(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev())),
        text_color: Some(text_2()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: CONTROL_RADIUS.into(),
        },
        shadow: Shadow {
            color: Color {
                a: 0.30,
                ..bg_base()
            },
            offset: Vector::new(0.0, 2.0),
            blur_radius: 10.0,
        },
        ..container::Style::default()
    }
}

pub fn account_menu_button(_theme: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: hovered.then_some(Background::Color(hover())),
        text_color: if hovered { text_1() } else { text_2() },
        border: Border::default().rounded(CONTROL_RADIUS - 2.0),
        ..button::Style::default()
    }
}

pub fn overlay_dim(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.55,
        })),
        ..container::Style::default()
    }
}

pub fn swatch(color: Color, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered);
        let border_color = if selected {
            text_1()
        } else if hovered {
            Color { a: 0.6, ..text_2() }
        } else {
            Color { a: 0.0, ..color }
        };
        button::Style {
            background: Some(Background::Color(color)),
            border: Border {
                color: border_color,
                width: if selected { 2.5 } else { 1.5 },
                radius: CONTROL_RADIUS.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn preset_card(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let hovered = matches!(status, button::Status::Hovered);
        button::Style {
            background: Some(Background::Color(if selected {
                active_overlay()
            } else if hovered {
                hover()
            } else {
                bg_elev()
            })),
            text_color: if selected || hovered {
                text_1()
            } else {
                text_2()
            },
            border: Border {
                color: if selected { accent() } else { border() },
                width: if selected { 1.5 } else { 1.0 },
                radius: CONTROL_RADIUS.into(),
            },
            ..button::Style::default()
        }
    }
}

pub fn divider<'a, Message: 'a>() -> Element<'a, Message> {
    divider_faded(1.0)
}

pub fn divider_faded<'a, Message: 'a>(alpha: f32) -> Element<'a, Message> {
    container(iced::widget::Space::new().width(Length::Fill).height(1.0))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(border().scale_alpha(alpha))),
            ..container::Style::default()
        })
        .into()
}

pub fn fade(color: Color, alpha: f32) -> Color {
    color.scale_alpha(alpha)
}

pub fn fade_container(
    style: impl Fn(&Theme) -> container::Style,
    alpha: f32,
) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let mut s = style(theme);
        s.background = s.background.map(|b| b.scale_alpha(alpha));
        s.text_color = s.text_color.map(|c| c.scale_alpha(alpha));
        s.border.color = s.border.color.scale_alpha(alpha);
        s.shadow.color = s.shadow.color.scale_alpha(alpha);
        s
    }
}

pub fn fade_button(
    style: impl Fn(&Theme, button::Status) -> button::Style,
    alpha: f32,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut s = style(theme, status);
        s.background = s.background.map(|b| b.scale_alpha(alpha));
        s.text_color = s.text_color.scale_alpha(alpha);
        s.border.color = s.border.color.scale_alpha(alpha);
        s.shadow.color = s.shadow.color.scale_alpha(alpha);
        s
    }
}

pub fn fade_input(
    style: impl Fn(&Theme, text_input::Status) -> text_input::Style,
    alpha: f32,
) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |theme, status| {
        let mut s = style(theme, status);
        s.background = s.background.scale_alpha(alpha);
        s.border.color = s.border.color.scale_alpha(alpha);
        s.icon = s.icon.scale_alpha(alpha);
        s.placeholder = s.placeholder.scale_alpha(alpha);
        s.value = s.value.scale_alpha(alpha);
        s.selection = s.selection.scale_alpha(alpha);
        s
    }
}

pub fn fade_slider(alpha: f32) -> impl Fn(&Theme, slider::Status) -> slider::Style {
    move |theme, status| {
        let mut s = slider::default(theme, status);
        s.rail.backgrounds.0 = s.rail.backgrounds.0.scale_alpha(alpha);
        s.rail.backgrounds.1 = s.rail.backgrounds.1.scale_alpha(alpha);
        s.rail.border.color = s.rail.border.color.scale_alpha(alpha);
        s.handle.background = s.handle.background.scale_alpha(alpha);
        s.handle.border_color = s.handle.border_color.scale_alpha(alpha);
        s
    }
}

pub fn fade_scrollbar(alpha: f32) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    move |theme, status| {
        let mut s = scrollbar(theme, status);
        s.vertical_rail.scroller.background =
            s.vertical_rail.scroller.background.scale_alpha(alpha);
        s.horizontal_rail.scroller.background =
            s.horizontal_rail.scroller.background.scale_alpha(alpha);
        s
    }
}

pub fn scrollbar(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let ((horizontal_visible, horizontal_interacting), (vertical_visible, vertical_interacting)) =
        scrollbar_state(vars().scrollbars_visible, status);
    let rail = |visible: bool, interacting: bool| scrollable::Rail {
        background: None,
        border: Border::default().rounded(CONTROL_RADIUS),
        scroller: scrollable::Scroller {
            background: Background::Color(if visible {
                if interacting {
                    Color {
                        a: 0.45,
                        ..active_overlay()
                    }
                } else {
                    active_overlay()
                }
            } else {
                Color::TRANSPARENT
            }),
            border: Border::default().rounded(CONTROL_RADIUS),
        },
    };
    scrollable::Style {
        vertical_rail: rail(vertical_visible, vertical_interacting),
        horizontal_rail: rail(horizontal_visible, horizontal_interacting),
        gap: None,
        ..scrollable::default(theme, status)
    }
}

fn scrollbar_state(
    activity_enabled: bool,
    status: scrollable::Status,
) -> ((bool, bool), (bool, bool)) {
    let activity_visible = activity_enabled
        && matches!(
            status,
            scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. }
        );
    let (horizontal_interacting, vertical_interacting) = scrollbar_interaction(status);
    (
        (
            activity_visible || horizontal_interacting,
            horizontal_interacting,
        ),
        (
            activity_visible || vertical_interacting,
            vertical_interacting,
        ),
    )
}

fn scrollbar_interaction(status: scrollable::Status) -> (bool, bool) {
    match status {
        scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
            ..
        } => (
            is_horizontal_scrollbar_hovered,
            is_vertical_scrollbar_hovered,
        ),
        scrollable::Status::Dragged {
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
            ..
        } => (
            is_horizontal_scrollbar_dragged,
            is_vertical_scrollbar_dragged,
        ),
        scrollable::Status::Active { .. } => (false, false),
    }
}
