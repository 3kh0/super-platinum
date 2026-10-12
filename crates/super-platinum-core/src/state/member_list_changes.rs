//! Member pages are snapshots. A join/leave invalidates them even when it is
//! another person's membership, and even if an older event arrives late.
use crate::slack::{events::RtEvent, models::Channel};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub(super) struct MemberListChanges(BTreeMap<String, u64>);

impl MemberListChanges {
    pub fn revision(&self, channel: &str) -> u64 {
        self.0.get(channel).copied().unwrap_or_default()
    }

    pub fn observe(&mut self, event: &RtEvent, channels: &BTreeMap<String, Channel>) {
        let channel = match event {
            RtEvent::Message(message)
                if matches!(
                    message.subtype.as_deref(),
                    Some("channel_join" | "group_join" | "channel_leave" | "group_leave")
                ) && message.user.as_deref().is_some_and(|id| !id.is_empty()) =>
            {
                message.channel.as_deref()
            }
            RtEvent::Unknown(raw) => match raw.kind.as_str() {
                "member_joined_channel" | "member_left_channel"
                    if raw
                        .rest
                        .get("user")
                        .and_then(|v| v.as_str())
                        .is_some_and(|id| !id.is_empty()) =>
                {
                    raw.rest.get("channel").and_then(|v| v.as_str())
                }
                "channel_joined" | "group_joined" => raw
                    .rest
                    .get("channel")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str()),
                "channel_left" | "group_left" => raw.rest.get("channel").and_then(|v| v.as_str()),
                _ => None,
            },
            _ => None,
        };
        if let Some(channel) = channel.filter(|id| channels.contains_key(*id)) {
            let revision = self.0.entry(channel.into()).or_default();
            *revision = revision.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn member_events_invalidate_only_known_channel_pages() {
        let channels = BTreeMap::from([(
            "C1".into(),
            serde_json::from_value(json!({"id":"C1"})).unwrap(),
        )]);
        let mut changes = MemberListChanges::default();
        for (index, value) in [
            json!({"type":"member_joined_channel","channel":"C1","user":"OTHER","event_ts":"20.0"}),
            // Out-of-order and unstamped frames still request an authoritative read.
            json!({"type":"member_left_channel","channel":"C1","user":"OTHER","event_ts":"19.0"}),
            json!({"type":"group_joined","channel":{"id":"C1"}}),
            json!({"type":"channel_left","channel":"C1"}),
            json!({"type":"message","subtype":"group_leave","channel":"C1","user":"OTHER","ts":"21.0"}),
            json!({"type":"message","subtype":"channel_join","channel":"C1","user":"OTHER","ts":"22.0"}),
        ].into_iter().enumerate() {
            changes.observe(&crate::slack::realtime::parse_event(&value.to_string()).unwrap(), &channels);
            assert_eq!(changes.revision("C1"), index as u64 + 1);
        }
        for value in [
            json!({"type":"member_joined_channel","channel":"UNKNOWN","user":"OTHER"}),
            json!({"type":"member_joined_channel","channel":"C1"}),
            json!({"type":"member_left_channel","channel":"C1","user":""}),
            json!({"type":"message","channel":"C1","user":"OTHER","text":"hello","ts":"23.0"}),
            json!({"type":"channel_rename","channel":{"id":"C1","name":"renamed"},"event_ts":"24.0"}),
        ] {
            changes.observe(
                &crate::slack::realtime::parse_event(&value.to_string()).unwrap(),
                &channels,
            );
        }
        assert_eq!(changes.revision("C1"), 6);
        assert_eq!(changes.revision("UNKNOWN"), 0);
    }
}
