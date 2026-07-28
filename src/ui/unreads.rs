use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

use iced::widget::{
    Column, Space, button, column, container, mouse_area, row, scrollable, svg, text,
};
use iced::{Alignment, Element, Fill, Font, Length, Padding, font};

use super::{icons, message, theme};
use crate::app::{
    FilePreview, Message, ProfileHoverState, TextSelection, TextSelectionSurface, UnreadsSort,
    UnreadsState,
};
use crate::slack::models::{Channel, Message as SlackMessage};
use crate::state::{self, Workspace};

type Previews = HashMap<String, FilePreview>;

pub fn view<'a>(
    ws: &'a Workspace,
    unreads: &'a UnreadsState,
    file_previews: &'a Previews,
    avatar_previews: &'a Previews,
    emoji_previews: &'a Previews,
    emoji_animation_elapsed: Duration,
    hovered_ts: Option<&'a str>,
    text_selection: Option<&'a TextSelection>,
    profile_hover: Option<&'a ProfileHoverState>,
) -> Element<'a, Message> {
    let header = row![
        text("Unreads")
            .size(theme::TEXT_LG)
            .color(theme::text_1())
            .font(Font {
                weight: font::Weight::Bold,
                ..Font::default()
            }),
        button(
            text(format!("All of {}  ▾", ws.name))
                .size(theme::TEXT_SM)
                .color(theme::text_3())
        )
        .padding([theme::SPACE_XS, theme::SPACE_SM])
        .style(theme::panel_close_button),
        button(
            text(match unreads.sort {
                UnreadsSort::Newest => "Sorted newest to oldest  ▾",
                UnreadsSort::Oldest => "Sorted oldest to newest  ▾",
            })
            .size(theme::TEXT_SM)
            .color(theme::text_3())
        )
        .padding([theme::SPACE_XS, theme::SPACE_SM])
        .style(theme::panel_close_button)
        .on_press(Message::Runtime(
            crate::app::RuntimeMessage::UnreadsSortToggled,
        )),
        Space::new().width(Fill),
    ]
    .spacing(theme::SPACE_XS)
    .align_y(Alignment::Center)
    .padding([theme::SPACE_SM, theme::SPACE_MD]);

    let channels = ordered_channels(ws, unreads.sort);
    let body: Element<'a, Message> = if channels.is_empty() {
        caught_up()
    } else {
        let mut groups = Column::new().width(Fill);
        for channel in channels {
            groups = groups.push(channel_group(
                ws,
                unreads,
                channel,
                file_previews,
                avatar_previews,
                emoji_previews,
                emoji_animation_elapsed,
                hovered_ts,
                text_selection,
                profile_hover,
            ));
        }
        scrollable(groups.padding(Padding::ZERO.right(theme::SCROLLBAR_GUTTER)))
            .on_scroll(|viewport| {
                Message::Runtime(crate::app::RuntimeMessage::UnreadsScrolled {
                    remaining: (viewport.content_bounds().height
                        - viewport.bounds().height
                        - viewport.absolute_offset().y)
                        .max(0.0),
                })
            })
            .style(theme::scrollbar)
            .height(Fill)
            .into()
    };

    container(column![header, theme::divider(), body].height(Fill))
        .width(Fill)
        .height(Fill)
        .style(theme::panel)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn channel_group<'a>(
    ws: &'a Workspace,
    unreads: &'a UnreadsState,
    channel: &'a Channel,
    file_previews: &'a Previews,
    avatar_previews: &'a Previews,
    emoji_previews: &'a Previews,
    emoji_animation_elapsed: Duration,
    hovered_ts: Option<&'a str>,
    text_selection: Option<&'a TextSelection>,
    profile_hover: Option<&'a ProfileHoverState>,
) -> Element<'a, Message> {
    let channel_id = channel.id.as_str();
    let collapsed = unreads.collapsed.contains(channel_id);
    let loaded_count = unread_messages(ws, channel_id).len() as u32;
    let count = ws.unread_total(channel).max(loaded_count);
    let private = channel.is_private || channel.is_group;
    let icon = svg(if private { icons::lock() } else { icons::tag() })
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(theme::sidebar_icon(theme::text_2()));

    let channel_button = button(
        row![
            icon,
            text(state::channel_display_name(ws, channel))
                .size(theme::TEXT_MD)
                .color(theme::text_1())
                .font(Font {
                    weight: font::Weight::Bold,
                    ..Font::default()
                }),
        ]
        .spacing(theme::SPACE_XS)
        .align_y(Alignment::Center),
    )
    .padding([theme::SPACE_XS, 0.0])
    .style(theme::panel_close_button)
    .on_press(Message::Runtime(
        crate::app::RuntimeMessage::UnreadsChannelOpened(channel.id.clone()),
    ));

    let group_header = container(
        row![
            button(
                text(if collapsed { "▸" } else { "▾" })
                    .size(theme::TEXT_MD)
                    .color(theme::text_2())
            )
            .width(Length::Fixed(24.0))
            .padding(0)
            .style(theme::panel_close_button)
            .on_press(Message::Runtime(
                crate::app::RuntimeMessage::UnreadsChannelToggled(channel.id.clone()),
            )),
            channel_button,
            Space::new().width(Fill),
            text(message_count_label(
                count,
                unreads.has_more.contains(channel_id)
            ))
            .size(theme::TEXT_SM)
            .color(theme::text_3()),
            text("Press Esc to")
                .size(theme::TEXT_SM)
                .color(theme::text_3()),
            button(text("Mark as Read").size(theme::TEXT_SM))
                .padding([theme::SPACE_XS, theme::SPACE_SM])
                .style(theme::secondary_button)
                .on_press(Message::Runtime(
                    crate::app::RuntimeMessage::UnreadsMarkRead(channel.id.clone()),
                )),
        ]
        .spacing(theme::SPACE_SM)
        .align_y(Alignment::Center)
        .padding([theme::SPACE_SM, theme::SPACE_MD]),
    )
    .width(Fill)
    .style(theme::unreads_group_header);

    if collapsed {
        return mouse_area(column![group_header, theme::divider_faded(0.7)])
            .on_enter(Message::Runtime(
                crate::app::RuntimeMessage::UnreadsChannelFocused(channel.id.clone()),
            ))
            .into();
    }

    let content: Element<'a, Message> = if unreads.loading.contains_key(channel_id) {
        group_placeholder("Loading messages…")
    } else if unreads.failed.contains(channel_id) {
        group_placeholder("Couldn’t load unread messages.")
    } else if !unreads.loaded.contains(channel_id) {
        group_placeholder("Loading messages…")
    } else {
        let messages = unread_messages(ws, channel_id);
        if messages.is_empty() {
            group_placeholder("No unread messages remain.")
        } else {
            message_list(
                ws,
                channel_id,
                messages,
                file_previews,
                avatar_previews,
                emoji_previews,
                emoji_animation_elapsed,
                hovered_ts,
                text_selection,
                profile_hover,
            )
        }
    };

    mouse_area(column![group_header, theme::divider_faded(0.7), content])
        .on_enter(Message::Runtime(
            crate::app::RuntimeMessage::UnreadsChannelFocused(channel.id.clone()),
        ))
        .into()
}

