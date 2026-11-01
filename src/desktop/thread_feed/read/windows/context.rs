//! Already-visible reading memory, not an original local window, history/IO,
//! profile or action receipt. Only the actual guarded native mutation seam can
//! preserve its visible preimage. Exact Source RT producer parity is unproved.
use super::*;
use super_platinum_core::{
    domain::ThreadKey,
    slack::{events::RtEvent, models::Message},
    thread_feed::VisibleChange,
};
#[cfg(test)]
mod tests;

#[derive(Clone)]
struct Owner {
    origin: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor: Arc<crate::realtime::activity_counts::CursorOwner>,
    client: SlackClient,
    epoch: u64,
    team: String,
}
impl Owner {
    fn from_window(window: &super::Scope) -> Self {
        Self {
            origin: window.origin.clone(),
            owner: window.owner.clone(),
            cursor: window.cursor_owner.clone(),
            client: window.client.clone(),
            epoch: window.epoch,
            team: window.team.clone(),
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        s.signed_in
            && s.core.account_epoch == self.epoch
            && s.core.active_team.as_ref() == Some(&self.team)
            && Arc::ptr_eq(&self.origin, &s.thread_feed_reads.incarnation)
            && self.owner.current(&s.core)
            && self.cursor.current(&s.core)
            && self.client.same_configuration(&s.core.client)
            && s.core.threads_view.matches_scope(self.epoch, &self.team)
            && s.core.threads_view.loaded
            && s.thread_feed_reads.owned_sequence == Some(s.core.threads_view.load_seq)
    }
}
struct Reading {
    lower: String,
    end: String,
    ids: Vec<String>,
    retired: AtomicBool,
}
#[derive(Clone)]
pub(in crate::thread_feed::read) struct Scope {
    owner: Owner,
    windows: BTreeMap<Key, Arc<Reading>>,
    retired: Arc<AtomicBool>,
}
fn timestamp(v: &str) -> bool {
    let Some((a, b)) = v.split_once('.') else {
        return false;
    };
    a.len() == 10
        && b.len() == 6
        && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
        && a.parse::<u64>().is_ok_and(|n| n > 0)
}
fn item<'a>(s: &'a ShellState, team: &str, key: &Key) -> Option<&'a ThreadViewItem> {
    let mut rows = s.core.threads_view.items.iter().filter(|i| {
        i.root_msg.channel.as_ref() == Some(&key.0) && i.root_msg.ts.as_ref() == Some(&key.1)
    });
    let i = rows.next()?;
    if rows.next().is_some()
        || i.root_msg.thread_ts.as_ref() != Some(&key.1)
        || i.root_msg.kind.as_deref() != Some("message")
        || super_platinum_core::channel_threads::hidden(&i.root_msg)
        || i.root_msg.team.as_deref().is_some_and(|v| v != team)
        || i.root_msg.extra.get("subscribed") != Some(&serde_json::json!(true))
        || i.latest_replies.len() + i.unread_replies.len() > 1000
    {
        return None;
    }
    let thread = (team.into(), key.0.clone(), key.1.clone());
    if s.core
        .thread_subscriptions
        .pending(s.core.account_epoch, &thread)
        || s.core
            .thread_subscriptions
            .active(s.core.account_epoch, &thread)
            == Some(false)
    {
        return None;
    }
    let channel = s.core.workspaces.get(team)?.messages.get(&key.0);
    let bag = s.core.threads.get(&thread);
    if channel.is_some_and(|b| b.was_deleted(&key.1)) || bag.is_some_and(|b| b.was_deleted(&key.1))
    {
        return None;
    }
    Some(i)
}
fn rows<'a>(s: &'a ShellState, team: &str, key: &Key) -> Option<Vec<&'a Message>> {
    let i = item(s, team, key)?;
    Some(crate::view::eligible_thread_replies(s, i))
}
fn ids(rows: &[&Message]) -> Option<Vec<String>> {
    rows.iter()
        .map(|m| m.ts.as_ref().filter(|t| timestamp(t)).cloned())
        .collect()
}
impl Scope {
    fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let ok = self.owner.current(s)
            && s.thread_feed_reads
                .context
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), self));
        if !ok {
            self.retired.store(true, Ordering::Release);
        }
        ok
    }
    fn window_current(&self, s: &ShellState, key: &Key) -> bool {
        if !self.current(s) {
            return false;
        }
        let Some(w) = self.windows.get(key) else {
            return false;
        };
        if w.retired.load(Ordering::Acquire) {
            return false;
        }
        let ok = rows(s, &self.owner.team, key)
            .and_then(|r| {
                let r = r
                    .into_iter()
                    .filter(|m| m.ts.as_deref().is_some_and(|t| t >= w.lower.as_str()))
                    .collect::<Vec<_>>();
                if r.iter().any(|m| m.kind.as_deref() != Some("message")) {
                    return None;
                }
                ids(&r)
            })
            .is_some_and(|actual| actual == w.ids);
        if !ok {
            w.retired.store(true, Ordering::Release);
        }
        ok
    }
}
pub(in crate::thread_feed::read) fn visible_limit(
    s: &ShellState,
    i: &ThreadViewItem,
) -> Option<usize> {
    if s.main_view != MainView::Threads {
        return None;
    }
    let key = super::key(i)?;
    let scope = s.thread_feed_reads.context.as_ref()?;
    scope
        .window_current(s, &key)
        .then(|| scope.windows[&key].ids.len())
}
pub(in crate::thread_feed::read) fn retire_missing(s: &ShellState) {
    if let Some(scope) = s.thread_feed_reads.context.as_ref() {
        for key in scope.windows.keys() {
            scope.window_current(s, key);
        }
    }
}
pub(in crate::thread_feed::read) fn page(s: &mut ShellState, touched: &[ThreadKey], append: bool) {
    if !append {
        s.thread_feed_reads.context = None;
        return;
    }
    let Some(old) = s.thread_feed_reads.context.as_ref().cloned() else {
        return;
    };
    if !old.current(s) {
        s.thread_feed_reads.context = None;
        return;
    }
    let mut next = old.as_ref().clone();
    next.windows.retain(|key, _| {
        !touched
            .iter()
            .any(|(t, c, r)| t == &old.owner.team && c == &key.0 && r == &key.1)
            && old.window_current(s, key)
    });
    next.retired = Arc::new(AtomicBool::new(false));
    s.thread_feed_reads.context = (!next.windows.is_empty()).then(|| Arc::new(next));
}

