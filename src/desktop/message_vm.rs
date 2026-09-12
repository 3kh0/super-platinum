use super_platinum_core::MediaAssetKind;

use crate::blocks::{BlockCtx, block_nodes, custom_emoji_media, plain_inline_nodes};
use crate::media::MediaRegistry;
use crate::model::{ExternalTeamVm, MessageVm, ReactionVm, RichNode};
use crate::unfurl::attachment_vms;

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
    // The key is the avatar's stable identity (a user id, or a bot/webhook icon
    // key), which is what the media cache keys its entries by.
    let (identity, avatar_url) = super_platinum_core::state::message_avatar(workspace, message);
    let avatar = avatar_url
        .as_deref()
        .map(|url| media.register_avatar(identity.as_deref().unwrap_or(url), url));
    let external_team = message.user.as_deref().and_then(|user| {
        let team = super_platinum_core::state::external_team_for_user(workspace, user)?;
        let name = team
            .name
            .clone()
            .unwrap_or_else(|| "External workspace".into());
        let initials = name
            .chars()
            .find(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_else(|| "E".into());
        let icon = super_platinum_core::state::team_icon_url(team)
            .map(|url| media.register_icon(MediaAssetKind::Avatar, url));
        Some(ExternalTeamVm {
            name,
            initials,
            icon,
        })
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
        external_team,
        body: message_body(workspace, message, media),
        edited: message.edited.is_some(),
        is_own: message.user.as_deref() == Some(workspace.self_user_id.as_str()),
        is_app: super_platinum_core::state::is_app_message(message),
        pending,
        compact: false,
        date_label: None,
        show_unread_divider: false,
        reactions: reaction_vms(workspace, message, media),
        attachments: attachment_vms(workspace, message, media),
        reply_count: message.reply_count.unwrap_or(0),
        reply_avatars: reply_avatars(workspace, message, media),
        last_reply: message
            .latest_reply
            .as_deref()
            .map(super_platinum_core::state::format_relative_ts),
    }
}

/// Reaction pills resolve like inline emoji: workspace custom emoji become
/// images, standard shortcodes become glyphs, unknown names stay `:name:`.
fn reaction_vms(
    workspace: &super_platinum_core::state::Workspace,
    message: &super_platinum_core::slack::models::Message,
    media: &MediaRegistry,
) -> Vec<ReactionVm> {
    let ctx = BlockCtx::new(workspace, media);
    message
        .reactions
        .iter()
        .map(|reaction| {
            let media = custom_emoji_media(ctx, &reaction.name);
            ReactionVm {
                glyph: media
                    .is_none()
                    .then(|| super_platinum_core::state::emoji_glyph(&reaction.name)),
                media,
                name: reaction.name.clone(),
                count: reaction.count.max(1),
                own: super_platinum_core::state::reaction_has_user(
                    reaction,
                    &workspace.self_user_id,
                ),
            }
        })
        .collect()
}

/// Avatars for the thread reply bar, in Slack's order (first repliers first).
fn reply_avatars(
    workspace: &super_platinum_core::state::Workspace,
    message: &super_platinum_core::slack::models::Message,
    media: &MediaRegistry,
) -> Vec<(String, Option<super_platinum_core::MediaAssetId>, String)> {
    message
        .reply_users
        .iter()
        .take(5)
        .map(|user| {
            let name = workspace.display_name(user);
            let initials = name
                .chars()
                .next()
                .map(|first| first.to_uppercase().to_string())
                .unwrap_or_else(|| "?".into());
            (
                user.clone(),
                workspace
                    .avatar_url(user)
                    .as_deref()
                    .map(|url| media.register_avatar(user, url)),
                initials,
            )
        })
        .collect()
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
    let ctx = BlockCtx::new(workspace, media);
    let mut nodes = message
        .blocks
        .iter()
        .flat_map(|block| block_nodes(ctx, block))
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        // Fall back to `text` only when the blocks rendered nothing — otherwise
        // the fallback string duplicates the Block Kit body.
        let text = super_platinum_core::state::message_text(message);
        if text.contains('\n') {
            for line in text.split('\n') {
                nodes.push(RichNode::Paragraph(plain_inline_nodes(ctx, line)));
            }
        } else if !text.is_empty() {
            nodes.extend(plain_inline_nodes(ctx, &text));
        }
    }
    for file in &message.files {
        nodes.push(RichNode::Paragraph(vec![file_node(media, file)]));
    }
    nodes
}

