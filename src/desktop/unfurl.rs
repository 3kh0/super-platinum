//! Message attachments → `AttachmentVm`.
//!
//! Slack packs four different things into `message.attachments`: shared-message
//! unfurls (`is_msg_unfurl`), link unfurls (`service_name` + `title_link`), app
//! unfurls (`is_app_unfurl`), and legacy bot attachments (`color` + `fields`).
//! They all project onto one shape so the timeline draws a single embed widget.

use super_platinum_core::MediaAssetKind;
use super_platinum_core::slack::models::Attachment;
use super_platinum_core::state::Workspace;

use crate::blocks::{BlockCtx, block_nodes, mrkdwn_blocks, plain_inline_nodes};
use crate::media::MediaRegistry;
use crate::model::{AttachmentAuthorVm, AttachmentFooterVm, AttachmentVm, RichNode};

pub(crate) fn attachment_vms(
    workspace: &Workspace,
    message: &super_platinum_core::slack::models::Message,
    media: &MediaRegistry,
) -> Vec<AttachmentVm> {
    let ctx = BlockCtx::new(workspace, media);
    message
        .attachments
        .iter()
        .enumerate()
        .map(|(index, attachment)| attachment_vm(ctx, index, attachment))
        .filter(|attachment| !is_empty(attachment))
        .collect()
}

/// Slack sends placeholder attachments (a bare `fallback`, or a private-channel
/// prompt) that render as an empty bordered box. Drop those.
fn is_empty(attachment: &AttachmentVm) -> bool {
    attachment.author.is_none()
        && attachment.title.is_none()
        && attachment.service.is_none()
        && attachment.body.is_empty()
        && attachment.pretext.is_empty()
        && attachment.fields.is_empty()
        && attachment.files.is_empty()
        && attachment.image.is_none()
        && attachment.thumb.is_none()
        && attachment.footer.is_none()
}

fn attachment_vm(ctx: BlockCtx<'_>, index: usize, attachment: &Attachment) -> AttachmentVm {
    let mrkdwn = |field: &str| attachment.mrkdwn_in.iter().any(|name| name == field);
    AttachmentVm {
        key: attachment
            .id
            .map(|id| format!("a{id}"))
            .unwrap_or_else(|| format!("i{index}")),
        color: attachment.color.as_deref().and_then(border_color),
        service: non_empty(attachment.service_name.as_deref()),
        service_icon: attachment.service_icon.as_deref().map(|url| icon(ctx, url)),
        author: author_vm(ctx, attachment),
        pretext: attachment
            .pretext
            .as_deref()
            .filter(|text| !text.is_empty())
            .map(|text| body_nodes(ctx, text, mrkdwn("pretext")))
            .unwrap_or_default(),
        title: non_empty(attachment.title.as_deref())
            .map(|title| super_platinum_core::state::emoji_text_to_display(&title)),
        title_link: non_empty(attachment.title_link.as_deref()),
        body: body(ctx, attachment, mrkdwn("text")),
        fields: attachment
            .fields
            .iter()
            .filter_map(|field| {
                let value = field.value.as_deref().filter(|value| !value.is_empty())?;
                Some((
                    field.title.clone().unwrap_or_default(),
                    body_nodes(ctx, value, mrkdwn("fields")),
                    field.short,
                ))
            })
            .collect(),
        image: attachment
            .image_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .map(|url| {
                (
                    image(ctx, url, MediaAssetKind::Attachment),
                    attachment.image_width.zip(attachment.image_height),
                )
            }),
        thumb: attachment
            .thumb_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .map(|url| image(ctx, url, MediaAssetKind::Attachment)),
        files: attachment
            .files
            .iter()
            .filter_map(|file| file_node(ctx, file))
            .collect(),
        footer: footer_vm(ctx, attachment),
    }
}

/// The attachment body: Block Kit if the attachment carries blocks (Slack does
/// this for message unfurls), else the `text` field.
fn body(ctx: BlockCtx<'_>, attachment: &Attachment, mrkdwn: bool) -> Vec<RichNode> {
    if !attachment.blocks.is_empty() {
        return attachment
            .blocks
            .iter()
            .flat_map(|block| block_nodes(ctx, block))
            .collect();
    }
    attachment
        .text
        .as_deref()
        .filter(|text| !text.is_empty())
        .map(|text| body_nodes(ctx, text, mrkdwn))
        .unwrap_or_default()
}

