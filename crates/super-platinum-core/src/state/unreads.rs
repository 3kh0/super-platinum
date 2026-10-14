//! Unread bookkeeping for messages that arrive over the socket.
//!
//! `client.counts` is only fetched at boot, and the sidebar hides read
//! conversations in most sections, so a conversation with new activity has to
//! turn unread here or it never appears.

use crate::slack::models::Message as SlackMessage;
use crate::state::{Workspace, cmp_ts};

/// Subtypes that are someone saying something; joins, topic changes and the
/// like do not make a conversation unread.
const COUNTED_SUBTYPES: &[&str] = &[
    "bot_message",
    "thread_broadcast",
    "file_share",
    "me_message",
];

impl Workspace {
    /// Metadata may provide the fallback cursor before a message bag has one.
    /// Preserve replacement semantics while retaining read acknowledgement order.
    pub fn insert_channel_metadata(&mut self, channel: crate::slack::models::Channel) {
        let id = channel.id.clone();
        let previous = self.read_cursor(&id).map(str::to_owned);
        self.channels.insert(id.clone(), channel);
        if self.read_cursor(&id) != previous.as_deref() {
            let list = self.messages.entry(id).or_default();
            list.read_revision = list.read_revision.wrapping_add(1);
        }
    }

    pub fn read_revision(&self, channel: &str) -> u64 {
        self.messages
            .get(channel)
            .map_or(0, |list| list.read_revision)
    }

    /// Exact socket acknowledgements are already authoritative. Otherwise both
    /// the prior cursor and its revision must still match: a cursor can move
    /// forward and back while an HTTP mark is in flight.
    pub fn accepts_read_ack(
        &self,
        channel: &str,
        cursor: &str,
        previous: Option<&str>,
        revision: u64,
    ) -> bool {
        self.read_cursor(channel) == Some(cursor)
            || (self.read_cursor(channel) == previous && self.read_revision(channel) == revision)
    }

    pub fn read_cursor(&self, channel: &str) -> Option<&str> {
        self.messages
            .get(channel)
            .and_then(|list| list.last_read.as_deref())
            .or_else(|| {
                self.channels
                    .get(channel)
                    .and_then(|row| row.last_read.as_deref())
            })
    }

    /// Apply an HTTP acknowledgement only to the read state it started from.
    /// Socket marks (including manual unread) outrank an older HTTP response.
    /// Returns true only when no newer messages remain to be acknowledged.
    pub fn confirm_read(&mut self, channel: &str, ts: &str, previous: Option<&str>) -> bool {
        if self.read_cursor(channel) != previous || !cmp_ts(Some(ts), previous).is_gt() {
            return false;
        }
        let newer = self.messages.get(channel).is_some_and(|list| {
            list.latest
                .as_deref()
                .into_iter()
                .chain(
                    list.messages
                        .last()
                        .and_then(|message| message.ts.as_deref()),
                )
                .any(|latest| cmp_ts(Some(latest), Some(ts)).is_gt())
        });
        {
            let list = self.messages.entry(channel.to_owned()).or_default();
            list.last_read = Some(ts.to_owned());
            list.read_revision = list.read_revision.wrapping_add(1);
            if !newer {
                list.unread_count = 0;
                list.mention_count = 0;
                list.vip_count = 0;
            }
        }
        if let Some(row) = self.channels.get_mut(channel) {
            row.last_read = Some(ts.to_owned());
            if !newer {
                row.has_unreads = false;
                row.unread_count = Some(0);
                row.unread_count_display = Some(0);
                row.mention_count = Some(0);
            }
        }
        !newer
    }