#[allow(clippy::too_many_arguments)]
fn message_list<'a>(
    ws: &'a Workspace,
    channel_id: &'a str,
    messages: Vec<&'a SlackMessage>,
    file_previews: &'a Previews,
    avatar_previews: &'a Previews,
    emoji_previews: &'a Previews,
    emoji_animation_elapsed: Duration,
    hovered_ts: Option<&'a str>,
    text_selection: Option<&'a TextSelection>,
    profile_hover: Option<&'a ProfileHoverState>,
) -> Element<'a, Message> {
    let mut list = Column::new()
        .spacing(theme::SPACE_XS)
        .padding([theme::SPACE_SM, 0.0]);
    let mut previous_date = None;
    for (message_index, msg) in messages.into_iter().enumerate() {
        let date = msg.ts.as_deref().and_then(state::date_key_for_ts);
        if date != previous_date {
            if let Some(ts) = msg.ts.as_deref() {
                list = list.push(date_separator(state::format_ts_date_label(ts)));
            }
            previous_date = date;
        }
        let hovered = msg.ts.as_deref().is_some() && msg.ts.as_deref() == hovered_ts;
        list = list.push(message::row(
            ws,
            channel_id,
            msg,
            false,
            false,
            false,
            hovered,
            file_previews,
            avatar_previews,
            emoji_previews,
            emoji_animation_elapsed,
            None,
            TextSelectionSurface::Channel {
                channel: channel_id.to_owned(),
            },
            message_index,
            text_selection,
            None,
            profile_hover,
        ));
    }
    list.into()
}

