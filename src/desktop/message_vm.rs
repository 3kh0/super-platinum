use super_platinum_core::MediaAssetKind;

use crate::media::MediaRegistry;
use crate::model::{MessageVm, RichNode};

pub(crate) fn message_vm(
    workspace: &super_platinum_core::state::Workspace,
    message: &super_platinum_core::slack::models::Message,
    media: &MediaRegistry,
) -> MessageVm {
    let author = super_platinum_core::state::message_author_name(workspace, message);
    let initials = author
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    let initials = if initials.is_empty() {
        "?".into()
    } else {
        initials
    };
    let (_, avatar_url) = super_platinum_core::state::message_avatar(workspace, message);
    let avatar = avatar_url.map(|url| {
        media.register(
            MediaAssetKind::Avatar,
            &url,
            "image/jpeg",
            url.contains("slack-edge.com") || url.contains("slack.com"),
        )
    });
    let raw_ts = message
        .ts
        .clone()
        .or_else(|| message.client_msg_id.clone())
        .unwrap_or_else(|| {
            format!(
                "cached-{}",
                super_platinum_core::state::message_text(message)
            )
        });
    let timestamp = message
        .ts
        .as_deref()
        .map(super_platinum_core::state::format_ts_hm)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if message.client_msg_id.is_some() {
                "now".into()
            } else {
                String::new()
            }
        });
    let pending = message
        .ts
        .as_deref()
        .is_some_and(|ts| workspace.messages.values().any(|bag| bag.is_pending(ts)));
    MessageVm {
        id: raw_ts.clone(),
        ts: message.ts.clone().unwrap_or_else(|| raw_ts.clone()),
        user_id: message.user.clone(),
        author,
        timestamp,
        avatar_initials: initials,
        avatar,
        body: message_body(workspace, message, media),
        edited: message.edited.is_some(),
        is_own: message.user.as_deref() == Some(workspace.self_user_id.as_str()),
        is_app: super_platinum_core::state::is_app_message(message),
        pending,
        compact: false,
        date_label: None,
        show_unread_divider: false,
        reactions: message
            .reactions
            .iter()
            .map(|reaction| {
                (
                    reaction.name.clone(),
                    reaction.count,
                    reaction
                        .users
                        .iter()
                        .any(|user| user == &workspace.self_user_id),
                )
            })
            .collect(),
        reply_count: message.reply_count.unwrap_or(0),
    }
}

/// Annotate date separators, compact grouping, and the first-unread divider.
pub(crate) fn annotate_timeline(
    messages: &mut [MessageVm],
    raw_messages: &[&super_platinum_core::slack::models::Message],
    last_read: Option<&str>,
) {
    let mut last_date: Option<String> = None;
    for message in messages.iter_mut() {
        if message.ts.is_empty() {
            continue;
        }
        if let Some(date) = super_platinum_core::state::date_key_for_ts(&message.ts) {
            if Some(&date) != last_date.as_ref() {
                message.date_label = Some(super_platinum_core::state::format_ts_date_label(
                    &message.ts,
                ));
                last_date = Some(date);
            } else {
                message.date_label = None;
            }
        }
    }

    for message in messages.iter_mut() {
        message.compact = false;
        message.show_unread_divider = false;
    }
    for index in 1..raw_messages.len().min(messages.len()) {
        if super_platinum_core::state::same_message_group(
            raw_messages[index - 1],
            raw_messages[index],
        ) {
            messages[index].compact = messages[index].date_label.is_none();
        }
    }

    if let Some(last_read) = last_read {
        let last_key = super_platinum_core::state::ts_key(last_read);
        if let Some(index) = messages.iter().position(|message| {
            !message.ts.is_empty() && super_platinum_core::state::ts_key(&message.ts) > last_key
        }) {
            messages[index].show_unread_divider = true;
            messages[index].compact = false;
        }
    }
}

