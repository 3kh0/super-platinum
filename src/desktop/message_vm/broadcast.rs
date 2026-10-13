//! Broadcast parent context never fetches or scans other conversations.
use crate::{
    blocks::{BlockCtx, mrkdwn_inline},
    media::MediaRegistry,
    model::RichNode,
};
use super_platinum_core::{slack::models::Message, state::Workspace};

pub(super) fn root(message: &Message) -> Option<String> {
    if !matches!(
        message.subtype.as_deref(),
        Some("thread_broadcast" | "reply_broadcast")
    ) {
        return None;
    }
    let root = message.thread_ts.as_deref()?;
    let ts = message.ts.as_deref()?;
    let valid = root.split_once('.').is_some_and(|(seconds, fraction)| {
        !seconds.is_empty()
            && !fraction.is_empty()
            && fraction.len() <= 6
            && seconds
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
    });
    (valid && root != ts).then(|| root.to_owned())
}
pub(crate) fn preview(
    workspace: &Workspace,
    message: &Message,
    root: &str,
    channel: &str,
    media: &MediaRegistry,
) -> Option<String> {
    // A scoped bag is required even for embedded context so deletion masking
    // never depends on a missing history-row channel field.
    let cached = workspace.messages.get(channel)?;
    if cached.was_deleted(root) {
        return None;
    }
    // Canonical channel bags are timestamp ordered. Exact identity is checked
    // after the logarithmic lookup; missing/malformed cache order falls back.
    let parent = {
        cached
            .messages
            .binary_search_by(|candidate| {
                super_platinum_core::state::cmp_ts(candidate.ts.as_deref(), Some(root))
            })
            .ok()
            .and_then(|index| cached.messages.get(index))
    }
    .filter(|parent| parent.ts.as_deref() == Some(root));
    let embedded = message.extra.get("root").filter(|parent| {
        parent.get("ts").and_then(serde_json::Value::as_str) == Some(root)
            && parent
                .get("channel")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|embedded_channel| embedded_channel == channel)
    });
    let text = if let Some(parent) = parent {
        // Block Kit is authoritative; do not expose stale accessibility/fallback
        // text as the parent summary when a rich body is present.
        if !parent.blocks.is_empty() {
            return None;
        }
        parent.text.as_deref()?
    } else {
        let embedded = embedded?;
        if embedded
            .get("blocks")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|blocks| !blocks.is_empty())
        {
            return None;
        }
        embedded.get("text")?.as_str()?
    };
    // Bound parsing/render metadata even for a large parent message. The main
    // authored message body remains untouched and uses the normal rich renderer.
    let source: String = text.chars().take(512).collect();
    let plain = mrkdwn_inline(BlockCtx::new(workspace, media), &source)
        .iter()
        .map(RichNode::plain_text)
        .collect::<String>();
    let collapsed = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let mut chars = collapsed.chars();
    let mut result: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    Some(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_broadcast_replies_have_a_parent_jump() {
        for subtype in ["thread_broadcast", "reply_broadcast"] {
            let mut message: Message = serde_json::from_value(
                json!({"subtype":subtype,"thread_ts":"100.000001","ts":"101.000001"}),
            )
            .unwrap();
            assert_eq!(root(&message).as_deref(), Some("100.000001"));
            message.thread_ts = message.ts.clone();
            assert_eq!(root(&message), None);
            message.thread_ts = Some("bad-root".into());
            assert_eq!(root(&message), None);
            message.thread_ts = Some("100.000001".into());
            message.subtype = None;
            assert_eq!(root(&message), None);
        }
    }
    #[test]
    fn exact_channel_root_beats_embedded_preview_and_deleted_root_stays_hidden() {
        let mut core = crate::fixture::fixture_core();
        let ws = core.workspaces.get_mut("T1").unwrap();
        let mut bag = super_platinum_core::state::ChannelMessages::default();
        bag.upsert(
            serde_json::from_value(json!({"ts":"100.000001","text":"*Current* parent\ncontext"}))
                .unwrap(),
        );
        ws.messages.insert("C2".into(), bag);
        let mut message: Message = serde_json::from_value(
            json!({"channel":"C2","root":{"ts":"100.000001","text":"old parent"}}),
        )
        .unwrap();
        let media = MediaRegistry::default();
        assert_eq!(
            preview(
                ws,
                &message,
                "100.000001",
                message.channel.as_deref().unwrap_or(""),
                &media
            )
            .as_deref(),
            Some("Current parent context")
        );
        message.channel = None;
        assert_eq!(
            preview(ws, &message, "100.000001", "C2", &media).as_deref(),
            Some("Current parent context")
        );
        assert_eq!(preview(ws, &message, "100.000001", "", &media), None);
        message.channel = Some("C2".into());
        ws.messages.get_mut("C2").unwrap().messages[0].blocks =
            vec![json!({"type":"rich_text","elements":[]})];
        assert_eq!(
            preview(
                ws,
                &message,
                "100.000001",
                message.channel.as_deref().unwrap_or(""),
                &media
            ),
            None
        );
        ws.messages.get_mut("C2").unwrap().messages[0]
            .blocks
            .clear();
        message.channel = Some("C1".into());
        message.extra.remove("root");
        assert_eq!(
            preview(
                ws,
                &message,
                "100.000001",
                message.channel.as_deref().unwrap_or(""),
                &media
            ),
            None
        );
        message.channel = Some("C2".into());
        ws.messages
            .get_mut("C2")
            .unwrap()
            .record_remote_deletion("100.000001");
        message.extra.insert(
            "root".into(),
            json!({"ts":"100.000001","text":"deleted parent"}),
        );
        assert_eq!(
            preview(
                ws,
                &message,
                "100.000001",
                message.channel.as_deref().unwrap_or(""),
                &media
            ),
            None
        );
    }
    #[test]
    fn embedded_identity_and_preview_size_are_bounded() {
        let core = crate::fixture::fixture_core();
        let ws = &core.workspaces["T1"];
        let media = MediaRegistry::default();
        let mut message: Message = serde_json::from_value(
            json!({"channel":"C2","root":{"ts":"100.000001","text":"🦊".repeat(600)}}),
        )
        .unwrap();
        let text = preview(
            ws,
            &message,
            "100.000001",
            message.channel.as_deref().unwrap_or(""),
            &media,
        )
        .unwrap();
        assert_eq!(text.chars().count(), 121);
        message.extra.get_mut("root").unwrap()["channel"] = json!("OTHER");
        assert_eq!(
            preview(
                ws,
                &message,
                "100.000001",
                message.channel.as_deref().unwrap_or(""),
                &media
            ),
            None
        );
    }
}
