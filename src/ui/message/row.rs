use super::*;

pub fn row<'a>(
    ws: &'a Workspace,
    channel_id: &str,
    msg: &'a SlackMessage,
    pending: bool,
    compact: bool,
    in_thread: bool,
    hovered: bool,
    file_previews: &HashMap<String, FilePreview>,
    avatar_previews: &'a HashMap<String, FilePreview>,
    emoji_previews: &HashMap<String, FilePreview>,
    emoji_animation_elapsed: Duration,
    edit_content: Option<&'a Content>,
    selection_surface: TextSelectionSurface,
    message_index: usize,
    text_selection: Option<&TextSelection>,
    pending_attachments: Option<&'a [ComposerAttachment]>,
    profile_hover: Option<&'a ProfileHoverState>,
) -> Element<'a, Message> {
    let author = ws.message_author_name(msg);
    let profile_user = msg
        .user
        .as_deref()
        .filter(|_| msg.bot_profile.is_none() && msg.bot_id.is_none());

    let time = msg
        .ts
        .as_deref()
        .map(state::format_ts_hm)
        .unwrap_or_default();

    let author_label: Element<'a, Message> = text(author.clone())
        .size(theme::TEXT_MD)
        .color(theme::text_1())
        .font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::default()
        })
        .into();
    let author_label = match profile_user {
        Some(user) => profile::trigger(
            author_label,
            ws,
            user,
            format!(
                "message-name:{}:{}",
                in_thread,
                msg.ts.as_deref().unwrap_or_default()
            ),
            profile_hover,
            avatar_previews,
        ),
        None => author_label,
    };

    let mut header = Row::new()
        .spacing(theme::SPACE_SM)
        .align_y(Alignment::Center)
        .push(author_label);

    if is_app_message(msg) {
        header = header.push(
            container(text("APP").size(10.0).font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::default()
            }))
            .padding([1.0, 4.0])
            .style(theme::app_badge),
        );
    }

    header = header.push(text(time).size(theme::TEXT_SM).color(theme::text_5()));

    if msg.edited.is_some() {
        header = header.push(text("(edited)").size(theme::TEXT_SM).color(theme::muted()));
    }
    if pending {
        header = header.push(sending_clock(emoji_animation_elapsed));
    }

    let block_lines = blocks::render_lines(ws, msg);
    let copy_text = selectable_copy_text_from_lines(ws, msg, &block_lines);

    let editable = !pending
        && msg.bot_id.is_none()
        && msg.ts.is_some()
        && msg.user.as_deref() == Some(ws.self_user_id.as_str());

    let thread_ts = thread_target_ts(msg);
    let can_reply = !in_thread && thread_ts.is_some();

    let action_bar: Option<Element<'a, Message>> = if hovered && edit_content.is_none() {
        let can_copy = !copy_text.is_empty();
        let edit_ts = editable.then(|| msg.ts.clone()).flatten();
        (can_reply || can_copy || edit_ts.is_some()).then(|| {
            let copy_text = copy_text.clone();
            let channel_id = channel_id.to_owned();
            let reply_ts = thread_ts.clone().filter(|_| can_reply);
            crate::ui::motion::micro_reveal(true, move |anim, at| {
                let progress = crate::ui::motion::t(anim, at);
                let mut actions = Row::new();
                if let Some(ts) = reply_ts.clone() {
                    actions = actions.push(action_item(
                        "Reply",
                        Message::Conversation(crate::app::ConversationMessage::ThreadOpened {
                            channel: channel_id.clone(),
                            ts,
                            unread_range: None,
                        }),
                    ));
                }
                if can_copy {
                    actions = actions.push(action_item(
                        "Copy",
                        Message::Workspace(crate::app::WorkspaceMessage::CopyMessage(
                            copy_text.clone(),
                        )),
                    ));
                }
                if let Some(ts) = edit_ts.clone() {
                    actions = actions
                        .push(action_item(
                            "Edit",
                            Message::Workspace(crate::app::WorkspaceMessage::EditPressed {
                                channel: channel_id.clone(),
                                ts: ts.clone(),
                            }),
                        ))
                        .push(action_item(
                            "Delete",
                            Message::Workspace(crate::app::WorkspaceMessage::DeletePressed {
                                channel: channel_id.clone(),
                                ts,
                            }),
                        ));
                }
                let bar = container(actions.padding(2))
                    .padding(2)
                    .style(theme::action_bar);
                crate::ui::motion::slide_y(bar.into(), progress, -4.0)
            })
            .into()
        })
    } else {
        None
    };

    if let Some(value) = edit_content {
        let input = container(composer::editor(
            value,
            "Edit message",
            ComposerTarget::Edit,
            Message::Workspace(crate::app::WorkspaceMessage::EditSubmit),
        ))
        .style(theme::file_attachment)
        .padding(theme::SPACE_XS)
        .width(Length::Fixed(360.0));
        let actions = Row::new()
            .spacing(theme::SPACE_SM)
            .push(
                button(text("Save").size(theme::TEXT_SM))
                    .padding([2, 0])
                    .style(theme::link_button)
                    .on_press(Message::Workspace(crate::app::WorkspaceMessage::EditSubmit)),
            )
            .push(
                button(text("Cancel").size(theme::TEXT_SM))
                    .padding([2, 0])
                    .style(theme::link_button)
                    .on_press(Message::Workspace(
                        crate::app::WorkspaceMessage::EditCancelled,
                    )),
            );
        let content = Column::new()
            .spacing(theme::SPACE_XS)
            .push(header)
            .push(input)
            .push(actions);
        let (avatar_key, avatar_url) = ws.message_avatar(msg);
        let avatar = avatar(
            avatar_key.as_deref(),
            avatar_url.as_deref(),
            avatar_previews,
            author.chars().next(),
        );
        let avatar = match profile_user {
            Some(user) => profile::trigger(
                avatar,
                ws,
                user,
                format!(
                    "message-avatar:{}:{}",
                    in_thread,
                    msg.ts.as_deref().unwrap_or_default()
                ),
                profile_hover,
                avatar_previews,
            ),
            None => avatar,
        };
        return container(
            Row::new()
                .spacing(theme::SPACE_SM)
                .align_y(Alignment::Start)
                .push(avatar)
                .push(container(content).width(Fill)),
        )
        .padding([theme::SPACE_XS / 2.0, theme::SPACE_SM])
        .into();
    }

    let mut col = Column::new().spacing(2.0);
    if !compact {
        col = col.push(header);
    }
    let body_lines = body_lines(ws, msg, block_lines);
    if !body_lines.is_empty() {
        if body_lines
            .iter()
            .any(|line| line_has_custom_emoji(ws, &line.text))
        {
            col = col.push(emoji_body(
                &body_lines,
                ws,
                emoji_previews,
                emoji_animation_elapsed,
            ));
        } else {
            // Body renders through the selectable widget so text can be dragged
            // over and copied (iced's plain `text` cannot be selected).
            let mut segments = Vec::new();
            for (i, line) in body_lines.into_iter().enumerate() {
                if i > 0 {
                    segments.push(selectable::Segment::plain("\n"));
                }
                segments.extend(selectable_segments(&line));
            }
            let selected_range = msg.ts.as_deref().and_then(|ts| {
                selection_range_for_message(
                    text_selection,
                    &selection_surface,
                    ts,
                    message_index,
                    selectable_span_count(&segments),
                )
            });
            let selection_active = text_selection.is_some_and(|selection| {
                selection.dragging && selection.anchor.surface == selection_surface
            });
            let mut body = selectable::SelectableText::new(
                &segments,
                theme::TEXT_MD,
                theme::text_2(),
                theme::selection(),
            )
            .selection(selected_range);
            if let Some(ts) = msg.ts.clone() {
                body = body.context(
                    selection_surface.clone(),
                    ts,
                    message_index,
                    selection_active,
                );
            }
            col = col.push(body);
        }
    }

    for file in &msg.files {
        col = col.push(file_row(ws, channel_id, msg, file, file_previews, hovered));
    }

    for att in &msg.attachments {
        col = col.push(attachment_row(ws, channel_id, msg, att, file_previews));
    }

    if let Some(attachments) = pending_attachments {
        col = col.push(composer::pending_attachment_strip(attachments));
    }

    if let (Some(ts), Some(count)) = (thread_ts.filter(|_| !in_thread), msg.reply_count) {
        if count > 0 {
            col = col.push(thread_summary(
                ws,
                msg,
                avatar_previews,
                channel_id,
                &ts,
                count,
            ));
        }
    }

    if !msg.reactions.is_empty() {
        let mut chips = Row::new().spacing(theme::SPACE_XS);
        for r in &msg.reactions {
            let active = state::reaction_has_user(r, &ws.self_user_id);
            let chip: Element<'a, Message> = if let Some(ts) = msg.ts.clone() {
                button(reaction_content(
                    ws,
                    r,
                    emoji_previews,
                    emoji_animation_elapsed,
                ))
                .padding([2, 6])
                .style(theme::reaction_button(active))
                .on_press(Message::Workspace(
                    crate::app::WorkspaceMessage::ReactionPressed {
                        channel: channel_id.to_owned(),
                        ts,
                        name: r.name.clone(),
                    },
                ))
                .into()
            } else {
                container(reaction_content(
                    ws,
                    r,
                    emoji_previews,
                    emoji_animation_elapsed,
                ))
                .padding([2, 6])
                .style(theme::reaction_chip)
                .into()
            };
            chips = chips.push(chip);
        }
        col = col.push(chips);
    }

    let content: Element<'a, Message> = if compact && pending {
        Row::new()
            .spacing(theme::SPACE_XS)
            .align_y(Alignment::Start)
            .push(sending_clock(emoji_animation_elapsed))
            .push(container(col).width(Fill))
            .into()
    } else {
        container(col).width(Fill).into()
    };
    let body = container(
        Row::new()
            .spacing(theme::SPACE_SM)
            .align_y(Alignment::Start)
            .push(if compact {
                avatar_spacer()
            } else {
                let (avatar_key, avatar_url) = ws.message_avatar(msg);
                let avatar = avatar(
                    avatar_key.as_deref(),
                    avatar_url.as_deref(),
                    avatar_previews,
                    author.chars().next(),
                );
                match profile_user {
                    Some(user) => profile::trigger(
                        avatar,
                        ws,
                        user,
                        format!(
                            "message-avatar:{}:{}",
                            in_thread,
                            msg.ts.as_deref().unwrap_or_default()
                        ),
                        profile_hover,
                        avatar_previews,
                    ),
                    None => avatar,
                }
            })
            .push(content),
    )
    .padding([if compact { 1.0 } else { theme::SPACE_XS }, theme::SPACE_SM])
    .width(Fill);

    match action_bar {
        Some(bar) => iced::widget::stack![
            body,
            container(bar)
                .width(Fill)
                .align_right(Fill)
                .padding([0.0, theme::SPACE_MD]),
        ]
        .into(),
        None => body.into(),
    }
}

