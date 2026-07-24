use std::collections::HashMap;

use iced::widget::{
    Space, button, column, container, row, scrollable, slider, stack, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length, Padding};

use super::{motion, theme};
use crate::app::Message;
use crate::config::{BackgroundFit, ColorRole, Settings, ThemePreset};

const CARD_WIDTH: f32 = 540.0;
const CARD_MAX_HEIGHT: f32 = 700.0;
const SWATCH_SIZE: f32 = 18.0;

pub fn modal<'a>(
    base: Element<'a, Message>,
    settings: &'a Settings,
    color_drafts: &'a HashMap<ColorRole, String>,
    color_errors: &'a HashMap<ColorRole, String>,
    open: bool,
) -> Element<'a, Message> {
    let layers = motion::overlay(open, move |anim, at| {
        let progress = motion::t(anim, at);
        let alpha = motion::fade(progress);
        let scrim = motion::scrim(progress, Message::SettingsClosed);
        let card = motion::zoom_y(
            card(settings, color_drafts, color_errors, alpha),
            progress,
            -8.0,
        );
        let centered = container(card)
            .center_x(Fill)
            .center_y(Fill)
            .padding(theme::SPACE_MD);

        Element::from(stack![scrim, centered].width(Fill).height(Fill))
    })
    .on_finish_maybe((!open).then_some(Message::SettingsDismissed));

    stack![base, layers].into()
}

fn card<'a>(
    settings: &'a Settings,
    color_drafts: &'a HashMap<ColorRole, String>,
    color_errors: &'a HashMap<ColorRole, String>,
    alpha: f32,
) -> Element<'a, Message> {
    let title = text("Appearance")
        .size(theme::TEXT_LG)
        .color(theme::fade(theme::text_1(), alpha))
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..iced::Font::default()
        });

    let body = column![
        section_label("Preset", alpha),
        preset_section(settings.preset, alpha),
        theme::divider_faded(alpha),
        section_label("Background", alpha),
        background_section(settings, alpha),
        theme::divider_faded(alpha),
        color_section(settings, color_drafts, color_errors, alpha),
        theme::divider_faded(alpha),
        section_label("Layout", alpha),
        layout_section(settings, alpha),
    ]
    .spacing(theme::SPACE_MD)
    .width(Fill);

    let content = column![
        title,
        theme::divider_faded(alpha),
        scrollable(body.padding(Padding::ZERO.right(theme::SCROLLBAR_GUTTER)))
            .style(theme::fade_scrollbar(alpha))
            .height(Fill),
        theme::divider_faded(alpha),
        actions(alpha),
    ]
    .spacing(theme::SPACE_MD)
    .width(Fill)
    .height(Fill);

    container(content)
        .width(Length::Fixed(CARD_WIDTH))
        .height(Length::Fixed(CARD_MAX_HEIGHT))
        .padding(theme::SPACE_MD)
        .style(theme::fade_container(theme::panel, alpha))
        .into()
}

fn section_label(label: &str, alpha: f32) -> Element<'_, Message> {
    text(label.to_owned())
        .size(theme::TEXT_MD)
        .color(theme::fade(theme::text_2(), alpha))
        .font(iced::Font {
            weight: iced::font::Weight::Semibold,
            ..iced::Font::default()
        })
        .into()
}

fn preset_section(selected: ThemePreset, alpha: f32) -> Element<'static, Message> {
    let mut cards = row![].spacing(theme::SPACE_SM).width(Fill);
    for preset in ThemePreset::ALL {
        let palette = theme::resolve_palette(preset, Default::default());
        let preview = row![
            color_dot(palette.base, alpha),
            color_dot(palette.panel, alpha),
            color_dot(palette.primary, alpha),
        ]
        .spacing(theme::SPACE_XS);
        let label = column![
            text(preset.label()).size(theme::TEXT_SM).font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..iced::Font::default()
            }),
            text(preset.description())
                .size(theme::TEXT_SM - 1.0)
                .color(theme::fade(theme::text_4(), alpha)),
            preview,
        ]
        .spacing(theme::SPACE_XS);
        cards = cards.push(
            button(label)
                .width(Fill)
                .padding(theme::SPACE_SM)
                .style(theme::fade_button(
                    theme::preset_card(preset == selected),
                    alpha,
                ))
                .on_press(Message::SettingsPresetSelected(preset)),
        );
    }
    cards.into()
}