enum Before {
    Original(Arc<super::Scope>, usize),
    Preserved(Arc<Scope>, Arc<Reading>),
}
/// Sealed native preimage. No Clone/Debug/serde or public constructor.
pub(crate) struct Prepared {
    owner: Owner,
    key: Key,
    generation: u64,
    before: Before,
    slot: Option<Arc<Scope>>,
    previous: Vec<Message>,
    lower: String,
    end: String,
    change: VisibleChange,
}
pub(crate) fn prepare(
    s: &ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<Prepared> {
    if s.core.active_team.as_deref() != Some(team)
        || s.core.workspaces.get(team)?.rt_generation != generation
    {
        return None;
    }
    let (key, change) = match event {
        RtEvent::Message(m) => {
            if m.kind.as_deref() != Some("message")
                || m.subtype.is_some()
                || super_platinum_core::channel_threads::hidden(m)
                || m.team.as_deref().is_some_and(|v| v != team)
            {
                return None;
            }
            let key = (m.channel.clone()?, m.thread_ts.clone()?);
            let ts = m.ts.clone()?;
            if !timestamp(&ts) || ts <= key.1 {
                return None;
            }
            (key, VisibleChange::Append(ts))
        }
        RtEvent::MessageDeleted {
            channel,
            deleted_ts,
        } => {
            if !timestamp(deleted_ts) {
                return None;
            }
            let mut keys = s
                .core
                .threads_view
                .items
                .iter()
                .filter(|i| {
                    i.root_msg.channel.as_ref() == Some(channel)
                        && i.replies()
                            .iter()
                            .any(|m| m.ts.as_ref() == Some(deleted_ts))
                })
                .filter_map(super::key);
            let key = keys.next()?;
            if keys.next().is_some() || deleted_ts == &key.1 {
                return None;
            }
            (key, VisibleChange::Remove(deleted_ts.clone()))
        }
        _ => return None,
    };
    let slot = s.thread_feed_reads.context.as_ref().cloned();
    let (owner, before, previous, lower, end) =
        if let Some(scope) = slot.as_ref().filter(|c| c.window_current(s, &key)) {
            let w = scope.windows.get(&key)?.clone();
            let visible = rows(s, team, &key)?
                .into_iter()
                .filter(|m| m.ts.as_deref().is_some_and(|t| t >= w.lower.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            (
                scope.owner.clone(),
                Before::Preserved(scope.clone(), w.clone()),
                visible,
                w.lower.clone(),
                w.end.clone(),
            )
        } else {
            let scope = s.thread_feed_reads.windows.as_ref()?.clone();
            if !scope.window_current(s, &key) {
                return None;
            }
            let visible = scope.windows.get(&key)?.visible;
            if visible <= INITIAL || visible > MAXIMUM {
                return None;
            }
            let i = item(s, team, &key)?;
            // The owned expanded surface remembers its budget while another
            // rail is selected. This is its private earlier budget, never a
            // public count or the other surface's default selection budget.
            let mut r = crate::view::eligible_thread_replies(s, i);
            if r.len() > visible {
                r.drain(..r.len() - visible);
            }
            let ids = ids(&r)?;
            let lower = ids.first()?.clone();
            let end = ids.last()?.clone();
            (
                Owner::from_window(&scope),
                Before::Original(scope, visible),
                r.into_iter().cloned().collect(),
                lower,
                end,
            )
        };
    if !owner.current(s) || previous.len() > MAXIMUM {
        return None;
    }
    if let VisibleChange::Append(ts) = &change {
        if ts <= &end
            || item(s, team, &key)?
                .replies()
                .iter()
                .any(|m| m.ts.as_ref() == Some(ts))
        {
            return None;
        }
    }
    Some(Prepared {
        owner,
        key,
        generation,
        before,
        slot,
        previous,
        lower,
        end,
        change,
    })
}
pub(crate) fn finish(s: &mut ShellState, prepared: Option<Prepared>) {
    let Some(p) = prepared else { return };
    if !p.owner.current(s)
        || s.core
            .workspaces
            .get(&p.owner.team)
            .is_none_or(|w| w.rt_generation != p.generation)
    {
        return;
    }
    let same_slot = match (&p.slot, &s.thread_feed_reads.context) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    };
    if !same_slot {
        return;
    }
    let before_current = match &p.before {
        Before::Original(scope, visible) => {
            scope.current(s)
                && scope
                    .windows
                    .get(&p.key)
                    .is_some_and(|w| w.visible == *visible)
        }
        Before::Preserved(scope, w) => {
            scope.current(s)
                && !w.retired.load(Ordering::Acquire)
                && scope.windows.get(&p.key).is_some_and(|v| Arc::ptr_eq(v, w))
        }
    };
    if !before_current || item(s, &p.owner.team, &p.key).is_none() {
        return;
    }
    let mut expected = p
        .previous
        .iter()
        .filter_map(|m| m.ts.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let end = match &p.change {
        VisibleChange::Append(ts) => {
            expected.insert(ts.clone());
            ts.clone()
        }
        VisibleChange::Remove(ts) => {
            expected.remove(ts);
            p.end.clone()
        }
    };
    let mut lower = p.lower;
    while expected.len() > MAXIMUM {
        expected.pop_first();
        lower = expected.first().unwrap().clone();
    }
    if !p.previous.is_empty()
        && !s.core.preserve_thread_feed_context(
            &p.owner.team,
            &p.key.0,
            &p.key.1,
            &p.previous,
            &p.change,
        )
    {
        return;
    }
    let Some(current) = rows(s, &p.owner.team, &p.key) else {
        return;
    };
    let selected = current
        .into_iter()
        .filter(|m| m.ts.as_deref().is_some_and(|t| t >= lower.as_str()))
        .collect::<Vec<_>>();
    let Some(actual) = ids(&selected) else { return };
    if actual != expected.into_iter().collect::<Vec<_>>()
        || selected
            .iter()
            .any(|m| m.kind.as_deref() != Some("message"))
    {
        return;
    }
    let mut windows = p
        .slot
        .as_ref()
        .map(|scope| scope.windows.clone())
        .unwrap_or_default();
    windows.insert(
        p.key,
        Arc::new(Reading {
            lower,
            end,
            ids: actual,
            retired: AtomicBool::new(false),
        }),
    );
    s.thread_feed_reads.context = Some(Arc::new(Scope {
        owner: p.owner,
        windows,
        retired: Arc::new(AtomicBool::new(false)),
    }));
}

/// Independent successful native full-body publication. The sealed input can
/// only preserve its actually published rows as reading memory, never mint an
/// original local window, further IO, profile, history or row-action purpose.
pub(in crate::thread_feed::read) fn publish_earlier(
    s: &mut ShellState,
    published: super::super::earlier::Published,
) {
    let Some((source, key, expected)) = published.reading(s) else {
        return;
    };
    let Some(selected) = rows(s, &source.team, &key) else {
        return;
    };
    if ids(&selected).as_ref() != Some(&expected)
        || selected
            .iter()
            .any(|m| m.kind.as_deref() != Some("message"))
    {
        return;
    }
    let owner = Owner {
        origin: source.root,
        owner: source.owner,
        cursor: source.cursor_owner,
        client: source.client,
        epoch: source.epoch,
        team: source.team,
    };
    if !owner.current(s) {
        return;
    }
    let mut windows = BTreeMap::new();
    if let Some(old) = s
        .thread_feed_reads
        .context
        .as_ref()
        .filter(|c| c.current(s))
    {
        for (k, w) in &old.windows {
            if k != &key && old.window_current(s, k) {
                windows.insert(k.clone(), w.clone());
            }
        }
    }
    windows.insert(
        key,
        Arc::new(Reading {
            lower: expected[0].clone(),
            end: expected.last().unwrap().clone(),
            ids: expected,
            retired: AtomicBool::new(false),
        }),
    );
    s.thread_feed_reads.context = Some(Arc::new(Scope {
        owner,
        windows,
        retired: Arc::new(AtomicBool::new(false)),
    }));
}
