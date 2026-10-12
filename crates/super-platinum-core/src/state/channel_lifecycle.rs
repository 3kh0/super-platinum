//! Channel metadata changes carried by Slack's live socket. These are partial
//! updates, not replacement Channel objects. HTTP snapshots use a revision
//! ticket so an in-flight read cannot overwrite a later socket update.
use super::{Workspace, ts_key};
use crate::slack::{events::RtEvent, models::Channel};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Name,
    Topic,
    Purpose,
    Archived,
    Member,
}
#[derive(Debug, Clone)]
struct Version {
    stamp: String,
    revision: u64,
    value: Value,
}
#[derive(Debug, Clone, Default)]
pub struct ChannelLifecycle {
    revision: u64,
    channels: BTreeMap<String, BTreeMap<Field, Version>>,
    changed: BTreeMap<String, u64>,
    pending_discovery: BTreeMap<String, u64>,
    member_lists: super::member_list_changes::MemberListChanges,
}
impl ChannelLifecycle {
    pub(super) fn membership_changed_since(&self, id: &str, since: u64) -> bool {
        self.channels
            .get(id)
            .and_then(|f| f.get(&Field::Member))
            .is_some_and(|v| v.revision > since)
    }
    pub fn members_revision(&self, id: &str) -> u64 {
        self.member_lists.revision(id)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn channel_revision(&self, id: &str) -> u64 {
        self.changed.get(id).copied().unwrap_or(0)
    }
    pub(super) fn touch(&mut self, id: &str) -> u64 {
        self.revision = self.revision.wrapping_add(1);
        self.changed.insert(id.into(), self.revision);
        self.revision
    }
    /// Apply only events newer than the start of this HTTP read. Field clocks
    /// stay independent: a late topic event must not suppress a newer name.
    pub fn overlay(&self, channel: &mut Channel, since: u64) {
        if let Some(fields) = self.channels.get(&channel.id) {
            for (field, version) in fields {
                if version.revision > since {
                    set(channel, *field, &version.value);
                }
            }
        }
    }
}
struct Patch {
    channel: String,
    field: Field,
    value: Value,
    stamp: String,
}
impl Patch {
    fn from_event(event: &RtEvent, self_user: &str) -> Option<Self> {
        let (channel, field, value, stamp) = match event {
            RtEvent::Message(message) => {
                let (field, value) = match message.subtype.as_deref()? {
                    "channel_name" | "group_name" => {
                        (Field::Name, message.extra.get("name")?.clone())
                    }
                    "channel_topic" | "group_topic" => {
                        (Field::Topic, message.extra.get("topic")?.clone())
                    }
                    "channel_purpose" | "group_purpose" => {
                        (Field::Purpose, message.extra.get("purpose")?.clone())
                    }
                    "channel_archive" | "group_archive" => (Field::Archived, true.into()),
                    "channel_unarchive" | "group_unarchive" => (Field::Archived, false.into()),
                    "channel_join" | "group_join" if message.user.as_deref() == Some(self_user) => {
                        (Field::Member, true.into())
                    }
                    "channel_leave" | "group_leave"
                        if message.user.as_deref() == Some(self_user) =>
                    {
                        (Field::Member, false.into())
                    }
                    _ => return None,
                };
                (
                    message.channel.as_ref()?.clone(),
                    field,
                    value,
                    message
                        .extra
                        .get("event_ts")
                        .and_then(Value::as_str)
                        .or(message.ts.as_deref())?
                        .to_owned(),
                )
            }
            RtEvent::Unknown(raw) => {
                let (channel, field, value) = match raw.kind.as_str() {
                    "channel_joined" | "group_joined" => (
                        raw.rest.get("channel")?.get("id")?.as_str()?.to_owned(),
                        Field::Member,
                        true.into(),
                    ),
                    "channel_left" | "group_left" => (
                        raw.rest.get("channel")?.as_str()?.to_owned(),
                        Field::Member,
                        false.into(),
                    ),
                    "member_joined_channel" | "member_left_channel"
                        if raw.rest.get("user")?.as_str()? == self_user =>
                    {
                        (
                            raw.rest.get("channel")?.as_str()?.to_owned(),
                            Field::Member,
                            (raw.kind == "member_joined_channel").into(),
                        )
                    }
                    "channel_rename" | "group_rename" => {
                        let c = raw.rest.get("channel")?;
                        (
                            c.get("id")?.as_str()?.to_owned(),
                            Field::Name,
                            c.get("name")?.clone(),
                        )
                    }
                    "channel_archive" | "group_archive" => (
                        raw.rest.get("channel")?.as_str()?.to_owned(),
                        Field::Archived,
                        true.into(),
                    ),
                    "channel_unarchive" | "group_unarchive" => (
                        raw.rest.get("channel")?.as_str()?.to_owned(),
                        Field::Archived,
                        false.into(),
                    ),
                    _ => return None,
                };
                (
                    channel,
                    field,
                    value,
                    raw.rest
                        .get("event_ts")
                        .or_else(|| raw.rest.get("ts"))?
                        .as_str()?
                        .to_owned(),
                )
            }
            _ => return None,
        };
        if channel.is_empty() || ts_key(&stamp).0 <= 0 {
            return None;
        }
        match field {
            Field::Name if value.as_str().is_none_or(str::is_empty) => return None,
            Field::Topic | Field::Purpose if !value.is_string() => return None,
            Field::Archived | Field::Member if !value.is_boolean() => return None,
            _ => {}
        }
        Some(Self {
            channel,
            field,
            value,
            stamp,
        })
    }
}
fn set(channel: &mut Channel, field: Field, value: &Value) {
    match field {
        Field::Name => {
            channel.name = value.as_str().map(str::to_owned);
            channel
                .extra
                .insert("name_normalized".into(), value.clone());
        }
        Field::Archived => channel.is_archived = value.as_bool().unwrap(),
        Field::Member => {
            channel.extra.insert("is_member".into(), value.clone());
        }
        Field::Topic | Field::Purpose => {
            let key = if field == Field::Topic {
                "topic"
            } else {
                "purpose"
            };
            channel
                .extra
                .insert(key.into(), serde_json::json!({"value":value}));
        }
    }
}
impl Workspace {
    /// Join notifications are discovery hints, including Slack's unstamped
    /// channel_joined frame. Only self membership can initiate a read.
    pub fn begin_channel_discovery(&mut self, event: &RtEvent) -> Option<(String, u64)> {
        let id = match event {
            RtEvent::Unknown(raw) => match raw.kind.as_str() {
                "channel_joined" | "group_joined" => {
                    raw.rest.get("channel")?.get("id")?.as_str()?
                }
                "member_joined_channel" if raw.rest.get("user")?.as_str()? == self.self_user_id => {
                    raw.rest.get("channel")?.as_str()?
                }
                _ => return None,
            },
            RtEvent::Message(message)
                if message.user.as_deref() == Some(&self.self_user_id)
                    && matches!(
                        message.subtype.as_deref(),
                        Some("channel_join" | "group_join")
                    ) =>
            {
                message.channel.as_deref()?
            }
            _ => return None,
        };
        if id.is_empty() || self.channel_lifecycle.pending_discovery.contains_key(id) {
            return None;
        }
        let id = id.to_owned();
        let revision = self.channel_lifecycle.touch(&id);
        self.channel_lifecycle
            .pending_discovery
            .insert(id.clone(), revision);
        Some((id, revision))
    }

