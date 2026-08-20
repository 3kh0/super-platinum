//! Slack's text layer: the `mrkdwn` mini-language, `<…>` entities, emoji, and
//! the mention chips both the `rich_text` blocks and the layout blocks embed.

use super::BlockCtx;
use crate::model::RichNode;

/// Slack's `mrkdwn` at block level: fenced code, `>` quotes, and paragraphs.
pub(crate) fn mrkdwn_blocks(ctx: BlockCtx<'_>, text: &str) -> Vec<RichNode> {
    let mut nodes = Vec::new();
    let mut quote: Vec<RichNode> = Vec::new();
    let mut fenced: Option<Vec<&str>> = None;

    let flush_quote = |quote: &mut Vec<RichNode>, nodes: &mut Vec<RichNode>| {
        if !quote.is_empty() {
            nodes.push(RichNode::Quote(std::mem::take(quote)));
        }
    };

    for line in text.split('\n') {
        if let Some(buffer) = fenced.as_mut() {
            if line.trim_end() == "```" {
                nodes.push(RichNode::Code(buffer.join("\n")));
                fenced = None;
            } else {
                buffer.push(line);
            }
            continue;
        }
        if line.trim_start().starts_with("```") {
            flush_quote(&mut quote, &mut nodes);
            let rest = line.trim_start().trim_start_matches("```");
            let mut buffer = Vec::new();
            if !rest.trim().is_empty() {
                buffer.push(rest);
            }
            fenced = Some(buffer);
            continue;
        }
        if let Some(rest) = line.strip_prefix('>') {
            quote.push(RichNode::Paragraph(mrkdwn_inline(
                ctx,
                rest.strip_prefix(' ').unwrap_or(rest),
            )));
            continue;
        }
        flush_quote(&mut quote, &mut nodes);
        nodes.push(RichNode::Paragraph(mrkdwn_inline(ctx, line)));
    }
    if let Some(buffer) = fenced {
        nodes.push(RichNode::Code(buffer.join("\n")));
    }
    flush_quote(&mut quote, &mut nodes);
    nodes
}

/// Slack's inline `mrkdwn`: `*bold*`, `_italic_`, `~strike~`, `` `code` ``,
/// plus the `<…>` entities and `:emoji:` runs plain text already carries.
pub(crate) fn mrkdwn_inline(ctx: BlockCtx<'_>, text: &str) -> Vec<RichNode> {
    let mut nodes = Vec::new();
    for span in mrkdwn_spans(text) {
        if span.code {
            nodes.push(RichNode::StyledText {
                text: span.text,
                bold: false,
                italic: false,
                strike: false,
                code: true,
            });
            continue;
        }
        let styled = span.bold || span.italic || span.strike;
        for node in plain_inline_nodes(ctx, &span.text) {
            match node {
                RichNode::Text(text) if styled => nodes.push(RichNode::StyledText {
                    text,
                    bold: span.bold,
                    italic: span.italic,
                    strike: span.strike,
                    code: false,
                }),
                other => nodes.push(other),
            }
        }
    }
    nodes
}