fn date_separator<'a>(label: String) -> Element<'a, Message> {
    let line = || {
        container(Space::new().height(1.0))
            .width(Fill)
            .style(theme::unreads_date_line)
    };
    row![
        line(),
        container(text(label).size(theme::TEXT_SM).color(theme::text_2()))
            .padding([theme::SPACE_XS, theme::SPACE_MD])
            .style(theme::unreads_date_pill),
        line(),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn caught_up<'a>() -> Element<'a, Message> {
    container(
        column![
            text("You’re all caught up")
                .size(theme::TEXT_LG)
                .color(theme::text_1())
                .font(Font {
                    weight: font::Weight::Bold,
                    ..Font::default()
                }),
            text("New unread messages will appear here.")
                .size(theme::TEXT_MD)
                .color(theme::text_3()),
        ]
        .spacing(theme::SPACE_XS)
        .align_x(Alignment::Center),
    )
    .center_x(Fill)
    .center_y(Fill)
    .into()
}

fn group_placeholder<'a>(label: &str) -> Element<'a, Message> {
    container(
        text(label.to_owned())
            .size(theme::TEXT_MD)
            .color(theme::text_4()),
    )
    .width(Fill)
    .padding([theme::SPACE_LG, theme::SPACE_MD])
    .into()
}

fn message_count_label(count: u32, has_more: bool) -> String {
    if has_more || count > 55 {
        return "55+ messages".to_owned();
    }
    match count {
        0 => "No messages".to_owned(),
        1 => "1 message".to_owned(),
        _ => format!("{count} messages"),
    }
}

pub(crate) fn ordered_channels(ws: &Workspace, sort: UnreadsSort) -> Vec<&Channel> {
    let mut channels: Vec<_> = ws
        .channels
        .values()
        .filter(|channel| ws.unread_total(channel) > 0)
        .collect();
    channels.sort_by(|a, b| {
        let order = ws
            .channel_recency(b)
            .cmp(&ws.channel_recency(a))
            .then_with(|| a.id.cmp(&b.id));
        if sort == UnreadsSort::Newest {
            order
        } else {
            match order {
                Ordering::Less => Ordering::Greater,
                Ordering::Greater => Ordering::Less,
                Ordering::Equal => Ordering::Equal,
            }
        }
    });
    channels
}

pub(crate) fn unread_messages<'a>(ws: &'a Workspace, channel: &str) -> Vec<&'a SlackMessage> {
    let Some(messages) = ws.messages.get(channel) else {
        return Vec::new();
    };
    messages
        .messages
        .iter()
        .filter(|message| state::is_channel_timeline_visible(message))
        .filter(|message| {
            let Some(ts) = message.ts.as_deref() else {
                return false;
            };
            messages
                .last_read
                .as_deref()
                .is_none_or(|last_read| state::cmp_ts(Some(ts), Some(last_read)).is_gt())
        })
        .collect()
}
