//! Slack Block Kit → typed `RichNode` translation.
//!
//! Everything the timeline draws for a message body goes through here: the
//! `rich_text` family and the layout blocks (`section`, `context`, `header`,
//! `divider`, `actions`, `image`). The text layer those blocks embed —
//! `mrkdwn`, `<…>` entities, emoji, mentions — lives in [`text`]. Nothing here
//! touches the DOM; `view/rich.rs` renders the nodes this module produces.

mod text;

use text::{channel_mention, emoji_node};
pub(crate) use text::{custom_emoji_media, mrkdwn_blocks, mrkdwn_inline, plain_inline_nodes};

use serde_json::Value;
use super_platinum_core::MediaAssetKind;
use super_platinum_core::state::Workspace;

use crate::media::MediaRegistry;
use crate::model::{ButtonStyle, RichNode};

/// Everything block translation needs to resolve names, emoji, and media.
#[derive(Clone, Copy)]
pub(crate) struct BlockCtx<'a> {
    pub workspace: &'a Workspace,
    pub media: &'a MediaRegistry,
}

impl<'a> BlockCtx<'a> {
    pub fn new(workspace: &'a Workspace, media: &'a MediaRegistry) -> Self {
        Self { workspace, media }
    }

    fn image(&self, kind: MediaAssetKind, url: &str) -> super_platinum_core::MediaAssetId {
        self.media.register_image(kind, url, "image/png")
    }

    /// Context blocks draw their images at 20px, and bots use them for user
    /// avatars, so they go through the persistent icon cache rather than being
    /// refetched on every launch.
    fn inline_image(&self, url: &str, alt: &str) -> RichNode {
        RichNode::InlineImage {
            id: self.media.register_icon(MediaAssetKind::Attachment, url),
            alt: alt.to_owned(),
        }
    }
}

/// Translate one Block Kit block (or nested element) into rich nodes.
pub(crate) fn block_nodes(ctx: BlockCtx<'_>, value: &Value) -> Vec<RichNode> {
    match value.get("type").and_then(Value::as_str) {
        // ── rich_text family ─────────────────────────────────────────────
        Some("rich_text") => elements(value)
            .flat_map(|element| block_nodes(ctx, element))
            .collect(),
        Some("rich_text_section") => {
            // Slack packs multi-paragraph posts as one section with embedded `\n`.
            split_section_on_newlines(child_nodes(ctx, value))
        }
        Some("rich_text_quote") => vec![RichNode::Quote(child_nodes(ctx, value))],
        Some("rich_text_preformatted") => vec![RichNode::Code(plain_children(ctx, value))],
        Some("rich_text_list") => vec![RichNode::List {
            ordered: value.get("style").and_then(Value::as_str) == Some("ordered"),
            indent: value
                .get("indent")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(8) as u8,
            offset: value.get("offset").and_then(Value::as_u64).unwrap_or(0) as u32,
            items: elements(value)
                .map(|element| child_nodes(ctx, element))
                .collect(),
        }],

        // ── layout blocks ────────────────────────────────────────────────
        Some("divider") => vec![RichNode::Divider],
        Some("header") => vec![RichNode::Header(
            text_object(ctx, value.get("text")).unwrap_or_default(),
        )],
        Some("section") => vec![RichNode::Section {
            text: value
                .get("text")
                .and_then(|text| text_object_blocks(ctx, Some(text)))
                .unwrap_or_default(),
            fields: value
                .get("fields")
                .and_then(Value::as_array)
                .map(|fields| {
                    fields
                        .iter()
                        .filter_map(|field| text_object(ctx, Some(field)))
                        .collect()
                })
                .unwrap_or_default(),
            accessory: value
                .get("accessory")
                .and_then(|accessory| block_nodes(ctx, accessory).into_iter().next())
                .map(Box::new),
        }],
        Some("context") => vec![RichNode::Context(
            elements(value)
                .flat_map(|element| context_element(ctx, element))
                .collect(),
        )],
        Some("actions") => vec![RichNode::Actions(
            elements(value)
                .flat_map(|element| block_nodes(ctx, element))
                .collect(),
        )],
        Some("image") => image_block(ctx, value),

        // ── interactive elements ─────────────────────────────────────────
        Some("button") => vec![RichNode::Button {
            label: text_object_plain(value.get("text")).unwrap_or_else(|| "Button".into()),
            url: value
                .get("url")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|url| !url.is_empty()),
            style: ButtonStyle::from_block(value.get("style").and_then(Value::as_str)),
        }],
        // Menus, pickers, and inputs cannot round-trip to Slack from here.
        // Render their placeholder so the block's layout still reads right.
        Some(
            kind @ ("static_select"
            | "external_select"
            | "users_select"
            | "conversations_select"
            | "channels_select"
            | "multi_static_select"
            | "multi_users_select"
            | "multi_channels_select"
            | "multi_conversations_select"
            | "overflow"
            | "datepicker"
            | "timepicker"
            | "datetimepicker"
            | "checkboxes"
            | "radio_buttons"
            | "plain_text_input"
            | "workflow_button"),
        ) => vec![RichNode::Button {
            label: text_object_plain(value.get("placeholder"))
                .or_else(|| text_object_plain(value.get("text")))
                .unwrap_or_else(|| placeholder_label(kind).to_owned()),
            url: None,
            style: ButtonStyle::Default,
        }],

        // ── inline leaves ────────────────────────────────────────────────
        Some("text") => text_leaf_nodes(ctx, value),
        Some("mrkdwn") => mrkdwn_inline(
            ctx,
            value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        Some("plain_text") => plain_inline_nodes(
            ctx,
            value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ),
        Some("link") => {
            let url = value.get("url").and_then(Value::as_str).unwrap_or_default();
            let label = value.get("text").and_then(Value::as_str).unwrap_or(url);
            vec![RichNode::Link {
                label: label.into(),
                url: url.into(),
            }]
        }
        Some("emoji") => {
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("emoji")
                .to_owned();
            vec![emoji_node(ctx, &name)]
        }
        Some("user") => {
            let user = value
                .get("user_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            vec![RichNode::UserMention {
                user_id: user.into(),
                label: format!("@{}", ctx.workspace.display_name(user)),
            }]
        }
        Some("usergroup") => {
            let group = value
                .get("usergroup_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            vec![group_mention(
                ctx,
                group,
                value.get("name").and_then(Value::as_str).unwrap_or("group"),
            )]
        }
        Some("channel") => {
            let channel = value
                .get("channel_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            vec![channel_mention(ctx, channel, channel)]
        }
        Some("broadcast") => {
            let range = value
                .get("range")
                .and_then(Value::as_str)
                .unwrap_or("channel");
            vec![RichNode::StyledText {
                text: format!("@{range}"),
                bold: true,
                italic: false,
                strike: false,
                code: false,
            }]
        }
        Some("date") => vec![RichNode::Text(
            value
                .get("fallback")
                .and_then(Value::as_str)
                .unwrap_or("date")
                .to_owned(),
        )],
        _ => Vec::new(),
    }
}