pub(super) fn is_app_message(msg: &SlackMessage) -> bool {
    msg.subtype.as_deref() == Some("bot_message")
}

pub(super) fn thread_target_ts(msg: &SlackMessage) -> Option<String> {
    match (msg.thread_ts.as_deref(), msg.ts.as_deref()) {
        (Some(root), Some(ts)) if root != ts => Some(root.to_owned()),
        (_, Some(ts)) => Some(ts.to_owned()),
        _ => None,
    }
}

pub(super) fn thread_summary<'a>(
    ws: &Workspace,
    msg: &SlackMessage,
    avatar_previews: &HashMap<String, FilePreview>,
    channel_id: &str,
    ts: &str,
    count: u32,
) -> Element<'a, Message> {
    let mut content = Row::new()
        .spacing(theme::SPACE_XS)
        .align_y(Alignment::Center);

    let users = thread_participants(msg);
    if !users.is_empty() {
        let mut avatars = Row::new().spacing(2).align_y(Alignment::Center);
        for user in users {
            avatars = avatars.push(thread_participant_avatar(ws, avatar_previews, user));
        }
        content = content.push(avatars);
    }

    content = content.push(
        text(format!(
            "{count} repl{}",
            if count == 1 { "y" } else { "ies" }
        ))
        .size(theme::TEXT_SM),
    );

    button(content)
        .padding([2, 0])
        .style(theme::link_button)
        .on_press(Message::Conversation(
            crate::app::ConversationMessage::ThreadOpened {
                channel: channel_id.to_owned(),
                ts: ts.to_owned(),
                unread_range: None,
            },
        ))
        .into()
}