    /// Fallback after an explicit unread acknowledgement, before Slack supplies
    /// exact counts. Cached history can have gaps, so preserve known counts when
    /// moving backward and keep at least one unread for the selected message.
    pub fn mark_unread_locally(&mut self, channel: &str, cursor: &str) {
        let backward = !cmp_ts(Some(cursor), self.read_cursor(channel)).is_gt();
        let list = self.messages.entry(channel.to_owned()).or_default();
        let cached = list
            .messages
            .iter()
            .filter(|message| {
                cmp_ts(message.ts.as_deref(), Some(cursor)).is_gt()
                    && message
                        .subtype
                        .as_deref()
                        .is_none_or(|kind| COUNTED_SUBTYPES.contains(&kind))
            })
            .count()
            .min(u32::MAX as usize) as u32;
        list.unread_count = cached
            .max(1)
            .max(if backward { list.unread_count } else { 0 });
        list.last_read = Some(cursor.into());
        list.read_revision = list.read_revision.wrapping_add(1);
        if let Some(row) = self.channels.get_mut(channel) {
            row.last_read = Some(cursor.into());
            row.has_unreads = true;
            row.unread_count = Some(list.unread_count);
            row.unread_count_display = Some(list.unread_count);
        }
    }

    /// Count a newly delivered message the way `client.counts` would: unread
    /// for everyone else's message, a mention for every DM message or a ping
    /// of you, and a VIP mention when a VIP pinged you in a channel.
    ///
    /// `viewing` is the conversation on screen, which is read as it arrives.
    /// `group_ping` is a mention of one of your user groups.
    pub fn count_incoming_message(
        &mut self,
        channel: &str,
        message: &SlackMessage,
        viewing: bool,
        group_ping: bool,
    ) {
        let Some(ts) = message.ts.as_deref() else {
            return;
        };
        if viewing
            || message.user.as_deref() == Some(self.self_user_id.as_str())
            || message
                .subtype
                .as_deref()
                .is_some_and(|subtype| !COUNTED_SUBTYPES.contains(&subtype))
        {
            return;
        }
        let (is_dm, channel_read, initial_mentions) = match self.channels.get(channel) {
            Some(c) => (c.is_im || c.is_mpim, c.last_read.clone(), c.mention_count),
            None => (false, None, None),
        };
        let self_ping = mentions_user(message, &self.self_user_id);
        let vip = message
            .user
            .as_deref()
            .is_some_and(|user| self.vip_users.contains(user));
        let cm = self.messages.entry(channel.to_owned()).or_default();
        let held = cm.messages.iter().any(|m| m.ts.as_deref() == Some(ts));
        let unread = cmp_ts(
            Some(ts),
            cm.last_read.as_deref().or(channel_read.as_deref()),
        )
        .is_gt();
        if held || !unread {
            return;
        }
        let pinged = self_ping || group_ping;
        cm.unread_count = cm.unread_count.saturating_add(1);
        if is_dm || pinged {
            cm.mention_count = cm
                .mention_count
                .max(initial_mentions.unwrap_or(0))
                .saturating_add(1);
        }
        if pinged && vip && !is_dm {
            cm.vip_count = cm.vip_count.saturating_add(1);
        }
        if cm
            .latest
            .as_deref()
            .is_none_or(|latest| cmp_ts(Some(ts), Some(latest)).is_gt())
        {
            cm.latest = Some(ts.to_owned());
        }
        let (unread_count, mention_count) = (cm.unread_count, cm.mention_count);
        if let Some(c) = self.channels.get_mut(channel) {
            c.has_unreads = true;
            c.unread_count = Some(unread_count);
            c.unread_count_display = Some(unread_count);
            c.mention_count = Some(mention_count);
        }
    }
}

