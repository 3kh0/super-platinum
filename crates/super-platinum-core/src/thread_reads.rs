//! Thread read cursors never alter a conversation's read state.
use crate::{CoreAppState, domain::ThreadKey, state::cmp_ts};

/// Slack's `read=0` makes the selected reply unread by subtracting one
/// microsecond. Integer arithmetic avoids rounding current epoch timestamps.
pub fn before(ts: &str) -> Option<String> {
    let (seconds, fraction) = ts.split_once('.')?;
    if fraction.is_empty() || fraction.len() > 6 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let micros = fraction
        .parse::<u64>()
        .ok()?
        .checked_mul(10u64.pow(6 - fraction.len() as u32))?;
    let value = seconds
        .parse::<u64>()
        .ok()?
        .checked_mul(1_000_000)?
        .checked_add(micros)?
        .checked_sub(1)?;
    Some(format!("{}.{:06}", value / 1_000_000, value % 1_000_000))
}

impl crate::state::ChannelMessages {
    /// Recount after paging or a reply arrival without changing read ordering.
    pub fn recount_thread_unreads(&mut self, root: &str) {
        let Some(cursor) = self.last_read.as_deref() else {
            return;
        };
        self.unread_count = self
            .messages
            .iter()
            .filter(|message| {
                message.ts.as_deref().is_some_and(|ts| {
                    ts != root && !self.is_pending(ts) && cmp_ts(Some(ts), Some(cursor)).is_gt()
                })
            })
            .count()
            .min(u32::MAX as usize) as u32;
    }
}

impl CoreAppState {
    pub fn thread_cursor(&self, key: &ThreadKey) -> Option<&str> {
        self.threads
            .get(key)
            .and_then(|bag| bag.last_read.as_deref())
    }

    pub fn thread_read_revision(&self, key: &ThreadKey) -> u64 {
        self.threads.get(key).map_or(0, |bag| bag.read_revision)
    }

    pub fn apply_thread_mark(&mut self, key: &ThreadKey, cursor: &str) {
        let bag = self.threads.entry(key.clone()).or_default();
        bag.last_read = Some(cursor.into());
        bag.read_revision = bag.read_revision.wrapping_add(1);
        bag.recount_thread_unreads(&key.2);
        if self.active_team.as_ref() == Some(&key.0) {
            for item in &mut self.threads_view.items {
                if item.channel() == Some(&key.1) && item.root_ts() == Some(&key.2) {
                    let replies = item.replies().into_iter().cloned().collect::<Vec<_>>();
                    item.unread_replies = replies
                        .iter()
                        .filter(|m| cmp_ts(m.ts.as_deref(), Some(cursor)).is_gt())
                        .cloned()
                        .collect();
                    item.latest_replies = replies;
                    item.root_msg
                        .extra
                        .insert("last_read".into(), cursor.into());
                }
            }
        }
    }

    /// Merge a feed response without discarding a mark delivered while its
    /// request was in flight. The feed only belongs to the active workspace.
    pub fn merge_thread_view_item(
        &mut self,
        team: &str,
        item: crate::slack::models::ThreadViewItem,
        revision: u64,
    ) {
        if self.active_team.as_deref() != Some(team) {
            return;
        }
        let Some((channel, root)) = item.channel().cloned().zip(item.root_ts().cloned()) else {
            return;
        };
        let key = (team.into(), channel, root);
        let current = self.thread_cursor(&key).map(str::to_owned);
        let cursor = if self.thread_read_revision(&key) != revision {
            current
        } else {
            item.root_msg
                .extra
                .get("last_read")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        self.threads_view.upsert(item);
        if let Some(cursor) = cursor {
            self.apply_thread_mark(&key, &cursor);
        }
    }

    /// A realtime update outranks the older HTTP acknowledgement. An exact
    /// match is already confirmed and retains authoritative server state.
    pub fn confirm_thread_mark(&mut self, key: &ThreadKey, cursor: &str, revision: u64) -> bool {
        if self.thread_cursor(key) == Some(cursor) {
            return true;
        }
        if self.thread_read_revision(key) != revision {
            return false;
        }
        self.apply_thread_mark(key, cursor);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unread_cursor_is_exact_across_second_boundaries() {
        assert_eq!(
            before("1791795731.106029").as_deref(),
            Some("1791795731.106028")
        );
        assert_eq!(before("100.000000").as_deref(), Some("99.999999"));
        assert_eq!(before("100.1").as_deref(), Some("100.099999"));
        for invalid in ["0.0", "100", "100.bad", "100.1234567"] {
            assert!(before(invalid).is_none());
        }
    }
    #[test]
    fn marks_are_thread_scoped_and_delayed_acks_do_not_overwrite_socket_state() {
        let mut core = CoreAppState::new(Default::default());
        let key = ("T".into(), "C".into(), "100.0".into());
        let other = ("T".into(), "C".into(), "200.0".into());
        let bag = core.threads.entry(key.clone()).or_default();
        for ts in ["100.0", "101.0", "102.0"] {
            bag.upsert(crate::slack::models::Message {
                ts: Some(ts.into()),
                ..Default::default()
            });
        }
        core.apply_thread_mark(&key, "100.999999");
        assert_eq!(core.threads[&key].unread_count, 2);
        assert!(core.thread_cursor(&other).is_none());
        assert!(!core.confirm_thread_mark(&key, "102.0", 0));
        assert_eq!(core.thread_cursor(&key), Some("100.999999"));
        assert!(core.confirm_thread_mark(&key, "101.0", 1));
        assert_eq!(core.threads[&key].unread_count, 1);
        assert!(core.confirm_thread_mark(&key, "102.0", 2));
        assert_eq!(core.threads[&key].unread_count, 0);
    }
    #[test]
    fn feed_response_preserves_intervening_unread_and_latest_reply_copies() {
        let mut core = CoreAppState::new(Default::default());
        core.active_team = Some("T".into());
        let key = ("T".into(), "C".into(), "100.0".into());
        let item: crate::slack::models::ThreadViewItem = serde_json::from_value(serde_json::json!({"root_msg":{"channel":"C","ts":"100.0","last_read":"102.0"},"latest_replies":[{"ts":"101.0"},{"ts":"102.0"}]})).unwrap();
        core.apply_thread_mark(&key, "100.999999");
        core.merge_thread_view_item("T", item.clone(), 0);
        assert_eq!(core.threads_view.items[0].unread_replies.len(), 2);
        assert_eq!(core.threads_view.items[0].replies().len(), 2);
        core.merge_thread_view_item("T", item, core.thread_read_revision(&key));
        assert!(core.threads_view.items[0].unread_replies.is_empty());
        assert_eq!(core.thread_cursor(&key), Some("102.0"));
    }
    #[test]
    fn cursor_returning_to_its_old_value_does_not_revive_a_stale_http_mark() {
        let mut core = CoreAppState::new(Default::default());
        let key = ("T".into(), "C".into(), "100.0".into());
        core.apply_thread_mark(&key, "102.0");
        let revision = core.thread_read_revision(&key);
        core.apply_thread_mark(&key, "101.999999");
        core.apply_thread_mark(&key, "102.0");
        assert!(!core.confirm_thread_mark(&key, "101.999999", revision));
        assert_eq!(core.thread_cursor(&key), Some("102.0"));
    }
}
