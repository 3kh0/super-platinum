use super::*;

pub(super) fn body_lines(
    ws: &Workspace,
    msg: &SlackMessage,
    block_lines: Vec<blocks::RenderLine>,
) -> Vec<blocks::RenderLine> {
    if !block_lines.is_empty() {
        return block_lines;
    }
    let body = state::message_text(msg);
    if body.is_empty() {
        Vec::new()
    } else {
        blocks::mrkdwn_lines(ws, &body)
    }
}

pub fn selectable_copy_text(ws: &Workspace, msg: &SlackMessage) -> String {
    let block_lines = blocks::render_lines(ws, msg);
    selectable_copy_text_from_lines(ws, msg, &block_lines)
}

pub(super) fn selectable_copy_text_from_lines(
    ws: &Workspace,
    msg: &SlackMessage,
    block_lines: &[blocks::RenderLine],
) -> String {
    body_lines(ws, msg, block_lines.to_vec())
        .into_iter()
        .map(|line| {
            if line.segments.is_empty() {
                state::emoji_text_to_display(&line.text)
            } else {
                line.segments
                    .iter()
                    .map(|segment| state::emoji_text_to_display(&segment.text))
                    .collect()
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

pub(super) fn selectable_span_count(segments: &[selectable::Segment]) -> usize {
    segments
        .iter()
        .map(|segment| segment.text.graphemes(true).count())
        .sum()
}

pub(super) fn selection_range_for_message(
    selection: Option<&TextSelection>,
    surface: &TextSelectionSurface,
    ts: &str,
    index: usize,
    len: usize,
) -> Option<(usize, usize)> {
    let selection = selection?;
    if len == 0
        || &selection.anchor.surface != surface
        || &selection.focus.surface != surface
        || (selection.anchor.message_ts != ts && selection.focus.message_ts != ts)
            && (index
                < selection
                    .anchor
                    .message_index
                    .min(selection.focus.message_index)
                || index
                    > selection
                        .anchor
                        .message_index
                        .max(selection.focus.message_index))
    {
        return None;
    }

    let anchor = &selection.anchor;
    let focus = &selection.focus;
    let start_index = anchor.message_index.min(focus.message_index);
    let end_index = anchor.message_index.max(focus.message_index);
    if index < start_index || index > end_index {
        return None;
    }

    let anchor_offset = anchor.offset.min(len - 1);
    let focus_offset = focus.offset.min(len - 1);
    let forward = anchor.message_index < focus.message_index
        || (anchor.message_index == focus.message_index && anchor.offset <= focus.offset);

    if anchor.message_index == focus.message_index {
        return Some((
            anchor_offset.min(focus_offset),
            anchor_offset.max(focus_offset),
        ));
    }
    if index != anchor.message_index && index != focus.message_index {
        return Some((0, len - 1));
    }
    if forward {
        if index == anchor.message_index {
            Some((anchor_offset, len - 1))
        } else {
            Some((0, focus_offset))
        }
    } else if index == focus.message_index {
        Some((focus_offset, len - 1))
    } else {
        Some((0, anchor_offset))
    }
}

pub(super) fn line_has_custom_emoji(ws: &Workspace, line: &str) -> bool {
    state::emoji_text_tokens(line).into_iter().any(|token| {
        matches!(
            token,
            state::EmojiTextToken::Emoji(name) if ws.custom_emoji_url(&name).is_some()
        )
    })
}

pub(super) fn selectable_segments(line: &blocks::RenderLine) -> Vec<selectable::Segment> {
    if line.segments.is_empty() {
        return vec![selectable::Segment {
            text: state::emoji_text_to_display(&line.text),
            channel: None,
            user: None,
            mono: line.mono,
            color: None,
            background: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
        }];
    }
    line.segments
        .iter()
        .map(|segment| {
            let style = &segment.style;
            selectable::Segment {
                text: state::emoji_text_to_display(&segment.text),
                channel: segment.channel.clone(),
                user: segment.user.clone(),
                mono: line.mono || style.code,
                color: segment_fg(style),
                background: segment_bg(style),
                bold: style.bold,
                italic: style.italic,
                underline: style.underline,
                strikethrough: style.strike,
            }
        })
        .collect()
}

pub(super) fn emoji_body<'a>(
    lines: &[blocks::RenderLine],
    ws: &Workspace,
    emoji_previews: &HashMap<String, FilePreview>,
    elapsed: Duration,
) -> Element<'a, Message> {
    let mut col = Column::new().spacing(theme::SPACE_XS);
    for line in lines {
        for soft_line in soft_wrap_lines(line) {
            let mut row = Row::new().spacing(0).align_y(Alignment::End).width(Fill);
            for (text_value, mono, style, channel, user) in &soft_line.parts {
                match text_value {
                    SoftPart::Text(value) if !value.is_empty() => {
                        for run in text_runs(value) {
                            row = row.push(text_run(
                                run,
                                *mono,
                                style,
                                channel.as_deref(),
                                user.as_deref(),
                            ));
                        }
                    }
                    SoftPart::Text(_) => {}
                    SoftPart::Emoji(name) => {
                        row = row.push(emoji_inline(
                            ws,
                            name,
                            emoji_previews,
                            elapsed,
                            theme::TEXT_MD,
                        ));
                    }
                }
            }
            col = col.push(row.wrap().vertical_spacing(2));
        }
    }
    col.into()
}

enum SoftPart {
    Text(String),
    Emoji(String),
}

struct SoftLine {
    parts: Vec<(
        SoftPart,
        bool,
        blocks::SegmentStyle,
        Option<String>,
        Option<String>,
    )>,
}

fn soft_wrap_lines(line: &blocks::RenderLine) -> Vec<SoftLine> {
    let mut rows = vec![SoftLine { parts: Vec::new() }];
    for segment in line_segments(line) {
        let mono = line.mono || segment.style.code;
        for token in state::emoji_text_tokens(&segment.text) {
            match token {
                state::EmojiTextToken::Text(value) => {
                    let mut parts = value.split('\n').peekable();
                    while let Some(part) = parts.next() {
                        if !part.is_empty() {
                            rows.last_mut().unwrap().parts.push((
                                SoftPart::Text(part.to_owned()),
                                mono,
                                segment.style.clone(),
                                segment.channel.clone(),
                                segment.user.clone(),
                            ));
                        }
                        if parts.peek().is_some() {
                            rows.push(SoftLine { parts: Vec::new() });
                        }
                    }
                }
                state::EmojiTextToken::Emoji(name) => {
                    rows.last_mut().unwrap().parts.push((
                        SoftPart::Emoji(name),
                        mono,
                        segment.style.clone(),
                        segment.channel.clone(),
                        segment.user.clone(),
                    ));
                }
            }
        }
    }
    rows.retain(|row| !row.parts.is_empty());
    if rows.is_empty() {
        rows.push(SoftLine { parts: Vec::new() });
    }
    rows
}

pub(super) fn segment_fg(style: &blocks::SegmentStyle) -> Option<iced::Color> {
    if style.broadcast {
        Some(theme::broadcast_fg())
    } else if style.mention {
        Some(theme::mention_fg())
    } else if style.link {
        Some(theme::accent_bright())
    } else {
        None
    }
}

pub(super) fn segment_bg(style: &blocks::SegmentStyle) -> Option<iced::Color> {
    if style.broadcast {
        Some(theme::broadcast_bg())
    } else if style.mention {
        Some(theme::mention_bg())
    } else {
        None
    }
}

pub(super) fn line_segments(line: &blocks::RenderLine) -> Vec<blocks::RenderSegment> {
    if line.segments.is_empty() {
        vec![blocks::RenderSegment {
            text: line.text.clone(),
            style: blocks::SegmentStyle::default(),
            channel: None,
            user: None,
        }]
    } else {
        line.segments.clone()
    }
}

pub(super) fn text_runs(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    for c in value.chars() {
        run.push(c);
        if c.is_whitespace() {
            out.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        out.push(run);
    }
    out
}

pub(super) fn text_run<'a>(
    value: String,
    mono: bool,
    style: &blocks::SegmentStyle,
    channel: Option<&str>,
    user: Option<&str>,
) -> Element<'a, Message> {
    let mut font = if mono { Font::MONOSPACE } else { Font::DEFAULT };
    if style.bold {
        font.weight = iced::font::Weight::Bold;
    }
    if style.italic {
        font.style = iced::font::Style::Italic;
    }
    let styled = text(value)
        .size(theme::TEXT_MD)
        .font(font)
        .color(segment_fg(style).unwrap_or(theme::text_2()));
    match (segment_bg(style), channel, user) {
        (Some(_), Some(channel), _) => button(styled)
            .padding([0.0, 3.0])
            .style(theme::inline_mention_button(style.broadcast))
            .on_press(Message::Conversation(
                crate::app::ConversationMessage::ChannelSelected(channel.to_owned()),
            ))
            .into(),
        (Some(_), None, Some(user)) => button(styled)
            .padding([0.0, 3.0])
            .style(theme::inline_mention_button(style.broadcast))
            .on_press(Message::Workspace(
                crate::app::WorkspaceMessage::ProfilePressed(user.to_owned()),
            ))
            .into(),
        (Some(_), None, None) => container(styled)
            .padding([0.0, 3.0])
            .style(theme::inline_mention(style.broadcast))
            .into(),
        (None, _, _) => styled.into(),
    }
}

pub(super) fn reaction_content<'a>(
    ws: &Workspace,
    reaction: &crate::slack::models::Reaction,
    emoji_previews: &HashMap<String, FilePreview>,
    elapsed: Duration,
) -> Element<'a, Message> {
    Row::new()
        .spacing(theme::SPACE_XS)
        .align_y(Alignment::Center)
        .push(emoji_inline(
            ws,
            &reaction.name,
            emoji_previews,
            elapsed,
            theme::TEXT_SM,
        ))
        .push(text(reaction.count.max(1).to_string()).size(theme::TEXT_SM))
        .into()
}

