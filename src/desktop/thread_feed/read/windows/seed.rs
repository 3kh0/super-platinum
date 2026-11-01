//! Incoming getView descriptor, not warm global history or pane authority.
use super::*;
use std::collections::BTreeMap;
use super_platinum_core::slack::models::{Message, ThreadViewItem};

fn timestamp(v: &str) -> bool {
    let Some((a, b)) = v.split_once('.') else {
        return false;
    };
    a.len() == 10
        && b.len() == 6
        && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
        && a.parse::<u64>().is_ok_and(|n| n > 0)
}
fn context(m: &Message, team: &str, channel: &str, root: &str) -> bool {
    m.kind.as_deref() == Some("message")
        && !super_platinum_core::channel_threads::hidden(m)
        && m.channel.as_deref().is_none_or(|s| s == channel)
        && m.thread_ts.as_deref().is_none_or(|s| s == root)
        && m.team.as_deref().is_none_or(|s| s == team)
}
#[derive(Clone)]
pub(super) struct Seed {
    pub(super) timestamps: Vec<String>,
    pub(super) reached_start: bool,
}
impl Seed {
    fn from_item(item: &ThreadViewItem, team: &str) -> Option<(Key, Self)> {
        let root = &item.root_msg;
        let (channel, ts) = (root.channel.as_deref()?, root.ts.as_deref()?);
        if channel.is_empty()
            || !timestamp(ts)
            || root.thread_ts.as_deref() != Some(ts)
            || !context(root, team, channel, ts)
            || root.extra.get("subscribed") != Some(&serde_json::json!(true))
            || super_platinum_core::channel_threads::hidden(root)
            || item.unread_replies.len() + item.latest_replies.len() > 1000
        {
            return None;
        }
        let count = root.reply_count? as usize;
        // Source sLu1 sorts/dedups unread.concat(latest). Refuse ambiguous
        // duplicate payloads instead of choosing another normalization branch.
        let mut replies = BTreeMap::new();
        for message in item.unread_replies.iter().chain(&item.latest_replies) {
            let rts = message.ts.as_deref()?;
            if !timestamp(rts) || rts <= ts || !context(message, team, channel, ts) {
                return None;
            }
            let raw = serde_json::to_value(message).ok()?;
            if replies
                .insert(rts.to_owned(), raw.clone())
                .is_some_and(|old| old != raw)
            {
                return None;
            }
        }
        if replies.is_empty() {
            return None;
        }
        let timestamps = replies.into_keys().collect::<Vec<_>>();
        // The original Source installer explicitly supplies incoming tail End
        // false in both ordinary branches. Count only chooses its Start branch.
        // This is a new private incoming descriptor, never warm-store equality.
        let reached_start = count <= timestamps.len();
        Some((
            (channel.into(), ts.into()),
            Self {
                timestamps,
                reached_start,
            },
        ))
    }
    pub(super) fn matches(&self, shell: &ShellState, team: &str, key: &Key) -> bool {
        let mut items = shell.core.threads_view.items.iter().filter(|i| {
            i.root_msg.channel.as_ref() == Some(&key.0) && i.root_msg.ts.as_ref() == Some(&key.1)
        });
        let Some(item) = items.next() else {
            return false;
        };
        if items.next().is_some()
            || item.root_msg.thread_ts.as_ref() != Some(&key.1)
            || !context(&item.root_msg, team, &key.0, &key.1)
            || item.root_msg.extra.get("subscribed") != Some(&serde_json::json!(true))
            || item.latest_replies.len() + item.unread_replies.len() > 1000
        {
            return false;
        }
        // Ambiguous duplicate payloads are refused once at original intake.
        // Current membership needs no message serialization in render paths;
        // realtime edits/reactions continue using the live existing vectors.
        let mut seen = HashSet::new();
        for message in item.latest_replies.iter().chain(&item.unread_replies) {
            let Some(ts) = message.ts.as_deref() else {
                return false;
            };
            if !context(message, team, &key.0, &key.1)
                || self
                    .timestamps
                    .binary_search_by(|s| s.as_str().cmp(ts))
                    .is_err()
            {
                return false;
            }
            seen.insert(ts);
        }
        if seen.len() != self.timestamps.len() {
            return false;
        }
        let thread_key = (team.into(), key.0.clone(), key.1.clone());
        if shell
            .core
            .thread_subscriptions
            .pending(shell.core.account_epoch, &thread_key)
            || shell
                .core
                .thread_subscriptions
                .active(shell.core.account_epoch, &thread_key)
                == Some(false)
        {
            return false;
        }
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return false;
        };
        let channel = workspace.messages.get(&key.0);
        let bag = shell.core.threads.get(&thread_key);
        std::iter::once(&key.1).chain(&self.timestamps).all(|ts| {
            !channel.is_some_and(|b| b.was_deleted(ts)) && !bag.is_some_and(|b| b.was_deleted(ts))
        })
    }
}
pub(in crate::thread_feed::read) struct Candidates(pub(super) BTreeMap<Key, Option<Seed>>);
impl Candidates {
    /// Sole production caller receives the original page BEFORE Core mutation.
    pub(in crate::thread_feed::read) fn from_page(page: &ThreadsViewPage, team: &str) -> Self {
        let mut rows = BTreeMap::new();
        for item in &page.threads {
            let (Some(channel), Some(root)) = (&item.root_msg.channel, &item.root_msg.ts) else {
                continue;
            };
            let key = (channel.clone(), root.clone());
            let seed = Seed::from_item(item, team).map(|(_, v)| v);
            rows.entry(key).and_modify(|s| *s = None).or_insert(seed);
        }
        Self(rows)
    }
}
