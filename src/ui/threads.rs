use std::collections::{HashMap, HashSet};
use std::time::Duration;

use iced::widget::{Column, button, column, container, row, scrollable, text};
use iced::{Element, Fill, Font, Length, Padding, font};

use super::{composer, message, theme};
use crate::app::{
    ComposerAttachment, FilePreview, Message, ProfileHoverState, TextSelection,
    TextSelectionSurface, ThreadsState,
};
use crate::slack::models::ThreadViewItem;
use crate::state::{self, Workspace};

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    ws: &'a Workspace,
    threads: &'a ThreadsState,
    file_previews: &'a HashMap<String, FilePreview>,
    avatar_previews: &'a HashMap<String, FilePreview>,
    emoji_previews: &'a HashMap<String, FilePreview>,
    emoji_animation_elapsed: Duration,
    hovered_ts: Option<&str>,
    text_selection: Option<&'a TextSelection>,
    profile_hover: Option<&'a ProfileHoverState>,
    composer_content: &'a iced::widget::text_editor::Content,
    composer_attachments: &'a [ComposerAttachment],
) -> Element<'a, Message> {
    let header = column![
        text("Threads")
            .size(theme::TEXT_LG)
            .color(theme::text_1())
            .font(Font {
                weight: font::Weight::Bold,
                ..Font::default()
            }),
        row![
            thread_filter("All", !threads.vip_only),
            thread_filter("VIP", threads.vip_only)
        ]
        .spacing(theme::SPACE_MD),
    ]
    .spacing(theme::SPACE_MD);

    let body: Element<'a, Message> = if threads.items.is_empty() {
        let status = if threads.loading && !threads.loaded {
            "Loading threads…"
        } else {
            "You’re all caught up. Threads you follow will appear here."
        };
        container(text(status).size(theme::TEXT_MD).color(theme::text_3()))
            .center_x(Fill)
            .center_y(Fill)
            .into()
    } else {
        let mut list = Column::new().spacing(theme::SPACE_LG);
        for item in &threads.items {
            list = list.push(thread_card(
                ws,
                item,
                threads.selected.as_ref(),
                file_previews,
                avatar_previews,
                emoji_previews,
                emoji_animation_elapsed,
                hovered_ts,
                text_selection,
                profile_hover,
                composer_content,
                composer_attachments,
            ));
        }
        if threads.loading {
            list = list.push(
                container(text("Loading more threads…").color(theme::text_4())).center_x(Fill),
            );
        }
        scrollable(list.padding(
            Padding::new(theme::SPACE_MD).right(theme::SPACE_MD + theme::SCROLLBAR_GUTTER),
        ))
        .on_scroll(|viewport| {
            Message::Runtime(crate::app::RuntimeMessage::ThreadsScrolled {
                remaining: viewport.content_bounds().height
                    - viewport.absolute_offset().y
                    - viewport.bounds().height,
            })
        })
        .style(theme::scrollbar)
        .height(Fill)
        .into()
    };

    container(column![
        container(header).padding([theme::SPACE_MD, theme::SPACE_LG]),
        theme::divider(),
        body,
    ])
    .width(Fill)
    .height(Fill)
    .style(theme::panel)
    .into()
}

fn thread_filter<'a>(label: &'a str, selected: bool) -> Element<'a, Message> {
    button(
        text(label)
            .size(theme::TEXT_MD)
            .color(if selected {
                theme::text_1()
            } else {
                theme::text_4()
            })
            .font(Font {
                weight: if selected {
                    font::Weight::Semibold
                } else {
                    font::Weight::Normal
                },
                ..Font::default()
            }),
    )
    .padding(0)
    .style(theme::link_button)
    .on_press(Message::Runtime(
        crate::app::RuntimeMessage::ThreadsVipSelected(label == "VIP"),
    ))
    .into()
}