fn placeholder_label(kind: &str) -> &'static str {
    match kind {
        "overflow" => "More",
        "datepicker" => "Select a date",
        "timepicker" => "Select a time",
        "datetimepicker" => "Select a date and time",
        "plain_text_input" => "Text input",
        _ => "Select",
    }
}

fn group_mention(ctx: BlockCtx<'_>, id: &str, fallback: &str) -> RichNode {
    let group = ctx.workspace.usergroups.get(id);
    RichNode::GroupMention {
        group_id: id.into(),
        label: format!(
            "@{}",
            group
                .map(|g| g.handle.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback)
                .trim_start_matches('@')
        ),
        member: group.is_some_and(|g| g.includes(&ctx.workspace.self_user_id)),
    }
}

fn image_block(ctx: BlockCtx<'_>, value: &Value) -> Vec<RichNode> {
    let Some(url) = value
        .get("image_url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
    else {
        // `slack_file` images carry a permalink instead of a public URL.
        return value
            .get("slack_file")
            .and_then(|file| file.get("url"))
            .and_then(Value::as_str)
            .map(|url| {
                vec![RichNode::ImageBlock {
                    id: ctx.image(MediaAssetKind::Attachment, url),
                    alt: String::new(),
                    title: None,
                    size: None,
                }]
            })
            .unwrap_or_default();
    };
    vec![RichNode::ImageBlock {
        id: ctx.image(MediaAssetKind::Attachment, url),
        alt: value
            .get("alt_text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        title: text_object_plain(value.get("title")),
        size: value
            .get("image_width")
            .and_then(Value::as_u64)
            .zip(value.get("image_height").and_then(Value::as_u64))
            .map(|(width, height)| (width as u32, height as u32)),
    }]
}

/// Context elements are `image` thumbnails interleaved with small text runs.
fn context_element(ctx: BlockCtx<'_>, value: &Value) -> Vec<RichNode> {
    if value.get("type").and_then(Value::as_str) == Some("image") {
        let Some(url) = value.get("image_url").and_then(Value::as_str) else {
            return Vec::new();
        };
        let alt = value
            .get("alt_text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return vec![ctx.inline_image(url, alt)];
    }
    block_nodes(ctx, value)
}

fn elements(value: &Value) -> impl Iterator<Item = &Value> {
    value
        .get("elements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// A Block Kit composition object (`{type: mrkdwn|plain_text, text}`) as inline
/// nodes — used where a single line is expected (fields, headers, context).
fn text_object(ctx: BlockCtx<'_>, value: Option<&Value>) -> Option<Vec<RichNode>> {
    let value = value?;
    let text = value.get("text").and_then(Value::as_str)?;
    Some(match value.get("type").and_then(Value::as_str) {
        Some("mrkdwn") => mrkdwn_inline(ctx, text),
        _ => plain_inline_nodes(ctx, text),
    })
}

/// Same, but allowed to produce block-level nodes (paragraphs, quotes, code) —
/// used for `section.text`, which routinely carries multi-line mrkdwn.
fn text_object_blocks(ctx: BlockCtx<'_>, value: Option<&Value>) -> Option<Vec<RichNode>> {
    let value = value?;
    let text = value.get("text").and_then(Value::as_str)?;
    Some(match value.get("type").and_then(Value::as_str) {
        Some("mrkdwn") => mrkdwn_blocks(ctx, text),
        _ => text
            .split('\n')
            .map(|line| RichNode::Paragraph(plain_inline_nodes(ctx, line)))
            .collect(),
    })
}

/// Flattened text of a composition object, with emoji resolved to glyphs.
/// Button and title labels are plain strings, so custom emoji fall back to
/// their `:shortcode:` rather than becoming inline images.
fn text_object_plain(value: Option<&Value>) -> Option<String> {
    let text = value?.get("text").and_then(Value::as_str)?;
    Some(super_platinum_core::state::emoji_text_to_display(text)).filter(|text| !text.is_empty())
}

pub(crate) fn child_nodes(ctx: BlockCtx<'_>, value: &Value) -> Vec<RichNode> {
    elements(value)
        .flat_map(|element| block_nodes(ctx, element))
        .collect()
}

fn plain_children(ctx: BlockCtx<'_>, value: &Value) -> String {
    child_nodes(ctx, value)
        .iter()
        .map(RichNode::plain_text)
        .collect()
}

fn text_leaf_nodes(ctx: BlockCtx<'_>, value: &Value) -> Vec<RichNode> {
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let style = value.get("style");
    let flag = |name: &str| {
        style
            .and_then(|style| style.get(name))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let (bold, italic, strike, code) = (flag("bold"), flag("italic"), flag("strike"), flag("code"));
    // Unstyled leaves may still embed `<…>` entities and `:emoji:` runs.
    if !bold && !italic && !strike && !code {
        return plain_inline_nodes(ctx, &text);
    }
    if code {
        return vec![RichNode::StyledText {
            text,
            bold,
            italic,
            strike,
            code,
        }];
    }
    // Styled leaves still need emoji and entity expansion inside the run.
    plain_inline_nodes(ctx, &text)
        .into_iter()
        .map(|node| match node {
            RichNode::Text(text) => RichNode::StyledText {
                text,
                bold,
                italic,
                strike,
                code,
            },
            other => other,
        })
        .collect()
}

/// Split a `rich_text_section`'s inline children into real paragraph lines when
/// Slack embeds `\n` inside text leaves (the Hack Piano layout class).
pub(crate) fn split_section_on_newlines(children: Vec<RichNode>) -> Vec<RichNode> {
    let mut lines: Vec<Vec<RichNode>> = vec![Vec::new()];
    for child in children {
        let (text, rebuild): (String, Box<dyn Fn(String) -> RichNode>) = match child {
            RichNode::Text(text) if text.contains('\n') => (text, Box::new(RichNode::Text)),
            RichNode::StyledText {
                text,
                bold,
                italic,
                strike,
                code,
            } if text.contains('\n') => (
                text,
                Box::new(move |piece| RichNode::StyledText {
                    text: piece,
                    bold,
                    italic,
                    strike,
                    code,
                }),
            ),
            other => {
                lines.last_mut().expect("seeded line").push(other);
                continue;
            }
        };
        let mut pieces = text.split('\n');
        if let Some(first) = pieces.next()
            && !first.is_empty()
        {
            lines
                .last_mut()
                .expect("seeded line")
                .push(rebuild(first.to_owned()));
        }
        for piece in pieces {
            lines.push(Vec::new());
            if !piece.is_empty() {
                lines
                    .last_mut()
                    .expect("seeded line")
                    .push(rebuild(piece.to_owned()));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with<'a>(
        core: &'a super_platinum_core::CoreAppState,
        media: &'a MediaRegistry,
    ) -> BlockCtx<'a> {
        BlockCtx::new(&core.workspaces["T1"], media)
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
            other => panic!("expected paragraph, got {other:?}"),
        }
        match &nodes[1] {
            RichNode::Paragraph(children) => {
                assert!(matches!(&children[0], RichNode::Text(t) if t == "Second line"));
                assert!(matches!(&children[1], RichNode::Emoji { .. }));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn context_block_keeps_image_then_mention() {
        // Shape captured from a real #out-of-context bot post.
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let block = serde_json::json!({
            "type": "context",
            "elements": [
                {"type": "image", "image_url": "https://example.test/pfp.png", "alt_text": "user pfp"},
                {"type": "mrkdwn", "text": "<@U1>"}
            ]
        });
        let nodes = block_nodes(ctx_with(&core, &media), &block);
        let [RichNode::Context(children)] = nodes.as_slice() else {
            panic!("expected one context node, got {nodes:?}");
        };
        assert!(matches!(&children[0], RichNode::InlineImage { alt, .. } if alt == "user pfp"));
        assert!(
            matches!(&children[1], RichNode::UserMention { label, .. } if label == "@Maya Chen")
        );
    }

    #[test]
    fn renders_layout_blocks() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let ctx = ctx_with(&core, &media);
        assert_eq!(
            block_nodes(ctx, &serde_json::json!({"type": "divider"})),
            vec![RichNode::Divider]
        );
        let header = block_nodes(
            ctx,
            &serde_json::json!({"type": "header", "text": {"type": "plain_text", "text": "Weather"}}),
        );
        assert!(matches!(header.as_slice(), [RichNode::Header(_)]));
        let actions = block_nodes(
            ctx,
            &serde_json::json!({
                "type": "actions",
                "elements": [{
                    "type": "button",
                    "text": {"type": "plain_text", "text": "Open"},
                    "url": "https://example.test"
                }]
            }),
        );
        let [RichNode::Actions(children)] = actions.as_slice() else {
            panic!("expected actions, got {actions:?}");
        };
        assert!(
            matches!(&children[0], RichNode::Button { label, url, .. } if label == "Open" && url.as_deref() == Some("https://example.test"))
        );
    }

    #[test]
    fn section_carries_fields_and_accessory() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let nodes = block_nodes(
            ctx_with(&core, &media),
            &serde_json::json!({
                "type": "section",
                "text": {"type": "mrkdwn", "text": "*Temp:* 62.4"},
                "fields": [
                    {"type": "mrkdwn", "text": "Wind"},
                    {"type": "mrkdwn", "text": "6.5"}
                ],
                "accessory": {"type": "image", "image_url": "https://example.test/i.png", "alt_text": "icon"}
            }),
        );
        let [
            RichNode::Section {
                text,
                fields,
                accessory,
            },
        ] = nodes.as_slice()
        else {
            panic!("expected section, got {nodes:?}");
        };
        assert_eq!(fields.len(), 2);
        assert!(accessory.is_some());
        let RichNode::Paragraph(children) = &text[0] else {
            panic!("expected paragraph, got {text:?}");
        };
        assert!(
            matches!(&children[0], RichNode::StyledText { text, bold: true, .. } if text == "Temp:")
        );
    }

    #[test]
    fn rich_text_list_keeps_style_and_items() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let nodes = block_nodes(
            ctx_with(&core, &media),
            &serde_json::json!({
                "type": "rich_text_list",
                "style": "ordered",
                "indent": 1,
                "elements": [
                    {"type": "rich_text_section", "elements": [{"type": "text", "text": "one"}]},
                    {"type": "rich_text_section", "elements": [{"type": "text", "text": "two"}]}
                ]
            }),
        );
        let [
            RichNode::List {
                ordered: true,
                indent: 1,
                items,
                ..
            },
        ] = nodes.as_slice()
        else {
            panic!("expected ordered list, got {nodes:?}");
        };
        assert_eq!(items.len(), 2);
    }
}