pub(super) fn thread_participants(msg: &SlackMessage) -> Vec<&str> {
    let mut users = Vec::new();
    for user in &msg.reply_users {
        if users.len() >= 5 {
            break;
        }
        if !users.contains(&user.as_str()) {
            users.push(user.as_str());
        }
    }
    users
}

pub(super) fn thread_participant_avatar<'a>(
    ws: &Workspace,
    avatar_previews: &HashMap<String, FilePreview>,
    user: &str,
) -> Element<'a, Message> {
    let fallback = ws.display_name(user).chars().next();
    avatar_with_size(
        Some(user),
        ws.avatar_url(user).as_deref(),
        avatar_previews,
        fallback,
        20.0,
        5.0,
    )
}

pub(super) fn avatar_spacer<'a>() -> Element<'a, Message> {
    iced::widget::Space::new()
        .width(Length::Fixed(theme::MSG_AVATAR))
        .into()
}

pub(super) fn sending_clock<'a>(elapsed: Duration) -> Element<'a, Message> {
    let hour_end = clock_hand_end(elapsed, 2.6, 4.6);
    let minute_end = clock_hand_end(elapsed, 1.3, 5.7);
    let muted = svg_color(theme::muted());
    let hand = svg_color(theme::text_4());
    let data = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 14 14" fill="none">
<circle cx="7" cy="7" r="5.8" stroke="{muted}" stroke-width="1.25"/>
<path d="M7 7 L{:.3} {:.3}" stroke="{hand}" stroke-width="1.8" stroke-linecap="round"/>
<path d="M7 7 L{:.3} {:.3}" stroke="{hand}" stroke-width="2.4" stroke-linecap="round"/>
<circle cx="7" cy="7" r="1.15" fill="{hand}"/>
</svg>"##,
        hour_end.x, hour_end.y, minute_end.x, minute_end.y
    );
    container(
        svg(svg::Handle::from_memory(data.into_bytes()))
            .width(Length::Fixed(14.0))
            .height(Length::Fixed(14.0)),
    )
    .into()
}

pub(super) fn clock_hand_end(elapsed: Duration, period_secs: f32, length: f32) -> Point {
    let angle = elapsed.as_secs_f32() / period_secs * TAU - TAU / 4.0;
    Point::new(7.0 + angle.cos() * length, 7.0 + angle.sin() * length)
}

pub(super) fn svg_color(color: Color) -> String {
    let red = (color.r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let green = (color.g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let blue = (color.b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{red:02x}{green:02x}{blue:02x}")
}