fn body_nodes(ctx: BlockCtx<'_>, text: &str, mrkdwn: bool) -> Vec<RichNode> {
    if mrkdwn {
        return mrkdwn_blocks(ctx, text);
    }
    text.split('\n')
        .map(|line| RichNode::Paragraph(plain_inline_nodes(ctx, line)))
        .collect()
}

fn author_vm(ctx: BlockCtx<'_>, attachment: &Attachment) -> Option<AttachmentAuthorVm> {
    let name = non_empty(attachment.author_name.as_deref())
        .or_else(|| non_empty(attachment.author_subname.as_deref()))?;
    Some(AttachmentAuthorVm {
        name,
        user_id: non_empty(attachment.author_id.as_deref()),
        icon: author_icon(ctx, attachment),
        link: non_empty(attachment.author_link.as_deref()),
    })
}

/// Slack synthesizes the shared-message footer from the unfurl flags rather than
/// showing the raw `footer` string ("Thread in Slack Conversation").
fn footer_vm(ctx: BlockCtx<'_>, attachment: &Attachment) -> Option<AttachmentFooterVm> {
    if attachment.is_msg_unfurl {
        let from_thread = attachment.is_reply_unfurl || attachment.is_thread_root_unfurl;
        let permalink = non_empty(attachment.from_url.as_deref()).map(|url| {
            (
                if from_thread {
                    "View reply".to_owned()
                } else {
                    "View message".to_owned()
                },
                url,
            )
        });
        return Some(AttachmentFooterVm {
            lead: if from_thread {
                "From a thread in".to_owned()
            } else {
                "Posted in".to_owned()
            },
            channel_label: attachment.channel_id.as_deref().map(|channel| {
                ctx.workspace
                    .channels
                    .get(channel)
                    .map(|channel| {
                        super_platinum_core::state::channel_display_name(ctx.workspace, channel)
                    })
                    .unwrap_or_else(|| channel.to_owned())
            }),
            channel_id: non_empty(attachment.channel_id.as_deref()),
            stamp: attachment
                .ts
                .as_deref()
                .map(super_platinum_core::state::format_ts_unfurl_label),
            permalink,
            icon: None,
        });
    }
    let lead = non_empty(attachment.footer.as_deref())
        .map(|footer| super_platinum_core::state::emoji_text_to_display(&footer))?;
    Some(AttachmentFooterVm {
        lead,
        channel_id: None,
        channel_label: None,
        stamp: attachment
            .ts
            .as_deref()
            .map(super_platinum_core::state::format_ts_unfurl_label),
        permalink: None,
        icon: attachment
            .footer_icon
            .as_deref()
            .filter(|url| !url.is_empty())
            .map(|url| icon(ctx, url)),
    })
}

/// Prefer the workspace's own avatar so the embed matches the sidebar, and route
/// it through the identity-keyed avatar cache — the author of a shared message is
/// a real person whose picture the app already has. A bot-supplied `author_icon`
/// is just an image.
fn author_icon(
    ctx: BlockCtx<'_>,
    attachment: &Attachment,
) -> Option<super_platinum_core::MediaAssetId> {
    if let Some(user) = non_empty(attachment.author_id.as_deref())
        && let Some(url) = ctx.workspace.avatar_url(&user)
    {
        return Some(ctx.media.register_avatar(&user, &url));
    }
    let url = non_empty(attachment.author_icon.as_deref())?;
    Some(icon(ctx, &url))
}

fn file_node(
    ctx: BlockCtx<'_>,
    file: &super_platinum_core::slack::models::File,
) -> Option<RichNode> {
    Some(crate::message_vm::file_node(ctx.media, file))
}

fn image(ctx: BlockCtx<'_>, url: &str, kind: MediaAssetKind) -> super_platinum_core::MediaAssetId {
    ctx.media.register_image(kind, url, "image/png")
}

/// The 13–16px marks in an embed's author and footer strips. They repeat across
/// every unfurl from the same service or person, so they go through the
/// persistent icon cache instead of being refetched on every launch.
fn icon(ctx: BlockCtx<'_>, url: &str) -> super_platinum_core::MediaAssetId {
    ctx.media.register_icon(MediaAssetKind::Avatar, url)
}

