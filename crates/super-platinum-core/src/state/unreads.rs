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
}
