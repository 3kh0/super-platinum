//! Keeping conversation metadata as fresh as Slack's own client keeps it.
//!
//! The warm cache remembers every conversation it ever saw, and `userBoot`
//! only re-sends some of them, so without this a channel you left, renamed,
//! archived, or that went dormant keeps its old shape forever. `client.counts`
//! says which conversations you are in; `conversations.genericInfo` returns
//! fresh objects for the ones whose `updated` stamp moved.

use std::collections::{BTreeMap, HashSet};

use crate::slack::models::{Channel, ChannelId, CountsPage, GenericInfoPage};
use crate::state::Workspace;

/// `conversations.genericInfo` rejects 500 ids with `too_many_channels`.
pub const GENERIC_INFO_BATCH: usize = 250;

impl Workspace {
    /// `client.counts` lists every conversation you are in. A channel missing
    /// from it is one you left; a DM missing from it is closed.
    pub(crate) fn apply_counts_membership(&mut self, counts: &CountsPage) {
        if counts.channels.is_empty() {
            return;
        }
        let present: HashSet<&str> = counts
            .channels
            .iter()
            .chain(&counts.groups)
            .chain(&counts.mpims)
            .chain(&counts.ims)
            .map(|c| c.id.as_str())
            .collect();
        for channel in self.channels.values_mut() {
            let here = present.contains(channel.id.as_str());
            if !here {
                // Its unread state is whatever it was when we last saw it.
                channel.has_unreads = false;
                channel.unread_count = Some(0);
                channel.unread_count_display = Some(0);
                channel.mention_count = Some(0);
                if let Some(cm) = self.messages.get_mut(&channel.id) {
                    cm.unread_count = 0;
                    cm.mention_count = 0;
                    cm.vip_count = 0;
                }
            }
            if channel.is_im || channel.is_mpim {
                if !here {
                    channel.extra.insert("is_open".into(), false.into());
                }
            } else {
                channel.extra.insert("is_member".into(), here.into());
            }
        }
    }

    /// `updated` stamps for every channel and group DM you are in, batched for
    /// `conversations.genericInfo`. A channel known only by id sends 0, so it
    /// always comes back whole.
    pub fn channels_to_refresh(&self) -> Vec<BTreeMap<ChannelId, u64>> {
        let ids: Vec<(ChannelId, u64)> = self
            .channels
            .values()
            .filter(|c| !c.is_im && !c.is_archived)
            .filter(|c| {
                c.extra
                    .get("is_member")
                    .and_then(serde_json::Value::as_bool)
                    != Some(false)
            })
            .map(|c| {
                let known = c.name.as_deref().is_some_and(|name| !name.is_empty());
                (c.id.clone(), if known { c.updated.unwrap_or(0) } else { 0 })
            })
            .collect();
        ids.chunks(GENERIC_INFO_BATCH)
            .map(|chunk| chunk.iter().cloned().collect())
            .collect()
    }

    /// Take the fresh objects wholesale, keeping only what they never carry:
    /// read state from `client.counts`, the star, and Slack Connect teams.
    pub fn apply_channel_refresh(&mut self, page: GenericInfoPage) {
        for fresh in page.channels {
            let merged = match self.channels.remove(&fresh.id) {
                Some(old) => keep_local_state(fresh, old),
                None => fresh,
            };
            self.channels.insert(merged.id.clone(), merged);
        }
    }
}

fn keep_local_state(mut fresh: Channel, old: Channel) -> Channel {
    fresh.is_starred |= old.is_starred;
    fresh.unread_count = fresh.unread_count.or(old.unread_count);
    fresh.unread_count_display = fresh.unread_count_display.or(old.unread_count_display);
    fresh.mention_count = fresh.mention_count.or(old.mention_count);
    fresh.has_unreads |= old.has_unreads;
    fresh.last_read = fresh.last_read.or(old.last_read);
    if fresh.connected_teams.is_empty() {
        fresh.connected_teams = old.connected_teams;
    }
    for key in ["is_member", "is_open"] {
        if !fresh.extra.contains_key(key)
            && let Some(value) = old.extra.get(key)
        {
            fresh.extra.insert(key.into(), value.clone());
        }
    }
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workspace() -> Workspace {
        Workspace::from_session(&crate::config::WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U_SELF".into(),
            name: "Test".into(),
            url: "https://t".into(),
            token: "xoxc".into(),
        })
    }

    fn channel(value: serde_json::Value) -> Channel {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn counts_mark_left_channels_and_closed_dms() {
        let mut ws = workspace();
        for c in [
            json!({"id":"C_IN","name":"in","is_channel":true}),
            json!({"id":"C_LEFT","name":"left","is_channel":true}),
            json!({"id":"D_GONE","is_im":true,"is_open":true,"user":"U1"}),
        ] {
            let c = channel(c);
            ws.channels.insert(c.id.clone(), c);
        }
        ws.apply_counts(
            serde_json::from_value(json!({"channels":[{"id":"C_IN","has_unreads":false}]}))
                .unwrap(),
        );
        assert_eq!(ws.channels["C_IN"].extra["is_member"], json!(true));
        assert_eq!(ws.channels["C_LEFT"].extra["is_member"], json!(false));
        assert_eq!(ws.channels["D_GONE"].extra["is_open"], json!(false));
        let batches = ws.channels_to_refresh();
        assert_eq!(batches.len(), 1);
        assert!(batches[0].contains_key("C_IN") && !batches[0].contains_key("C_LEFT"));
    }

    #[test]
    fn a_refresh_replaces_metadata_but_keeps_read_state() {
        let mut ws = workspace();
        let old = channel(json!({
            "id":"C1","name":"death","is_channel":true,"is_starred":true,
            "mention_count":2,"has_unreads":true,"last_read":"5.0","updated":1,"is_member":true
        }));
        ws.channels.insert("C1".into(), old);
        ws.apply_channel_refresh(GenericInfoPage {
            channels: vec![channel(json!({
                "id":"C1","name":"death-zzz","is_channel":true,"is_archived":true,
                "updated":9,"properties":{"is_dormant":true}
            }))],
            ..Default::default()
        });
        let c = &ws.channels["C1"];
        assert_eq!(c.name.as_deref(), Some("death-zzz"));
        assert!(c.is_archived && c.is_starred);
        assert_eq!(c.mention_count, Some(2));
        assert_eq!(c.updated, Some(9));
        assert_eq!(c.extra["properties"]["is_dormant"], json!(true));
        assert_eq!(c.extra["is_member"], json!(true));
    }

    #[test]
    fn unnamed_channels_ask_for_a_full_object() {
        let mut ws = workspace();
        let c = channel(json!({"id":"C_STUB","updated":1790727499}));
        ws.channels.insert(c.id.clone(), c);
        assert_eq!(ws.channels_to_refresh()[0]["C_STUB"], 0);
    }
}