struct MrkdwnSpan {
    text: String,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

/// Split inline mrkdwn into styled runs. Delimiters only open when a matching
/// closer exists later in the line, so stray `*` and `_` stay literal.
fn mrkdwn_spans(text: &str) -> Vec<MrkdwnSpan> {
    let bytes = text.as_bytes();
    let mut spans: Vec<MrkdwnSpan> = Vec::new();
    let mut buffer = String::new();
    let (mut bold, mut italic, mut strike) = (false, false, false);
    let mut index = 0;

    let flush = |buffer: &mut String,
                 spans: &mut Vec<MrkdwnSpan>,
                 bold: bool,
                 italic: bool,
                 strike: bool| {
        if !buffer.is_empty() {
            spans.push(MrkdwnSpan {
                text: std::mem::take(buffer),
                bold,
                italic,
                strike,
                code: false,
            });
        }
    };

    while index < bytes.len() {
        let byte = bytes[index];
        // Entities are opaque: never look for delimiters inside `<…>`.
        if byte == b'<'
            && let Some(end) = text[index..].find('>')
        {
            buffer.push_str(&text[index..index + end + 1]);
            index += end + 1;
            continue;
        }
        if byte == b'`' {
            let rest = &text[index + 1..];
            let fence = rest.starts_with("``");
            let (open, close) = if fence { (3, "```") } else { (1, "`") };
            if let Some(end) = text[index + open..].find(close) {
                flush(&mut buffer, &mut spans, bold, italic, strike);
                spans.push(MrkdwnSpan {
                    text: text[index + open..index + open + end].to_owned(),
                    bold: false,
                    italic: false,
                    strike: false,
                    code: true,
                });
                index += open + end + close.len();
                continue;
            }
        }
        if matches!(byte, b'*' | b'_' | b'~') {
            let flag = match byte {
                b'*' => &mut bold,
                b'_' => &mut italic,
                _ => &mut strike,
            };
            let toggles = if *flag {
                true
            } else {
                can_open(text, index, byte as char)
            };
            if toggles {
                flush(&mut buffer, &mut spans, bold, italic, strike);
                let flag = match byte {
                    b'*' => &mut bold,
                    b'_' => &mut italic,
                    _ => &mut strike,
                };
                *flag = !*flag;
                index += 1;
                continue;
            }
        }
        let char_len = text[index..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
        buffer.push_str(&text[index..index + char_len]);
        index += char_len;
    }
    flush(&mut buffer, &mut spans, bold, italic, strike);
    spans
}

/// A delimiter opens only when the next character is content and a matching
/// closer follows it — Slack's own rule, and what keeps `a * b` literal.
fn can_open(text: &str, index: usize, marker: char) -> bool {
    let rest = &text[index + marker.len_utf8()..];
    let Some(next) = rest.chars().next() else {
        return false;
    };
    if next.is_whitespace() || next == marker {
        return false;
    }
    rest.char_indices().skip(1).any(|(offset, candidate)| {
        candidate == marker
            && rest[..offset]
                .chars()
                .next_back()
                .is_some_and(|previous| !previous.is_whitespace())
    })
}

// ── plain Slack text ─────────────────────────────────────────────────────

/// Expand Slack's `<…>` entities and `:emoji:` runs in a plain text leaf.
pub(crate) fn plain_inline_nodes(ctx: BlockCtx<'_>, text: &str) -> Vec<RichNode> {
    let mut nodes = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let (before, candidate) = rest.split_at(start);
        append_emoji_text(ctx, before, &mut nodes);
        let Some(end) = candidate.find('>') else {
            append_emoji_text(ctx, candidate, &mut nodes);
            return nodes;
        };
        let token = &candidate[1..end];
        if let Some(user) = token.strip_prefix('@') {
            let (user_id, _) = user.split_once('|').unwrap_or((user, ""));
            nodes.push(RichNode::UserMention {
                user_id: user_id.into(),
                label: format!("@{}", ctx.workspace.display_name(user_id)),
            });
        } else if let Some(channel) = token.strip_prefix('#') {
            let (channel_id, fallback) = channel.split_once('|').unwrap_or((channel, channel));
            nodes.push(channel_mention(ctx, channel_id, fallback));
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
        } else if let Some(email) = token.strip_prefix("mailto:") {
            let (address, label) = email.split_once('|').unwrap_or((email, email));
            nodes.push(RichNode::Link {
                label: label.into(),
                url: format!("mailto:{address}"),
            });
        } else {
            append_emoji_text(ctx, &candidate[..=end], &mut nodes);
        }
        rest = &candidate[end + 1..];
    }
    append_emoji_text(ctx, rest, &mut nodes);
    nodes
}

pub(super) fn channel_mention(ctx: BlockCtx<'_>, channel_id: &str, fallback: &str) -> RichNode {
    let label = ctx
        .workspace
        .channels
        .get(channel_id)
        .map(|channel| super_platinum_core::state::channel_display_name(ctx.workspace, channel))
        .unwrap_or_else(|| fallback.to_owned());
    RichNode::ChannelMention {
        channel_id: channel_id.to_owned(),
        label: format!("#{label}"),
    }
}

fn append_emoji_text(ctx: BlockCtx<'_>, text: &str, nodes: &mut Vec<RichNode>) {
    for token in super_platinum_core::state::emoji_text_tokens(text) {
        match token {
            super_platinum_core::state::EmojiTextToken::Text(text) => {
                if !text.is_empty() {
                    nodes.push(RichNode::Text(text));
                }
            }
            super_platinum_core::state::EmojiTextToken::Emoji(name) => {
                nodes.push(emoji_node(ctx, &name));
            }
        }
    }
}

/// A custom workspace emoji renders as an inline image; anything else resolves
/// to its unicode glyph (or stays as `:shortcode:` when unknown).
pub(crate) fn emoji_node(ctx: BlockCtx<'_>, name: &str) -> RichNode {
    match custom_emoji_media(ctx, name) {
        Some(id) => RichNode::EmojiImage {
            id,
            name: name.to_owned(),
        },
        None => RichNode::Emoji {
            glyph: super_platinum_core::state::emoji_glyph(name),
            name: name.to_owned(),
        },
    }
}

/// Resolve a custom workspace emoji to a media asset, following aliases.
pub(crate) fn custom_emoji_media(
    ctx: BlockCtx<'_>,
    name: &str,
) -> Option<super_platinum_core::MediaAssetId> {
    // Slack appends `::skin-tone-N`; the base name owns the image.
    let base = name.split("::").next().unwrap_or(name);
    let url = ctx.workspace.custom_emoji_url(base)?;
    Some(ctx.media.register_emoji(base, url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaRegistry;

    fn ctx_with<'a>(
        core: &'a super_platinum_core::CoreAppState,
        media: &'a MediaRegistry,
    ) -> BlockCtx<'a> {
        BlockCtx::new(&core.workspaces["T1"], media)
    }

    #[test]
    fn expands_plain_slack_mentions_and_broadcasts() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let nodes = plain_inline_nodes(
            ctx_with(&core, &media),
            "hello <@U1> in <#C2|ship> <!channel>",
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
    fn mrkdwn_styles_only_close_matched_delimiters() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let ctx = ctx_with(&core, &media);
        let nodes = mrkdwn_inline(ctx, "*bold* and 2 * 3 and _it_ and `x*y`");
        assert!(nodes.iter().any(
            |node| matches!(node, RichNode::StyledText { text, bold: true, .. } if text == "bold")
        ));
        assert!(nodes.iter().any(
            |node| matches!(node, RichNode::StyledText { text, italic: true, .. } if text == "it")
        ));
        assert!(nodes.iter().any(
            |node| matches!(node, RichNode::StyledText { text, code: true, .. } if text == "x*y")
        ));
        // The lone `*` between numbers stays literal.
        let flat = nodes
            .iter()
            .filter_map(|node| match node {
                RichNode::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(flat.contains("2 * 3"), "unexpected literal text: {flat}");
    }

    #[test]
    fn mrkdwn_blocks_split_quotes_and_fences() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let nodes = mrkdwn_blocks(
            ctx_with(&core, &media),
            "intro\n> quoted\n> more\n```\ncode\n```",
        );
        assert!(matches!(nodes[0], RichNode::Paragraph(_)));
        let RichNode::Quote(quoted) = &nodes[1] else {
            panic!("expected quote, got {:?}", nodes[1]);
        };
        assert_eq!(quoted.len(), 2);
        assert!(matches!(&nodes[2], RichNode::Code(code) if code == "code"));
    }
}