/// One attached file. What is painted is always a Slack-side thumbnail — the
/// original is registered alongside it but stays out of the download queue
/// until the viewer opens it.
pub(crate) fn file_node(
    media: &MediaRegistry,
    file: &super_platinum_core::slack::models::File,
) -> RichNode {
    let name = file
        .title
        .clone()
        .or_else(|| file.name.clone())
        .unwrap_or_else(|| "Attachment".into());
    let mime = file
        .mimetype
        .clone()
        .unwrap_or_else(|| "application/octet-stream".into());
    let full = file
        .full_url()
        .map(|url| media.register_deferred(MediaAssetKind::Attachment, url, mime.clone()));
    match file.preview() {
        Some(preview) => RichNode::Media {
            // `register_image` picks the cookie by host: an external image must
            // never be fetched with the session cookie attached.
            id: Some(media.register_image(MediaAssetKind::Attachment, &preview.url, preview.mime)),
            full,
            name,
            mime,
            size: preview.size,
        },
        // A PDF, or a clip Slack has not finished transcoding: nothing to draw,
        // and nothing downloaded until the reader opens it.
        None if full.is_some() => RichNode::Media {
            id: None,
            full,
            name,
            mime,
            size: None,
        },
        None => RichNode::Text(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_message(
        value: serde_json::Value,
    ) -> (
        super_platinum_core::CoreAppState,
        super_platinum_core::slack::models::Message,
    ) {
        let core = crate::fixture::fixture_core();
        let message = serde_json::from_value(value).expect("message fixture");
        (core, message)
    }

    #[test]
    fn custom_emoji_reactions_resolve_to_images() {
        let (mut core, message) = workspace_message(serde_json::json!({
            "ts": "1.0",
            "user": "U1",
            "reactions": [
                {"name": "sob-pray", "count": 1, "users": ["U9"]},
                {"name": "interrobang", "count": 6, "users": ["U0"]},
                {"name": "gone-from-workspace", "count": 2, "users": []}
            ]
        }));
        core.workspaces
            .get_mut("T1")
            .expect("fixture workspace")
            .apply_emojis(vec![
                serde_json::from_value(serde_json::json!({
                    "name": "sob-pray",
                    "value": "https://example.test/sob-pray.png"
                }))
                .expect("emoji fixture"),
            ]);
        let media = MediaRegistry::default();
        let vm = message_vm(&core.workspaces["T1"], &message, &media);
        assert!(vm.reactions[0].media.is_some(), "custom emoji → image");
        assert!(vm.reactions[0].glyph.is_none());
        assert_eq!(vm.reactions[1].glyph.as_deref(), Some("⁉️"));
        assert!(vm.reactions[1].own, "self reaction is marked own");
        // An unresolvable name keeps its shortcode rather than vanishing.
        assert_eq!(
            vm.reactions[2].glyph.as_deref(),
            Some(":gone-from-workspace:")
        );
    }

    #[test]
    fn bot_messages_prefer_the_posting_users_avatar() {
        // Slack shows the bot user's real profile image, not the generic
        // `bot_profile.icons` placeholder.
        let (mut core, message) = workspace_message(serde_json::json!({
            "ts": "1.0",
            "user": "U1",
            "bot_id": "B1",
            "bot_profile": {
                "id": "B1",
                "name": "Out of Context",
                "user_id": "U1",
                "icons": {"image_48": "https://a.slack-edge.com/img/plugins/app/bot_48.png"}
            }
        }));
        let media = MediaRegistry::default();
        let workspace = core.workspaces.get("T1").expect("fixture workspace");
        let (_, url) = super_platinum_core::state::message_avatar(workspace, &message);
        assert_eq!(url.as_deref(), Some("https://example.test/maya.png"));
        let vm = message_vm(workspace, &message, &media);
        assert!(vm.avatar.is_some());
        assert!(vm.is_app);
        core.workspaces.clear();
    }

    #[test]
    fn bot_messages_fall_back_to_the_app_icon() {
        let (core, message) = workspace_message(serde_json::json!({
            "ts": "1.0",
            "user": "UNKNOWN",
            "bot_id": "B1",
            "bot_profile": {
                "id": "B1",
                "name": "Out of Context",
                "icons": {"image_48": "https://a.slack-edge.com/img/plugins/app/bot_48.png"}
            }
        }));
        let (_, url) = super_platinum_core::state::message_avatar(&core.workspaces["T1"], &message);
        assert_eq!(
            url.as_deref(),
            Some("https://a.slack-edge.com/img/plugins/app/bot_48.png")
        );
    }

    #[test]
    fn slack_connect_members_get_their_avatar_and_workspace_badge() {
        let (mut core, message) = workspace_message(serde_json::json!({
            "ts": "1.0",
            "user": "U_EXT",
            "text": "hello from outside"
        }));
        let workspace = core.workspaces.get_mut("T1").expect("fixture workspace");
        workspace.users.insert(
            "U_EXT".into(),
            serde_json::from_value(serde_json::json!({
                "id": "U_EXT",
                "name": "external",
                "profile": {
                    "team": "E_VERCEL",
                    "avatar_hash": "12fe3fbf9a8c"
                }
            }))
            .expect("external user"),
        );
        workspace
            .channels
            .values_mut()
            .next()
            .expect("fixture channel")
            .connected_teams
            .push(
                serde_json::from_value(serde_json::json!({
                    "id": "E_VERCEL",
                    "name": "Vercel",
                    "icon": {"image_34": "https://avatars.slack-edge.com/vercel_34.png"}
                }))
                .expect("connected team"),
            );

        let media = MediaRegistry::default();
        let vm = message_vm(workspace, &message, &media);

        assert!(vm.avatar.is_some());
        let team = vm.external_team.expect("external team badge");
        assert_eq!(team.name, "Vercel");
        assert_eq!(team.initials, "V");
        assert!(team.icon.is_some());
    }

    #[test]
    fn block_body_wins_over_the_fallback_text() {
        // The real #out-of-context shape: a context block plus a `text` fallback
        // that repeats the mention and leaks the image's alt text.
        let (core, message) = workspace_message(serde_json::json!({
            "ts": "1.0",
            "user": "U1",
            "text": "user pfp <@U1>",
            "blocks": [{
                "type": "context",
                "elements": [
                    {"type": "image", "image_url": "https://example.test/pfp.png", "alt_text": "user pfp"},
                    {"type": "mrkdwn", "text": "<@U1>"}
                ]
            }]
        }));
        let media = MediaRegistry::default();
        let vm = message_vm(&core.workspaces["T1"], &message, &media);
        let [RichNode::Context(children)] = vm.body.as_slice() else {
            panic!("expected a single context node, got {:?}", vm.body);
        };
        assert!(matches!(children[0], RichNode::InlineImage { .. }));
        assert!(
            matches!(&children[1], RichNode::UserMention { label, .. } if label == "@Maya Chen")
        );
    }

    #[test]
    fn reply_bar_carries_avatars_and_last_reply() {
        let (core, message) = workspace_message(serde_json::json!({
            "ts": "1.0",
            "user": "U1",
            "reply_count": 7,
            "reply_users": ["U1", "U2"],
            "latest_reply": "1.5"
        }));
        let media = MediaRegistry::default();
        let vm = message_vm(&core.workspaces["T1"], &message, &media);
        assert_eq!(vm.reply_count, 7);
        assert_eq!(vm.reply_avatars.len(), 2);
        assert!(vm.reply_avatars[0].1.is_some());
        assert!(vm.last_reply.is_some());
    }
}