pub fn inline_line<'a>(
    ws: &Workspace,
    line: &str,
    emoji_previews: &HashMap<String, FilePreview>,
    elapsed: Duration,
    size: f32,
    color: Color,
) -> Element<'a, Message> {
    let mut row = Row::new().spacing(0).align_y(Alignment::Center);
    for token in state::emoji_text_tokens(line) {
        match token {
            state::EmojiTextToken::Text(value) if !value.is_empty() => {
                row = row.push(
                    text(value)
                        .size(size)
                        .color(color)
                        .wrapping(text::Wrapping::None),
                );
            }
            state::EmojiTextToken::Text(_) => {}
            state::EmojiTextToken::Emoji(name) => {
                row = row.push(emoji_inline(ws, &name, emoji_previews, elapsed, size));
            }
        }
    }
    row.into()
}

pub(in crate::ui) fn emoji_inline<'a>(
    ws: &Workspace,
    name: &str,
    emoji_previews: &HashMap<String, FilePreview>,
    elapsed: Duration,
    size: f32,
) -> Element<'a, Message> {
    if ws.custom_emoji_url(name).is_some() {
        let key = state::emoji_preview_key(&ws.team_id, name);
        match emoji_previews.get(&key) {
            Some(FilePreview::Loaded(handle)) => {
                return emoji_image(handle.clone(), size);
            }
            Some(FilePreview::Animated {
                frames,
                delays,
                total,
                ..
            }) => {
                if let Some(handle) = animated_frame(frames, delays, *total, elapsed) {
                    return emoji_image(handle, size);
                }
            }
            _ => {}
        }
    }
    text(state::emoji_glyph(name))
        .size(size)
        .color(theme::text_2())
        .into()
}

