use super::*;

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

pub(super) fn scrollbar_state(
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

pub(super) fn scrollbar_interaction(status: scrollable::Status) -> (bool, bool) {
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