fn background_section<'a>(settings: &'a Settings, alpha: f32) -> Element<'a, Message> {
    let Some(background) = settings.background.as_ref() else {
        return column![
            text("Use a local PNG, JPEG, or WebP image behind your workspace.")
                .size(theme::TEXT_SM)
                .color(theme::fade(theme::text_4(), alpha)),
            button(text("Choose image").size(theme::TEXT_SM))
                .padding([theme::SPACE_XS, theme::SPACE_SM])
                .style(theme::fade_button(theme::secondary_button, alpha))
                .on_press(Message::SettingsBackgroundPickerOpened),
        ]
        .spacing(theme::SPACE_SM)
        .into();
    };

    let fit = row![
        button(text("Cover").size(theme::TEXT_SM))
            .padding([theme::SPACE_XS, theme::SPACE_SM])
            .style(theme::fade_button(
                theme::preset_card(background.fit == BackgroundFit::Cover),
                alpha,
            ))
            .on_press(Message::SettingsBackgroundFitChanged(BackgroundFit::Cover)),
        button(text("Contain").size(theme::TEXT_SM))
            .padding([theme::SPACE_XS, theme::SPACE_SM])
            .style(theme::fade_button(
                theme::preset_card(background.fit == BackgroundFit::Contain),
                alpha,
            ))
            .on_press(Message::SettingsBackgroundFitChanged(
                BackgroundFit::Contain
            )),
    ]
    .spacing(theme::SPACE_SM);

    column![
        row![
            column![
                text("Custom image")
                    .size(theme::TEXT_SM)
                    .color(theme::fade(theme::text_2(), alpha)),
                text(background.file_name.clone())
                    .size(theme::TEXT_SM - 1.0)
                    .color(theme::fade(theme::text_4(), alpha)),
            ]
            .spacing(2.0),
            Space::new().width(Fill),
            button(text("Replace").size(theme::TEXT_SM))
                .padding([theme::SPACE_XS, theme::SPACE_SM])
                .style(theme::fade_button(theme::secondary_button, alpha))
                .on_press(Message::SettingsBackgroundPickerOpened),
            button(text("Remove").size(theme::TEXT_SM))
                .padding([theme::SPACE_XS, theme::SPACE_SM])
                .style(theme::fade_button(theme::secondary_button, alpha))
                .on_press(Message::SettingsBackgroundRemoved),
        ]
        .spacing(theme::SPACE_SM)
        .align_y(Alignment::Center),
        fit,
        slider_row(
            "Background dim",
            format!("{}%", (background.dim * 100.0).round() as i32),
            0.0..=0.90,
            background.dim,
            Message::SettingsBackgroundDimChanged,
            0.01,
            alpha,
        ),
        slider_row(
            "Panel opacity",
            format!("{}%", (background.surface_opacity * 100.0).round() as i32),
            0.65..=1.0,
            background.surface_opacity,
            Message::SettingsSurfaceOpacityChanged,
            0.01,
            alpha,
        ),
    ]
    .spacing(theme::SPACE_SM)
    .into()
}