pub(super) fn emoji_image<'a>(handle: ImageHandle, size: f32) -> Element<'a, Message> {
    image::Image::new(handle)
        .width(Length::Fixed(size + 2.0))
        .height(Length::Fixed(size + 2.0))
        .content_fit(ContentFit::Contain)
        .into()
}

pub(super) fn animated_frame(
    frames: &[ImageHandle],
    delays: &[Duration],
    total: Duration,
    elapsed: Duration,
) -> Option<ImageHandle> {
    let index = animated_frame_index(frames.len(), delays, total, elapsed)?;
    frames.get(index).cloned()
}

pub(super) fn animated_frame_index(
    frame_count: usize,
    delays: &[Duration],
    total: Duration,
    elapsed: Duration,
) -> Option<usize> {
    if frame_count == 0 || total.is_zero() {
        return None;
    }
    let elapsed_ms = elapsed.as_millis() % total.as_millis().max(1);
    let mut cursor = 0u128;
    for (index, delay) in delays.iter().enumerate() {
        cursor += delay.as_millis().max(1);
        if elapsed_ms < cursor {
            return (index < frame_count).then_some(index);
        }
    }
    Some(frame_count - 1)
}

pub(super) fn action_item<'a>(label: &'a str, on_press: Message) -> Element<'a, Message> {
    button(text(label).size(theme::TEXT_SM))
        .padding([2.0, theme::SPACE_SM])
        .style(theme::action_button)
        .on_press(on_press)
        .into()
}

pub fn empty_placeholder<'a>(label: &str) -> Element<'a, Message> {
    container(
        text(label.to_owned())
            .size(theme::TEXT_MD)
            .color(theme::muted()),
    )
    .padding(theme::SPACE_LG)
    .into()
}