fn message_body(
    workspace: &super_platinum_core::state::Workspace,
    message: &super_platinum_core::slack::models::Message,
    media: &MediaRegistry,
) -> Vec<RichNode> {
    let mut nodes = message
        .blocks
        .iter()
        .flat_map(|block| block_nodes(workspace, block, media))
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        let text = super_platinum_core::state::message_text(message);
        if text.contains('\n') {
            for line in text.split('\n') {
                nodes.push(RichNode::Paragraph(plain_inline_nodes(
                    workspace, line, media,
                )));
            }
        } else if !text.is_empty() {
            nodes.extend(plain_inline_nodes(workspace, &text, media));
        }
    }
    for file in &message.files {
        let name = file
            .title
            .clone()
            .or_else(|| file.name.clone())
            .unwrap_or_else(|| "Attachment".into());
        let mime = file
            .mimetype
            .clone()
            .unwrap_or_else(|| "application/octet-stream".into());
        let url = file.url_private.as_deref();
        nodes.push(RichNode::Paragraph(match url {
            Some(url) => vec![RichNode::Media {
                id: media.register(MediaAssetKind::Attachment, url, mime.clone(), true),
                name,
                mime,
            }],
            None => vec![RichNode::Text(name)],
        }));
    }
    nodes
}

fn plain_inline_nodes(
    workspace: &super_platinum_core::state::Workspace,
    text: &str,
    media: &MediaRegistry,
) -> Vec<RichNode> {
    let mut nodes = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let (before, candidate) = rest.split_at(start);
        append_emoji_text(workspace, before, media, &mut nodes);
        let Some(end) = candidate.find('>') else {
            append_emoji_text(workspace, candidate, media, &mut nodes);
            return nodes;
        };
        let token = &candidate[1..end];
        if let Some(user) = token.strip_prefix('@') {
            nodes.push(RichNode::UserMention {
                user_id: user.into(),
                label: format!("@{}", workspace.display_name(user)),
            });
        } else if let Some(channel) = token.strip_prefix('#') {
            let (channel_id, fallback) = channel.split_once('|').unwrap_or((channel, channel));
            let label = workspace
                .channels
                .get(channel_id)
                .map(|channel| super_platinum_core::state::channel_display_name(workspace, channel))
                .unwrap_or_else(|| fallback.into());
            nodes.push(RichNode::ChannelMention {
                channel_id: channel_id.into(),
                label: format!("#{label}"),
            });
        } else if let Some(broadcast) = token.strip_prefix('!').or_else(|| token.strip_prefix('|'))
        {
            if broadcast.starts_with("date^") {
                let fallback = broadcast
                    .rsplit_once('|')
                    .map(|(_, fallback)| fallback)
                    .unwrap_or("date");
                nodes.push(RichNode::Text(fallback.into()));
            } else {
                let range = broadcast
                    .split_once('|')
                    .map(|(_, label)| label.trim_start_matches('@'))
                    .unwrap_or_else(|| {
                        broadcast
                            .split_once('^')
                            .map_or(broadcast, |(kind, _)| kind)
                    });
                nodes.push(RichNode::StyledText {
                    text: format!("@{range}"),
                    bold: true,
                    italic: false,
                    strike: false,
                    code: false,
                });
            }
        } else if token.starts_with("https://") || token.starts_with("http://") {
            let (url, label) = token.split_once('|').unwrap_or((token, token));
            nodes.push(RichNode::Link {
                label: label.into(),
                url: url.into(),
            });
        } else {
            append_emoji_text(workspace, &candidate[..=end], media, &mut nodes);
        }
        rest = &candidate[end + 1..];
    }
    append_emoji_text(workspace, rest, media, &mut nodes);
    nodes
}

fn append_emoji_text(
    workspace: &super_platinum_core::state::Workspace,
    text: &str,
    media: &MediaRegistry,
    nodes: &mut Vec<RichNode>,
) {
    for token in super_platinum_core::state::emoji_text_tokens(text) {
        match token {
            super_platinum_core::state::EmojiTextToken::Text(text) => {
                if !text.is_empty() {
                    nodes.push(RichNode::Text(text));
                }
            }
            super_platinum_core::state::EmojiTextToken::Emoji(name) => {
                if let Some(url) = workspace.custom_emoji_url(&name) {
                    nodes.push(RichNode::Media {
                        id: media.register(MediaAssetKind::Emoji, url, "image/png", false),
                        name: format!(":{name}:"),
                        mime: "image/png".into(),
                    });
                } else {
                    nodes.push(RichNode::Emoji {
                        glyph: super_platinum_core::state::emoji_glyph(&name),
                        name,
                    });
                }
            }
        }
    }
}