fn color_section<'a>(
    settings: &'a Settings,
    drafts: &'a HashMap<ColorRole, String>,
    errors: &'a HashMap<ColorRole, String>,
    alpha: f32,
) -> Element<'a, Message> {
    let mut fields = column![
        row![
            section_label("Colors", alpha),
            Space::new().width(Fill),
            button(text("Use preset colors").size(theme::TEXT_SM))
                .padding([theme::SPACE_XS, theme::SPACE_SM])
                .style(theme::fade_button(theme::secondary_button, alpha))
                .on_press_maybe(
                    (!settings.colors.is_empty()).then_some(Message::SettingsPresetColorsRestored),
                ),
        ]
        .align_y(Alignment::Center),
        text("Leave a field empty to use the preset value.")
            .size(theme::TEXT_SM)
            .color(theme::fade(theme::text_4(), alpha)),
    ]
    .spacing(theme::SPACE_SM);

    for role in ColorRole::ALL {
        let value = drafts.get(&role).map_or("", String::as_str);
        let input = text_input("Preset", value)
            .on_input(move |value| Message::SettingsRoleColorChanged(role, value))
            .size(theme::TEXT_SM)
            .padding([theme::SPACE_XS, theme::SPACE_SM])
            .style(theme::fade_input(theme::input, alpha))
            .width(Length::Fixed(150.0));
        let field = row![
            color_dot(theme::role_color(role), alpha),
            text(role.label())
                .size(theme::TEXT_SM)
                .color(theme::fade(theme::text_2(), alpha))
                .width(Length::Fixed(80.0)),
            Space::new().width(Fill),
            input,
        ]
        .spacing(theme::SPACE_SM)
        .align_y(Alignment::Center);
        fields = fields.push(field);
        if let Some(error) = errors.get(&role) {
            fields = fields.push(
                text(error.clone())
                    .size(theme::TEXT_SM - 1.0)
                    .color(theme::fade(theme::role_color(ColorRole::Danger), alpha)),
            );
        }
    }
    fields.into()
}

fn layout_section<'a>(settings: &'a Settings, alpha: f32) -> Element<'a, Message> {
    column![
        slider_row(
            "Panel gap",
            format!("{} px", settings.gap as i32),
            4.0..=24.0,
            settings.gap,
            Message::SettingsGapChanged,
            1.0,
            alpha,
        ),
        slider_row(
            "Corner radius",
            format!("{} px", settings.panel_radius as i32),
            0.0..=20.0,
            settings.panel_radius,
            Message::SettingsRadiusChanged,
            1.0,
            alpha,
        ),
        slider_row(
            "Border thickness",
            format!("{} px", settings.border_thickness as i32),
            0.0..=4.0,
            settings.border_thickness,
            Message::SettingsBorderChanged,
            1.0,
            alpha,
        ),
    ]
    .spacing(theme::SPACE_MD)
    .into()
}

fn color_dot(color: Color, alpha: f32) -> Element<'static, Message> {
    container(Space::new())
        .width(Length::Fixed(SWATCH_SIZE))
        .height(Length::Fixed(SWATCH_SIZE))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(theme::fade(color, alpha))),
            border: Border {
                color: theme::fade(theme::border(), alpha),
                width: 1.0,
                radius: 5.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn slider_row<'a>(
    label: &'a str,
    value_label: String,
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    on_change: fn(f32) -> Message,
    step: f32,
    alpha: f32,
) -> Element<'a, Message> {
    column![
        row![
            text(label)
                .size(theme::TEXT_SM)
                .color(theme::fade(theme::text_2(), alpha)),
            Space::new().width(Fill),
            text(value_label)
                .size(theme::TEXT_SM)
                .color(theme::fade(theme::text_4(), alpha)),
        ]
        .align_y(Alignment::Center),
        slider(range, value, on_change)
            .step(step)
            .style(theme::fade_slider(alpha)),
    ]
    .spacing(theme::SPACE_XS)
    .into()
}

fn actions<'a>(alpha: f32) -> Element<'a, Message> {
    row![
        button(text("Reset appearance").size(theme::TEXT_SM))
            .style(theme::fade_button(theme::secondary_button, alpha))
            .padding([theme::SPACE_XS, theme::SPACE_SM])
            .on_press(Message::SettingsReset),
        Space::new().width(Fill),
        button(text("Done").size(theme::TEXT_SM))
            .style(theme::fade_button(theme::primary_button, alpha))
            .padding([theme::SPACE_XS, theme::SPACE_SM])
            .on_press(Message::SettingsClosed),
    ]
    .align_y(Alignment::Center)
    .into()
}