    /// A replacement socket makes reads scheduled by its predecessor obsolete.
    pub fn reset_channel_discovery(&mut self) {
        self.channel_lifecycle.pending_discovery.clear();
    }

    pub fn finish_channel_discovery(
        &mut self,
        id: &str,
        revision: u64,
        info: Option<Channel>,
    ) -> bool {
        if self.channel_lifecycle.pending_discovery.get(id) != Some(&revision) {
            return false;
        }
        self.channel_lifecycle.pending_discovery.remove(id);
        let Some(info) = info.filter(|info| info.id == id) else {
            return false;
        };
        self.apply_channel_refresh_since(
            crate::slack::models::GenericInfoPage {
                channels: vec![info],
                ..Default::default()
            },
            revision,
        );
        true
    }

    pub fn apply_channel_event(&mut self, event: &RtEvent) -> bool {
        // Other people joining/leaving invalidate member pages without changing
        // our own membership or inventing a channel from a sparse event.
        self.channel_lifecycle
            .member_lists
            .observe(event, &self.channels);
        let Some(patch) = Patch::from_event(event, &self.self_user_id) else {
            return false;
        };
        // An organization-wide event need not mean membership. Never invent a
        // public/member channel from the sparse rename/archive payload.
        if !self.channels.contains_key(&patch.channel)
            && !self
                .channel_lifecycle
                .pending_discovery
                .contains_key(&patch.channel)
            && !self.channel_lifecycle.channels.contains_key(&patch.channel)
            && patch.field != Field::Member
        {
            return false;
        }
        if self
            .channel_lifecycle
            .channels
            .get(&patch.channel)
            .and_then(|fields| fields.get(&patch.field))
            .is_some_and(|v| ts_key(&v.stamp) >= ts_key(&patch.stamp))
        {
            return false;
        }
        let revision = self.channel_lifecycle.touch(&patch.channel);
        if let Some(channel) = self.channels.get_mut(&patch.channel) {
            set(channel, patch.field, &patch.value);
        }
        self.channel_lifecycle
            .channels
            .entry(patch.channel)
            .or_default()
            .insert(
                patch.field,
                Version {
                    stamp: patch.stamp,
                    revision,
                    value: patch.value,
                },
            );
        true
    }
    /// Apply a confirmed join/leave without overwriting a membership event
    /// received after the mutation began. Protect pending HTTP reads as well.
    pub fn confirm_channel_membership(&mut self, id: &str, member: bool, since: u64) {
        self.confirm_channel_field(id, Field::Member, member, since);
    }
    pub fn confirm_channel_archive(&mut self, id: &str, archived: bool, since: u64) {
        self.confirm_channel_field(id, Field::Archived, archived, since);
    }
    fn confirm_channel_field(&mut self, id: &str, field: Field, enabled: bool, since: u64) {
        if !self.channels.contains_key(id) {
            return;
        }
        let previous = self
            .channel_lifecycle
            .channels
            .get(id)
            .and_then(|f| f.get(&field));
        if previous.is_some_and(|v| v.revision > since) {
            return;
        }
        let stamp = previous.map(|v| v.stamp.clone()).unwrap_or_default();
        let revision = self.channel_lifecycle.touch(id);
        let value = Value::Bool(enabled);
        set(self.channels.get_mut(id).unwrap(), field, &value);
        self.channel_lifecycle
            .channels
            .entry(id.into())
            .or_default()
            .insert(
                field,
                Version {
                    stamp,
                    revision,
                    value,
                },
            );
    }
    /// Merge authoritative details without replacing read/sidebar/Connect state.
    /// Return the effective snapshot for an already-open details dialog.
    pub fn merge_channel_details(&mut self, mut info: Channel, since: u64) -> Channel {
        self.channel_lifecycle.overlay(&mut info, since);
        if let Some(channel) = self.channels.get_mut(&info.id) {
            channel.name = info.name.clone();
            channel.is_archived = info.is_archived;
            for field in [
                "topic",
                "purpose",
                "num_members",
                "created",
                "creator",
                "is_member",
                "is_general",
                "name_normalized",
            ] {
                if let Some(value) = info.extra.get(field) {
                    channel.extra.insert(field.into(), value.clone());
                }
            }
            self.channel_lifecycle.touch(&info.id);
        }
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::WorkspaceSession,
        slack::{
            api::ChannelTextField,
            models::{GenericInfoPage, Team},
            realtime::parse_event,
        },
    };
    use serde_json::json;
    fn workspace() -> Workspace {
        let mut w = Workspace::from_session(&WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U1".into(),
            name: "Test".into(),
            url: "https://example.invalid".into(),
            token: String::new(),
        });
        w.channels.insert(
            "C1".into(),
            Channel {
                id: "C1".into(),
                name: Some("original".into()),
                is_private: true,
                is_starred: true,
                last_read: Some("10.1".into()),
                unread_count: Some(7),
                connected_teams: vec![Team {
                    id: "T_EXTERNAL".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        w
    }
    fn event(value: Value) -> RtEvent {
        parse_event(&value.to_string()).unwrap()
    }
    fn rename(name: Value, stamp: &str) -> RtEvent {
        event(json!({"type":"group_rename","channel":{"id":"C1","name":name},"event_ts":stamp}))
    }
    fn message(field: &str, value: Value, stamp: &str) -> RtEvent {
        let mut e = json!({"type":"message","subtype":format!("channel_{field}"),"channel":"C1","user":"U1","ts":stamp});
        e[field] = value;
        event(e)
    }
    #[test]
    fn fields_order_independently_and_partial_events_preserve_channel_identity_and_read_state() {
        let mut w = workspace();
        assert!(w.apply_channel_event(&rename(json!("new-name"), "20.9")));
        assert!(w.apply_channel_event(&message("topic", json!("topic :rocket:"), "18.5")));
        assert!(!w.apply_channel_event(&rename(json!("old-name"), "20.8")));
        assert!(!w.apply_channel_event(&message("topic", json!("stale"), "18.4")));
        let c = &w.channels["C1"];
        assert_eq!(c.name.as_deref(), Some("new-name"));
        assert_eq!(ChannelTextField::Topic.value(c), "topic :rocket:");
        assert!(c.is_private && c.is_starred);
        assert_eq!(c.unread_count, Some(7));
        assert_eq!(c.last_read.as_deref(), Some("10.1"));
        assert_eq!(c.connected_teams[0].id, "T_EXTERNAL");
    }
    #[test]
    fn unstamped_join_discovers_channel_without_inventing_its_privacy() {
        let mut w = workspace();
        let joined = event(
            json!({"type":"channel_joined","channel":{"id":"C_NEW","name":"private-new","is_private":true}}),
        );
        let (id, ticket) = w.begin_channel_discovery(&joined).unwrap();
        assert!(
            !w.channels.contains_key(&id),
            "wait for authoritative details"
        );
        assert!(
            w.begin_channel_discovery(&joined).is_none(),
            "deduplicate pending reads"
        );
        let info = serde_json::from_value(
            json!({"id":"C_NEW","name":"private-new","is_private":true,"is_member":true}),
        )
        .unwrap();
        assert!(w.finish_channel_discovery(&id, ticket, Some(info)));
        assert!(w.channels[&id].is_private);
        assert_eq!(w.channels[&id].extra["is_member"], true);
    }

    #[test]
    fn discovery_keeps_changes_received_while_details_are_loading() {
        let mut w = workspace();
        let joined = event(
            json!({"type":"member_joined_channel","channel":"C_NEW","user":"U1","event_ts":"50.1"}),
        );
        let (id, ticket) = w.begin_channel_discovery(&joined).unwrap();
        assert!(w.apply_channel_event(&joined));
        assert!(w.apply_channel_event(&event(json!({"type":"channel_rename","channel":{"id":"C_NEW","name":"renamed"},"event_ts":"50.2"}))));
        assert!(w.apply_channel_event(&event(
            json!({"type":"channel_archive","channel":"C_NEW","event_ts":"50.3"})
        )));
        assert!(w.apply_channel_event(&event(
            json!({"type":"member_left_channel","channel":"C_NEW","user":"U1","event_ts":"50.4"})
        )));
        let info = serde_json::from_value(json!({"id":"C_NEW","name":"stale","is_member":true,"is_archived":false,"is_private":true})).unwrap();
        assert!(w.finish_channel_discovery(&id, ticket, Some(info)));
        let channel = &w.channels[&id];
        assert_eq!(channel.name.as_deref(), Some("renamed"));
        assert!(channel.is_archived);
        assert_eq!(channel.extra["is_member"], false);
        assert!(
            !w.apply_channel_event(&joined),
            "replayed join cannot undo leave"
        );
    }

    #[test]
    fn discovery_ignores_other_members_and_retries_failures_without_accepting_old_reads() {
        let mut w = workspace();
        for raw in [
            json!({"type":"member_joined_channel","channel":"C_NEW","user":"OTHER","event_ts":"50.1"}),
            json!({"type":"channel_created","channel":{"id":"C_NEW"}}),
            json!({"type":"group_joined","channel":{"id":""}}),
        ] {
            assert!(w.begin_channel_discovery(&event(raw)).is_none());
        }
        let joined = event(json!({"type":"group_joined","channel":{"id":"C_NEW"}}));
        let (id, old) = w.begin_channel_discovery(&joined).unwrap();
        w.reset_channel_discovery();
        let (_, newer) = w.begin_channel_discovery(&joined).unwrap();
        let info = || {
            Some(Channel {
                id: id.clone(),
                name: Some("private-new".into()),
                ..Default::default()
            })
        };
        assert!(!w.finish_channel_discovery(&id, old, info()));
        assert!(
            w.begin_channel_discovery(&joined).is_none(),
            "old response cannot clear newer request"
        );
        assert!(!w.finish_channel_discovery(&id, newer, None));
        let (_, retry) = w.begin_channel_discovery(&joined).unwrap();
        assert!(!w.finish_channel_discovery(
            &id,
            retry,
            Some(Channel {
                id: "WRONG".into(),
                ..Default::default()
            })
        ));
        assert!(!w.channels.contains_key(&id));
        assert!(w.begin_channel_discovery(&joined).is_some());
    }

    #[test]
    fn malformed_or_unknown_updates_cannot_poison_clocks_or_invent_membership() {
        let mut w = workspace();
        assert!(!w.apply_channel_event(&rename(Value::Null, "90.1")));
        assert!(!w.apply_channel_event(&rename(json!(""), "91.1")));
        assert!(!w.apply_channel_event(&rename(json!("invalid-stamp"), "bad")));
        assert!(w.apply_channel_event(&rename(json!("valid"), "30.1")));
        assert!(!w.apply_channel_event(&event(json!({"type":"group_rename","channel":{"id":"C_OTHER","name":"unknown"},"event_ts":"40.1"}))));
        assert_eq!(w.channels.len(), 1);
        assert!(!w.apply_channel_event(&message("purpose", json!({"value":"partial"}), "99.1")));
        assert!(w.apply_channel_event(&message("purpose", json!("description"), "31.1")));
        assert!(w.apply_channel_event(&message("purpose", json!(""), "32.1")));
        assert_eq!(ChannelTextField::Description.value(&w.channels["C1"]), "");
    }
    #[test]
    fn duplicate_archive_forms_order_together_and_unarchive_wins_over_old_frames() {
        let mut w = workspace();
        assert!(w.apply_channel_event(&event(
            json!({"type":"group_archive","channel":"C1","event_ts":"40.1"})
        )));
        assert!(w.channels["C1"].is_archived);
        assert!(w.apply_channel_event(&event(
            json!({"type":"message","subtype":"channel_unarchive","channel":"C1","ts":"41.1"})
        )));
        assert!(!w.apply_channel_event(&event(
            json!({"type":"group_archive","channel":"C1","event_ts":"40.9"})
        )));
        assert!(!w.channels["C1"].is_archived);
    }
    #[test]
    fn in_flight_snapshots_keep_newer_events_but_later_authoritative_reads_can_reconcile() {
        let mut w = workspace();
        let started = w.channel_lifecycle.revision();
        let old = w.channels["C1"].clone();
        w.apply_channel_event(&rename(json!("from-socket"), "50.1"));
        w.apply_channel_event(&message("topic", json!("remote topic"), "50.2"));
        let effective = w.merge_channel_details(old.clone(), started);
        assert_eq!(effective.name.as_deref(), Some("from-socket"));
        assert_eq!(ChannelTextField::Topic.value(&effective), "remote topic");
        let page: GenericInfoPage = serde_json::from_value(json!({"channels":[old]})).unwrap();
        w.apply_channel_refresh_since(page, started);
        assert_eq!(w.channels["C1"].name.as_deref(), Some("from-socket"));
        let later = w.channel_lifecycle.revision();
        let mut authoritative = w.channels["C1"].clone();
        authoritative.name = Some("missed-during-outage".into());
        w.merge_channel_details(authoritative, later);
        assert_eq!(
            w.channels["C1"].name.as_deref(),
            Some("missed-during-outage")
        );
    }
    #[test]
    fn membership_events_only_change_self_and_ignore_old_or_unrelated_frames() {
        let mut w = workspace();
        let membership = |user, kind, stamp| {
            event(json!({"type":kind,"user":user,"channel":"C1","event_ts":stamp}))
        };
        assert!(!w.apply_channel_event(&membership("OTHER", "member_left_channel", "90.1")));
        assert_eq!(w.channel_lifecycle.members_revision("C1"), 1);
        assert!(w.apply_channel_event(&membership("U1", "member_joined_channel", "20.1")));
        assert!(w.apply_channel_event(&membership("U1", "member_left_channel", "21.1")));
        assert!(!w.apply_channel_event(&membership("U1", "member_joined_channel", "20.9")));
        assert_eq!(w.channels["C1"].extra["is_member"], false);
        assert!(w.apply_channel_event(&event(
            json!({"type":"group_joined","channel":{"id":"C1"},"event_ts":"22.1"})
        )));
        assert!(w.apply_channel_event(&event(json!({"type":"message","subtype":"channel_leave","channel":"C1","user":"U1","ts":"23.1"}))));
        assert!(!w.apply_channel_event(&event(json!({"type":"message","subtype":"channel_join","channel":"C1","user":"OTHER","ts":"25.1"}))));
        assert_eq!(w.channels["C1"].extra["is_member"], false);
        assert!(w.channels["C1"].is_private);
    }

    #[test]
    fn confirmed_membership_protects_pending_counts_and_details_but_respects_newer_socket_events() {
        let mut w = workspace();
        let before = w.channel_lifecycle.revision();
        w.confirm_channel_membership("C1", false, before);
        let mut stale = w.channels["C1"].clone();
        stale.extra.insert("is_member".into(), true.into());
        w.merge_channel_details(stale, before);
        let counts = || serde_json::from_value(json!({"channels":[{"id":"C1"}]})).unwrap();
        w.apply_counts_since(counts(), before);
        assert_eq!(w.channels["C1"].extra["is_member"], false);
        let pending = w.channel_lifecycle.revision();
        assert!(w.apply_channel_event(&event(
            json!({"type":"member_joined_channel","user":"U1","channel":"C1","event_ts":"40.1"})
        )));
        w.confirm_channel_membership("C1", false, pending);
        assert_eq!(w.channels["C1"].extra["is_member"], true);
        let later = w.channel_lifecycle.revision();
        w.confirm_channel_membership("C1", false, later);
        w.apply_counts(counts());
        assert_eq!(w.channels["C1"].extra["is_member"], true);
    }
}
