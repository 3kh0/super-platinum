//! Inherit only an already-owned exact thread root when the native socket
//! application is about to insert its partial parent into the transcript.
use super_platinum_core::{
    slack::{
        events::{MessageUpdate, MessageUpdateKind},
        models::Message,
    },
    state::ChannelMessages,
};

pub(super) fn inherited_parent(
    transcript: Option<&ChannelMessages>,
    thread: Option<&ChannelMessages>,
    team: &str,
    channel: &str,
    update: &Message,
    original: Option<&MessageUpdate>,
) -> Option<Message> {
    let ts = update.ts.as_deref().filter(|ts| !ts.is_empty())?;
    let original = original?;
    if team.is_empty()
        || channel.is_empty()
        || original.kind() != MessageUpdateKind::Replied
        || !original.matches_identity(channel, update)
        || update.channel.as_deref() != Some(channel)
        || update.team.as_deref().is_some_and(|t| t != team)
        || update.thread_ts.as_deref().is_some_and(|root| root != ts)
        || original.raw().get("ts").and_then(serde_json::Value::as_str) != Some(ts)
        || ["channel", "team", "thread_ts"]
            .into_iter()
            .zip([channel, team, ts])
            .any(|(field, expected)| {
                original
                    .raw()
                    .get(field)
                    .is_some_and(|v| v.as_str() != Some(expected))
            })
        || transcript.is_some_and(|bag| {
            bag.was_deleted(ts) || bag.messages.iter().any(|m| m.ts.as_deref() == Some(ts))
        })
    {
        return None;
    }
    let thread = thread.filter(|bag| !bag.was_deleted(ts))?;
    thread
        .messages
        .iter()
        .find(|m| {
            m.ts.as_deref() == Some(ts)
                && m.channel.as_deref().is_none_or(|c| c == channel)
                && m.team.as_deref().is_none_or(|t| t == team)
                && m.thread_ts.as_deref().is_none_or(|root| root == ts)
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super_platinum_core::slack::{events::RtEvent, realtime::parse_event};

    #[test]
    fn search_reaction_reading_parent_inheritance_requires_exact_original_root_and_preserves_existing_or_deleted_transcript()
     {
        let mut thread = ChannelMessages::default();
        thread.upsert(serde_json::from_value(json!({"ts":"120.000001","team":"T1","channel":"C1","text":"Known root","reactions":[{"name":"eyes","count":5,"users":["U1"]}]})).unwrap());
        let parsed=parse_event(&json!({"type":"message","subtype":"message_replied","channel":"C1","message":{"ts":"120.000001","reply_count":3}}).to_string()).unwrap();
        let RtEvent::MessageChanged {
            channel,
            message,
            original_update,
        } = parsed
        else {
            panic!()
        };
        let original = original_update.as_ref().unwrap();
        assert!(message.reactions.is_empty());
        let parent = inherited_parent(
            None,
            Some(&thread),
            "T1",
            &channel,
            &message,
            Some(original),
        )
        .unwrap();
        assert_eq!(parent.text.as_deref(), Some("Known root"));
        assert_eq!(parent.reactions[0].count, 5);
        assert!(
            inherited_parent(None, Some(&thread), "T1", &channel, &message, None).is_none(),
            "synthetic legacy event cannot inherit"
        );
        assert!(
            inherited_parent(
                None,
                Some(&thread),
                "FOREIGN",
                &channel,
                &message,
                Some(original)
            )
            .is_none()
        );
        assert!(
            inherited_parent(None, Some(&thread), "T1", "OTHER", &message, Some(original))
                .is_none()
        );
        let mut changed = message.clone();
        changed.ts = Some("different".into());
        assert!(
            inherited_parent(
                None,
                Some(&thread),
                "T1",
                &channel,
                &changed,
                Some(original)
            )
            .is_none()
        );
        let mut existing = ChannelMessages::default();
        existing.upsert(serde_json::from_value(json!({"ts":"120.000001","reactions":[]})).unwrap());
        assert!(
            inherited_parent(
                Some(&existing),
                Some(&thread),
                "T1",
                &channel,
                &message,
                Some(original)
            )
            .is_none(),
            "known empty transcript stays authoritative"
        );
        existing.record_remote_deletion("120.000001");
        assert!(
            inherited_parent(
                Some(&existing),
                Some(&thread),
                "T1",
                &channel,
                &message,
                Some(original)
            )
            .is_none()
        );
        thread.record_remote_deletion("120.000001");
        assert!(
            inherited_parent(
                None,
                Some(&thread),
                "T1",
                &channel,
                &message,
                Some(original)
            )
            .is_none()
        );
    }
}
