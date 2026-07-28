use super::*;

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

pub fn unreads_group_header(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev())),
        text_color: Some(text_1()),
        ..container::Style::default()
    }
}

pub fn unreads_date_pill(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_elev())),
        text_color: Some(text_2()),
        border: Border {
            color: border(),
            width: 1.0,
            radius: 999.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn unreads_date_line(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(border())),
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
