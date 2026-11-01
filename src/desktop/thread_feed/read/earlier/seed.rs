//! Data validation for this independent original-packet full-body reader.
use super::*;
pub(super) fn timestamp(v: &str) -> bool {
    let Some((a, b)) = v.split_once('.') else {
        return false;
    };
    a.len() == 10
        && b.len() == 6
        && a.bytes().chain(b.bytes()).all(|b| b.is_ascii_digit())
        && a.parse::<u64>().is_ok_and(|n| n > 0)
}
pub(super) fn ordinary(m: &Message, team: &str, key: &Key, root: bool) -> bool {
    m.kind.as_deref() == Some("message")
        && m.subtype.is_none()
        && m.message.is_none()
        && m.team.as_deref().is_none_or(|t| t == team)
        && m.channel.as_deref().is_none_or(|c| c == key.0)
        && m.thread_ts.as_deref().is_none_or(|r| r == key.1)
        && m.ts
            .as_deref()
            .is_some_and(|t| timestamp(t) && if root { t == key.1 } else { t > key.1.as_str() })
        && !super_platinum_core::channel_threads::hidden(m)
        && ![
            "deleted",
            "is_deleted",
            "limited",
            "is_limited",
            "hidden",
            "is_ephemeral",
            "is_tombstone",
        ]
        .iter()
        .any(|k| {
            m.extra
                .get(*k)
                .is_some_and(|v| v != &Value::Bool(false) && v != &Value::Null)
        })
}
#[derive(Clone)]
pub(super) struct Seed {
    pub(super) ids: Vec<String>,
    pub(super) count: u32,
    pub(super) latest: String,
}
impl Seed {
    fn capture(i: &ThreadViewItem, team: &str) -> Option<(Key, Self)> {
        let key = (i.root_msg.channel.clone()?, i.root_msg.ts.clone()?);
        if key.0.is_empty()
            || key.0.len() > 128
            || !ordinary(&i.root_msg, team, &key, true)
            || i.root_msg.thread_ts.as_ref() != Some(&key.1)
            || i.root_msg.extra.get("subscribed") != Some(&Value::Bool(true))
            || !i.unread_replies.is_empty()
            || i.latest_replies.is_empty()
            || i.latest_replies.len() > 100
        {
            return None;
        }
        let mut ids = Vec::new();
        for m in &i.latest_replies {
            if !ordinary(m, team, &key, false) {
                return None;
            }
            ids.push(m.ts.clone()?);
        }
        ids.sort();
        if ids.windows(2).any(|v| v[0] >= v[1]) {
            return None;
        }
        let count = i.root_msg.reply_count?;
        let latest = i.root_msg.latest_reply.clone()?;
        if count as usize <= ids.len() || ids.last() != Some(&latest) {
            return None;
        }
        Some((key, Self { ids, count, latest }))
    }
    pub(super) fn root_matches(&self, i: &ThreadViewItem, team: &str, key: &Key) -> bool {
        ordinary(&i.root_msg, team, key, true)
            && i.root_msg.channel.as_ref() == Some(&key.0)
            && i.root_msg.thread_ts.as_ref() == Some(&key.1)
            && i.root_msg.extra.get("subscribed") == Some(&Value::Bool(true))
            && i.root_msg.reply_count == Some(self.count)
            && i.root_msg.latest_reply.as_ref() == Some(&self.latest)
    }
}
pub(super) fn item<'a>(s: &'a ShellState, team: &str, key: &Key) -> Option<&'a ThreadViewItem> {
    let mut items = s.core.threads_view.items.iter().filter(|i| {
        i.root_msg.channel.as_ref() == Some(&key.0) && i.root_msg.ts.as_ref() == Some(&key.1)
    });
    let i = items.next()?;
    if items.next().is_some() || i.latest_replies.len() + i.unread_replies.len() > 1000 {
        return None;
    }
    let k = (team.into(), key.0.clone(), key.1.clone());
    if s.core
        .thread_subscriptions
        .pending(s.core.account_epoch, &k)
        || s.core.thread_subscriptions.active(s.core.account_epoch, &k) == Some(false)
        || deleted(s, team, key, &key.1)
    {
        return None;
    }
    Some(i)
}
pub(super) fn deleted(s: &ShellState, team: &str, key: &Key, ts: &str) -> bool {
    s.core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(&key.0))
        .is_some_and(|b| b.was_deleted(ts))
        || s.core
            .threads
            .get(&(team.into(), key.0.clone(), key.1.clone()))
            .is_some_and(|b| b.was_deleted(ts))
}
pub(super) fn ids(i: &ThreadViewItem) -> Option<Vec<String>> {
    let mut ids = i
        .latest_replies
        .iter()
        .chain(&i.unread_replies)
        .map(|m| m.ts.clone())
        .collect::<Option<Vec<_>>>()?;
    ids.sort();
    if ids.windows(2).any(|v| v[0] >= v[1]) {
        return None;
    }
    Some(ids)
}
pub(in crate::thread_feed::read) struct Candidates(pub(super) BTreeMap<Key, Option<Seed>>);
impl Candidates {
    pub(in crate::thread_feed::read) fn from_page(page: &ThreadsViewPage, team: &str) -> Self {
        let mut rows = BTreeMap::new();
        for i in &page.threads {
            let (Some(c), Some(r)) = (&i.root_msg.channel, &i.root_msg.ts) else {
                continue;
            };
            let seed = Seed::capture(i, team).map(|(_, v)| v);
            rows.entry((c.clone(), r.clone()))
                .and_modify(|v| *v = None)
                .or_insert(seed);
        }
        Self(rows)
    }
}