/// `<@U123>`, `<@U123|name>`, or a broadcast (`<!channel>`, `<!here>`,
/// `<!everyone>`), which Slack counts as a mention of everyone in the room.
fn mentions_user(message: &SlackMessage, user: &str) -> bool {
    let Some(text) = message.text.as_deref() else {
        return false;
    };
    let direct = text.split("<@").skip(1).any(|tail| {
        tail.split_once('>')
            .is_some_and(|(token, _)| token.split('|').next() == Some(user))
    });
    direct
        || ["<!channel", "<!here", "<!everyone"]
            .iter()
            .any(|broadcast| text.contains(broadcast))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack::models::Channel;

    fn workspace() -> Workspace {
        let mut ws = Workspace::from_session(&crate::config::WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U_SELF".into(),
            name: "Test".into(),
            url: "https://t".into(),
            token: "xoxc".into(),
        });
        ws.vip_users.insert("U_VIP".into());
        for (id, is_im) in [("C1", false), ("D1", true)] {
            ws.channels.insert(
                id.into(),
                Channel {
                    id: id.into(),
                    is_im,
                    is_channel: !is_im,
                    last_read: Some("100.000000".into()),
                    ..Default::default()
                },
            );
        }
        ws
    }

    fn message(ts: &str, user: &str, text: &str) -> SlackMessage {
        SlackMessage {
            ts: Some(ts.into()),
            user: Some(user.into()),
            text: Some(text.into()),
            ..Default::default()
        }
    }

    #[test]
    fn a_new_channel_message_turns_the_channel_unread() {
        let mut ws = workspace();
        ws.count_incoming_message("C1", &message("200.0", "U_A", "hi"), false, false);
        let cm = &ws.messages["C1"];
        assert_eq!((cm.unread_count, cm.mention_count, cm.vip_count), (1, 0, 0));
        assert_eq!(cm.latest.as_deref(), Some("200.0"));
        assert!(ws.channels["C1"].has_unreads);
    }

    #[test]
    fn a_vip_ping_in_a_channel_is_a_vip_mention() {
        let mut ws = workspace();
        ws.count_incoming_message(
            "C1",
            &message("200.0", "U_VIP", "<@U_SELF> look"),
            false,
            false,
        );
        ws.count_incoming_message("C1", &message("201.0", "U_VIP", "no ping"), false, false);
        let cm = &ws.messages["C1"];
        assert_eq!((cm.unread_count, cm.mention_count, cm.vip_count), (2, 1, 1));
    }

    #[test]
    fn every_dm_message_is_a_badge() {
        let mut ws = workspace();
        ws.count_incoming_message("D1", &message("200.0", "U_A", "hey"), false, false);
        assert_eq!(ws.messages["D1"].mention_count, 1);
    }

    #[test]
    fn own_old_viewed_and_joins_do_not_count() {
        let mut ws = workspace();
        ws.count_incoming_message("C1", &message("200.0", "U_SELF", "mine"), false, false);
        ws.count_incoming_message("C1", &message("50.0", "U_A", "old"), false, false);
        ws.count_incoming_message("C1", &message("201.0", "U_A", "seen"), true, false);
        let mut join = message("202.0", "U_A", "joined");
        join.subtype = Some("channel_join".into());
        ws.count_incoming_message("C1", &join, false, false);
        assert_eq!(ws.messages.get("C1").map_or(0, |cm| cm.unread_count), 0);
    }

    #[test]
    fn read_ack_preserves_messages_that_arrived_during_the_request() {
        let mut ws = workspace();
        ws.count_incoming_message("C1", &message("200.0", "U_A", "hi"), false, false);
        let before = ws.read_cursor("C1").map(str::to_owned);
        ws.count_incoming_message(
            "C1",
            &message("201.0", "U_VIP", "<@U_SELF> later"),
            false,
            false,
        );
        assert!(!ws.confirm_read("C1", "200.0", before.as_deref()));
        assert_eq!(ws.read_cursor("C1"), Some("200.0"));
        assert!(ws.channels["C1"].has_unreads);
        assert_eq!(ws.messages["C1"].mention_count, 1);
        assert_eq!(ws.messages["C1"].vip_count, 1);
        assert!(ws.confirm_read("C1", "201.0", Some("200.0")));
        assert_eq!(ws.messages["C1"].unread_count, 0);
        assert!(!ws.channels["C1"].has_unreads);
    }

    #[test]
    fn read_ack_does_not_overwrite_a_later_server_mark_or_manual_unread() {
        for current in ["300.0", "50.0"] {
            let mut ws = workspace();
            ws.channels.get_mut("C1").unwrap().last_read = Some(current.into());
            assert!(!ws.confirm_read("C1", "200.0", Some("100.000000")));
            assert_eq!(ws.read_cursor("C1"), Some(current));
        }
    }
    #[test]
    fn read_ack_revision_rejects_cursor_aba_but_accepts_exact_socket_ack() {
        let mut ws = workspace();
        ws.mark_unread_locally("C1", "100.0");
        let revision = ws.read_revision("C1");
        assert!(ws.accepts_read_ack("C1", "200.0", Some("100.0"), revision));
        ws.mark_unread_locally("C1", "300.0");
        ws.mark_unread_locally("C1", "100.0");
        assert!(!ws.accepts_read_ack("C1", "200.0", Some("100.0"), revision));
        assert!(ws.accepts_read_ack("C1", "100.0", Some("300.0"), revision));
        let current = ws.read_revision("C1");
        ws.confirm_read("C1", "200.0", Some("100.0"));
        assert_ne!(ws.read_revision("C1"), current);
    }

    #[test]
    fn metadata_cursor_replacement_records_aba_without_changing_bag_precedence() {
        let mut ws = workspace();
        let revision = ws.read_revision("C1");
        let mut metadata_only = ws.channels["C1"].clone();
        metadata_only.name = Some("renamed".into());
        ws.insert_channel_metadata(metadata_only);
        assert_eq!(ws.read_revision("C1"), revision);
        for cursor in ["300.0", "100.000000"] {
            let mut channel = ws.channels["C1"].clone();
            channel.last_read = Some(cursor.into());
            ws.insert_channel_metadata(channel);
        }
        assert!(!ws.accepts_read_ack("C1", "200.0", Some("100.000000"), revision));
        ws.mark_unread_locally("C1", "50.0");
        let revision = ws.read_revision("C1");
        let mut channel = ws.channels["C1"].clone();
        channel.last_read = Some("500.0".into());
        ws.insert_channel_metadata(channel);
        assert_eq!(ws.read_cursor("C1"), Some("50.0"));
        assert_eq!(ws.read_revision("C1"), revision);
    }

    #[test]
    fn explicit_unread_includes_own_messages_and_preserves_known_badges() {
        let mut ws = workspace();
        let list = ws.messages.entry("C1".into()).or_default();
        list.messages = vec![
            message("200.0", "U_SELF", "mine"),
            message("201.0", "U_A", "later"),
        ];
        list.unread_count = 5; // Cached history is incomplete.
        list.mention_count = 2;
        list.vip_count = 1;
        ws.mark_unread_locally("C1", "50.0");
        assert_eq!(ws.read_cursor("C1"), Some("50.0"));
        assert_eq!(
            (
                ws.messages["C1"].unread_count,
                ws.messages["C1"].mention_count,
                ws.messages["C1"].vip_count
            ),
            (5, 2, 1)
        );
        assert!(ws.channels["C1"].has_unreads);
        ws.mark_unread_locally("C1", "200.0");
        assert_eq!(ws.messages["C1"].unread_count, 1);
    }

    #[test]
    fn explicit_unread_survives_a_system_message_or_empty_cache() {
        let mut ws = workspace();
        ws.mark_unread_locally("C1", "0.000000");
        assert_eq!(ws.messages["C1"].unread_count, 1);
        assert_eq!(ws.channels["C1"].last_read.as_deref(), Some("0.000000"));
        assert!(ws.confirm_read("C1", "200.0", Some("0.000000")));
        assert!(!ws.channels["C1"].has_unreads);
    }
}