#[allow(clippy::too_many_arguments)]
fn thread_card<'a>(
    ws: &'a Workspace,
    item: &'a ThreadViewItem,
    selected: Option<&(String, String)>,
    file_previews: &'a HashMap<String, FilePreview>,
    avatar_previews: &'a HashMap<String, FilePreview>,
    emoji_previews: &'a HashMap<String, FilePreview>,
    emoji_animation_elapsed: Duration,
    hovered_ts: Option<&str>,
    text_selection: Option<&'a TextSelection>,
    profile_hover: Option<&'a ProfileHoverState>,
    composer_content: &'a iced::widget::text_editor::Content,
    composer_attachments: &'a [ComposerAttachment],
) -> Element<'a, Message> {
    let Some(channel_id) = item.channel() else {
        return container(text("Unavailable thread")).into();
    };
    let Some(root_ts) = item.root_ts() else {
        return container(text("Unavailable thread")).into();
    };
    let channel_label = ws
        .channels
        .get(channel_id)
        .map(|channel| {
            let name = state::channel_display_name(ws, channel);
            if channel.is_im || channel.is_mpim {
                name
            } else {
                format!("# {name}")
            }
        })
        .unwrap_or_else(|| channel_id.clone());
    let active = selected.is_some_and(|(channel, root)| channel == channel_id && root == root_ts);
    let unread_range = item
        .unread_replies
        .first()
        .and_then(|first| first.ts.clone().zip(item.latest_ts().cloned()));
    let open = Message::Runtime(crate::app::RuntimeMessage::ThreadFeedSelected {
        channel: channel_id.clone(),
        root_ts: root_ts.clone(),
        unread_range,
    });

    let mut messages = Column::new().spacing(theme::SPACE_XS).push(message::row(
        ws,
        channel_id,
        &item.root_msg,
        false,
        false,
        true,
        hovered_ts == item.root_msg.ts.as_deref(),
        file_previews,
        avatar_previews,
        emoji_previews,
        emoji_animation_elapsed,
        None,
        TextSelectionSurface::Thread {
            channel: channel_id.clone(),
            root_ts: root_ts.clone(),
        },
        0,
        text_selection,
        None,
        profile_hover,
    ));
    let unread: HashSet<_> = item
        .unread_replies
        .iter()
        .filter_map(|reply| reply.ts.as_deref())
        .collect();
    let mut showed_unread = false;
    for (index, reply) in item.replies().into_iter().enumerate() {
        if !showed_unread && reply.ts.as_deref().is_some_and(|ts| unread.contains(ts)) {
            messages = messages.push(unread_divider());
            showed_unread = true;
        }
        messages = messages.push(message::row(
            ws,
            channel_id,
            reply,
            false,
            false,
            true,
            hovered_ts == reply.ts.as_deref(),
            file_previews,
            avatar_previews,
            emoji_previews,
            emoji_animation_elapsed,
            None,
            TextSelectionSurface::Thread {
                channel: channel_id.clone(),
                root_ts: root_ts.clone(),
            },
            index + 1,
            text_selection,
            None,
            profile_hover,
        ));
    }

    let reply: Element<'a, Message> = if active {
        container(composer::thread_view(
            composer_content,
            composer_attachments,
        ))
        .padding([theme::SPACE_SM, theme::SPACE_MD])
        .into()
    } else {
        container(
            button(text("Reply…").size(theme::TEXT_MD).color(theme::text_4()))
                .width(Fill)
                .padding([theme::SPACE_SM, theme::SPACE_MD])
                .style(theme::secondary_button)
                .on_press(open),
        )
        .padding([theme::SPACE_SM, theme::SPACE_MD])
        .into()
    };

    column![
        text(channel_label)
            .size(theme::TEXT_MD)
            .color(theme::text_2())
            .font(Font {
                weight: font::Weight::Semibold,
                ..Font::default()
            }),
        container(column![messages, reply])
            .width(Fill)
            .style(theme::file_attachment),
    ]
    .spacing(theme::SPACE_SM)
    .into()
}

fn unread_divider<'a>() -> Element<'a, Message> {
    row![
        container(iced::widget::Space::new().width(Fill).height(1))
            .width(Fill)
            .style(theme::unread_divider_line),
        text("New")
            .size(theme::TEXT_SM)
            .color(theme::ping())
            .font(Font {
                weight: font::Weight::Semibold,
                ..Font::default()
            }),
    ]
    .spacing(theme::SPACE_XS)
    .padding([theme::SPACE_XS, theme::SPACE_SM])
    .into()
}