fn block_nodes(
    workspace: &super_platinum_core::state::Workspace,
    value: &serde_json::Value,
    media: &MediaRegistry,
) -> Vec<RichNode> {
    let kind = value.get("type").and_then(serde_json::Value::as_str);
    match kind {
        Some("rich_text") => value
            .get("elements")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|element| block_nodes(workspace, element, media))
            .collect(),
        Some("rich_text_section") => {
            // Slack packs multi-paragraph posts as one section with embedded `\n`.
            split_section_on_newlines(child_nodes(workspace, value, media))
        }
        Some("rich_text_quote") => vec![RichNode::Quote(child_nodes(workspace, value, media))],
        Some("rich_text_preformatted") => {
            vec![RichNode::Code(plain_children(workspace, value, media))]
        }
        Some("rich_text_list") => value
            .get("elements")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .map(|element| {
                let mut children = vec![RichNode::Text("• ".into())];
                children.extend(block_nodes(workspace, element, media));
                RichNode::Paragraph(children)
            })
            .collect(),
        Some("section") => value
            .get("text")
            .and_then(|text| text.get("text"))
            .and_then(serde_json::Value::as_str)
            .map(|text| {
                text.split('\n')
                    .map(|line| RichNode::Paragraph(plain_inline_nodes(workspace, line, media)))
                    .collect()
            })
            .unwrap_or_default(),
        Some("text") => text_leaf_nodes(workspace, value, media),
        Some("link") => {
            let url = value
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let label = value
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(url);
            vec![RichNode::Link {
                label: label.into(),
                url: url.into(),
            }]
        }
        Some("emoji") => {
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("emoji");
            match workspace.custom_emoji_url(name) {
                Some(url) => vec![RichNode::Media {
                    id: media.register(MediaAssetKind::Emoji, url, "image/png", false),
                    name: format!(":{name}:"),
                    mime: "image/png".into(),
                }],
                None => vec![RichNode::Emoji {
                    name: name.into(),
                    glyph: super_platinum_core::state::emoji_glyph(name),
                }],
            }
        }
        Some("user") => {
            let user = value
                .get("user_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            vec![RichNode::UserMention {
                user_id: user.into(),
                label: format!("@{}", workspace.display_name(user)),
            }]
        }
        Some("channel") => {
            let channel = value
                .get("channel_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let label = workspace
                .channels
                .get(channel)
                .map(|channel| super_platinum_core::state::channel_display_name(workspace, channel))
                .unwrap_or_else(|| channel.into());
            vec![RichNode::ChannelMention {
                channel_id: channel.into(),
                label: format!("#{label}"),
            }]
        }
        Some("broadcast") => {
            let range = value
                .get("range")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("channel");
            vec![RichNode::StyledText {
                text: format!("@{range}"),
                bold: true,
                italic: false,
                strike: false,
                code: false,
            }]
        }
        _ => Vec::new(),
    }
}