/// Attachment `color` arrives as a bare hex digest, a `#hex`, or one of Slack's
/// three named levels. Anything else is dropped rather than injected as CSS.
fn border_color(color: &str) -> Option<String> {
    match color.trim() {
        "" => None,
        "good" => Some("var(--success)".to_owned()),
        "warning" => Some("var(--warning)".to_owned()),
        "danger" => Some("var(--danger)".to_owned()),
        value => {
            let hex = value.strip_prefix('#').unwrap_or(value);
            (matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()))
                .then(|| format!("#{hex}"))
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(attachment: serde_json::Value) -> super_platinum_core::slack::models::Message {
        serde_json::from_value(serde_json::json!({
            "ts": "1.0",
            "attachments": [attachment],
        }))
        .expect("message fixture")
    }

    #[test]
    fn projects_a_reply_unfurl_the_way_slack_renders_it() {
        // Trimmed from a real #out-of-context bot post.
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let message = message(serde_json::json!({
            "id": 1,
            "author_icon": "https://avatars.slack-edge.com/x_48.png",
            "author_id": "U1",
            "author_name": "kc",
            "author_subname": "kc",
            "blocks": [{
                "type": "rich_text",
                "elements": [{
                    "type": "rich_text_section",
                    "elements": [{"type": "text", "text": "aarav gets TOUCHED by slack icl"}]
                }]
            }],
            "channel_id": "C2",
            "color": "D0D0D0",
            "fallback": "[…]",
            "footer": "Thread in Slack Conversation",
            "from_url": "https://hackclub.slack.com/archives/C2/p1787153585078499",
            "is_msg_unfurl": true,
            "is_reply_unfurl": true,
            "is_share": true,
            "mrkdwn_in": ["text"],
            "text": "aarav gets TOUCHED by slack icl",
            "ts": "1787153585.078499"
        }));
        let vms = attachment_vms(&core.workspaces["T1"], &message, &media);
        let [vm] = vms.as_slice() else {
            panic!("expected one attachment, got {vms:?}");
        };
        assert_eq!(vm.color.as_deref(), Some("#D0D0D0"));
        let author = vm.author.as_ref().expect("author");
        assert_eq!(author.name, "kc");
        assert!(author.icon.is_some());
        assert!(!vm.body.is_empty());
        let footer = vm.footer.as_ref().expect("footer");
        assert_eq!(footer.lead, "From a thread in");
        // Known channel resolves to its display name, not the raw id.
        assert_eq!(footer.channel_label.as_deref(), Some("ship"));
        assert_eq!(
            footer.permalink.as_ref().map(|(label, _)| label.as_str()),
            Some("View reply")
        );
    }

    #[test]
    fn projects_a_link_unfurl() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let message = message(serde_json::json!({
            "id": 1,
            "service_name": "Stardance - Hack Club",
            "service_icon": "https://stardance.hackclub.com/apple-touch-icon.png",
            "title": "UrStudyBuddy by @kow4566 | Stardance",
            "title_link": "https://stardance.hackclub.com/projects/22862",
            "text": "Here is a buddy for u.",
            "image_url": "https://stardance.hackclub.com/projects/22862/og_image",
            "image_width": 1200,
            "image_height": 630,
            "from_url": "https://stardance.hackclub.com/projects/22862"
        }));
        let vms = attachment_vms(&core.workspaces["T1"], &message, &media);
        let [vm] = vms.as_slice() else {
            panic!("expected one attachment, got {vms:?}");
        };
        assert_eq!(vm.service.as_deref(), Some("Stardance - Hack Club"));
        assert!(vm.service_icon.is_some());
        assert!(vm.title_link.is_some());
        assert_eq!(
            vm.image.as_ref().and_then(|(_, size)| *size),
            Some((1200, 630))
        );
        // A link unfurl has no message footer to synthesize.
        assert!(vm.footer.is_none());
    }

    #[test]
    fn drops_placeholder_attachments() {
        let core = crate::fixture::fixture_core();
        let media = MediaRegistry::default();
        let message = message(serde_json::json!({"id": 1, "fallback": "nothing to show"}));
        assert!(attachment_vms(&core.workspaces["T1"], &message, &media).is_empty());
    }

    #[test]
    fn only_accepts_safe_border_colors() {
        assert_eq!(border_color("D0D0D0").as_deref(), Some("#D0D0D0"));
        assert_eq!(border_color("#36a64f").as_deref(), Some("#36a64f"));
        assert_eq!(border_color("good").as_deref(), Some("var(--success)"));
        assert_eq!(border_color("red; background: url(x)"), None);
        assert_eq!(border_color(""), None);
    }
}