fn text_leaf_nodes(
    workspace: &super_platinum_core::state::Workspace,
    value: &serde_json::Value,
    media: &MediaRegistry,
) -> Vec<RichNode> {
    let text = value
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if text.contains('<') || !super_platinum_core::state::emoji_names_in_text(&text).is_empty() {
        return plain_inline_nodes(workspace, &text, media);
    }
    let style = value.get("style");
    let bold = style
        .and_then(|s| s.get("bold"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let italic = style
        .and_then(|s| s.get("italic"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let strike = style
        .and_then(|s| s.get("strike"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let code = style
        .and_then(|s| s.get("code"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if bold || italic || strike || code {
        vec![RichNode::StyledText {
            text,
            bold,
            italic,
            strike,
            code,
        }]
    } else {
        vec![RichNode::Text(text)]
    }
}

/// Split a rich_text_section's inline children into real paragraph lines when
/// Slack embeds `\n` inside text leaves (the Hack Piano layout class).
fn split_section_on_newlines(children: Vec<RichNode>) -> Vec<RichNode> {
    let mut lines: Vec<Vec<RichNode>> = vec![Vec::new()];
    for child in children {
        match child {
            RichNode::Text(text) if text.contains('\n') => {
                let mut pieces = text.split('\n');
                if let Some(first) = pieces.next()
                    && !first.is_empty()
                {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(RichNode::Text(first.to_owned()));
                }
                for piece in pieces {
                    lines.push(Vec::new());
                    if !piece.is_empty() {
                        lines
                            .last_mut()
                            .unwrap()
                            .push(RichNode::Text(piece.to_owned()));
                    }
                }
            }
            RichNode::StyledText {
                text,
                bold,
                italic,
                strike,
                code,
            } if text.contains('\n') => {
                let mut pieces = text.split('\n');
                if let Some(first) = pieces.next()
                    && !first.is_empty()
                {
                    lines.last_mut().unwrap().push(RichNode::StyledText {
                        text: first.to_owned(),
                        bold,
                        italic,
                        strike,
                        code,
                    });
                }
                for piece in pieces {
                    lines.push(Vec::new());
                    if !piece.is_empty() {
                        lines.last_mut().unwrap().push(RichNode::StyledText {
                            text: piece.to_owned(),
                            bold,
                            italic,
                            strike,
                            code,
                        });
                    }
                }
            }
            other => lines.last_mut().unwrap().push(other),
        }
    }
    if lines.len() == 1 {
        return vec![RichNode::Paragraph(lines.pop().unwrap_or_default())];
    }
    lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(RichNode::Paragraph)
        .collect()
}

fn child_nodes(
    workspace: &super_platinum_core::state::Workspace,
    value: &serde_json::Value,
    media: &MediaRegistry,
) -> Vec<RichNode> {
    value
        .get("elements")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|element| block_nodes(workspace, element, media))
        .collect()
}

fn plain_children(
    workspace: &super_platinum_core::state::Workspace,
    value: &serde_json::Value,
    media: &MediaRegistry,
) -> String {
    child_nodes(workspace, value, media)
        .into_iter()
        .map(|node| match node {
            RichNode::Text(text) | RichNode::StyledText { text, .. } | RichNode::Code(text) => text,
            RichNode::Link { label, .. }
            | RichNode::UserMention { label, .. }
            | RichNode::ChannelMention { label, .. } => label,
            RichNode::Emoji { glyph, .. } => glyph,
            RichNode::Media { name, .. } => name,
            RichNode::Paragraph(_) | RichNode::Quote(_) => String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_plain_slack_mentions_and_broadcasts() {
        let core = crate::fixture::fixture_core();
        let workspace = &core.workspaces["T1"];
        let nodes = plain_inline_nodes(
            workspace,
            "hello <@U1> in <#C2|ship> <!channel>",
            &MediaRegistry::default(),
        );
        assert!(nodes.iter().any(
            |node| matches!(node, RichNode::UserMention { label, .. } if label == "@Maya Chen")
        ));
        assert!(nodes.iter().any(
            |node| matches!(node, RichNode::ChannelMention { label, .. } if label == "#ship")
        ));
        assert!(
            nodes.iter().any(
                |node| matches!(node, RichNode::StyledText { text, .. } if text == "@channel")
            )
        );
    }

    #[test]
    fn splits_embedded_newlines_into_paragraphs() {
        let nodes = split_section_on_newlines(vec![
            RichNode::Text("First line\nSecond line".into()),
            RichNode::Emoji {
                name: "ship".into(),
                glyph: "🚢".into(),
            },
        ]);
        assert_eq!(nodes.len(), 2);
        match &nodes[0] {
            RichNode::Paragraph(children) => {
                assert!(matches!(&children[0], RichNode::Text(t) if t == "First line"));
            }
            _ => panic!("expected paragraph"),
        }
        match &nodes[1] {
            RichNode::Paragraph(children) => {
                assert!(matches!(&children[0], RichNode::Text(t) if t == "Second line"));
                assert!(matches!(&children[1], RichNode::Emoji { .. }));
            }
            _ => panic!("expected paragraph"),
        }
    }
}
