//! New timestamp-purpose original coordinates; no Copy, pane or profile grant.
use super::*;
use super_platinum_core::slack::{
    events::RtEvent,
    models::ThreadViewItem,
    realtime::{ReadIdentity, ReadWatch},
};

pub(super) type Key = (String, String);
fn key(item: &ThreadViewItem) -> Option<Key> {
    let channel = item.root_msg.channel.as_ref()?;
    let ts = item.root_msg.ts.as_ref()?;
    ((2..=255).contains(&channel.len())
        && channel.starts_with('C')
        && channel
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && ts.len() == 17
        && ts.as_bytes()[10] == b'.'
        && ts
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 10 || b.is_ascii_digit())
        && item.channel() == Some(channel)
        && item.root_ts() == Some(ts)
        && item.root_msg.thread_ts.as_ref().is_none_or(|t| t == ts))
    .then(|| (channel.clone(), ts.clone()))
}
fn namespace(t: &str) -> bool {
    t.starts_with('T')
        && t.len() >= 9
        && t.len() <= 64
        && t.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
pub(super) struct Candidates(BTreeSet<Key>, BTreeSet<Key>);
impl Candidates {
    pub(super) fn from_page(page: &ThreadsViewPage) -> Option<Self> {
        let mut counts = BTreeMap::new();
        for i in &page.threads {
            if let Some(k) = key(i) {
                *counts.entry(k).or_insert(0usize) += 1;
            }
        }
        if counts.len() > 6000 {
            return None;
        }
        let touched = counts.keys().cloned().collect();
        Some(Self(
            counts
                .into_iter()
                .filter_map(|(k, n)| (n == 1).then_some(k))
                .collect(),
            touched,
        ))
    }
}
struct Row {
    snapshot: Value,
    retired: AtomicBool,
}
pub(super) struct Carry {
    scope: Arc<Scope>,
    rows: BTreeMap<Key, Arc<Row>>,
    watches: BTreeMap<String, Arc<ReadWatch>>,
}
pub(super) struct Scope {
    pub source: Source,
    pub id: u64,
    pub identity: ReadIdentity,
    pub generation: u64,
    channels: u64,
    sequence: u64,
    rows: BTreeMap<Key, Arc<Row>>,
    retired: AtomicBool,
}
pub(super) fn connection<'a>(
    s: &'a ShellState,
    team: &str,
) -> Option<&'a super_platinum_core::slack::realtime::Connection> {
    let super_platinum_core::state::RealtimeStatus::Connected(c) = &s.core.workspaces.get(team)?.rt
    else {
        return None;
    };
    Some(c)
}
fn item<'a>(s: &'a ShellState, k: &Key) -> Option<&'a ThreadViewItem> {
    let mut found = s
        .core
        .threads_view
        .items
        .iter()
        .filter(|i| key(i).as_ref() == Some(k));
    let one = found.next()?;
    found.next().is_none().then_some(one)
}
fn witness(m: &super_platinum_core::slack::models::Message) -> Option<Value> {
    let mut v = serde_json::to_value(m).ok()?;
    // Read markers, reactions and reply counters do not change the selected
    // root's content/identity. The intent's ingress watch still retires on them.
    for name in [
        "reply_count",
        "reply_users",
        "reply_users_count",
        "latest_reply",
        "last_read",
        "reactions",
    ] {
        v.as_object_mut()?.remove(name);
    }
    Some(v)
}
impl Scope {
    pub(super) fn carry(s: &ShellState) -> Option<Carry> {
        let scope = s.thread_feed_reads.timestamp.scope.as_ref()?;
        if !scope.current(s) {
            return None;
        }
        let c = connection(s, &scope.source.team)?;
        let barrier = s.thread_feed_reads.applied_read_barrier.as_ref()?;
        if !barrier.current_for(c) {
            return None;
        }
        let rows = scope
            .rows
            .iter()
            .filter(|(k, _)| scope.key_current(s, k))
            .map(|(k, r)| (k.clone(), r.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut watches = BTreeMap::new();
        for k in rows.keys() {
            if !watches.contains_key(&k.0) {
                let w = c.watch_message_read(&k.0)?;
                if !w.armed_after(barrier) {
                    return None;
                }
                watches.insert(k.0.clone(), Arc::new(w));
            }
        }
        Some(Carry {
            scope: scope.clone(),
            rows,
            watches,
        })
    }
    pub(super) fn capture(
        source: &Source,
        s: &ShellState,
        incoming: Option<Candidates>,
        carry: Option<Carry>,
    ) -> Option<Self> {
        let incoming = incoming?;
        if !namespace(&source.team)
            || !source.admitted_current(s)
            || !source.client.same_configuration(&s.core.client)
            || !s
                .core
                .threads_view
                .matches_scope(source.epoch, &source.team)
            || !s.core.threads_view.loaded
            || s.core.threads_view.loading
            || s.thread_feed_reads.owned_sequence != Some(s.core.threads_view.load_seq)
        {
            return None;
        }
        let c = connection(s, &source.team)?;
        // Retain coordinates, never an old action/URL/control. The temporary
        // native append loading state may retire its UI; independent channel
        // watches and exact original lineage guard this data across the await.
        let mut rows: BTreeMap<Key, Arc<Row>> = carry
            .filter(|old| {
                Arc::ptr_eq(&old.scope.source.root, &source.root)
                    && old.scope.source.owner.current(&s.core)
                    && old.scope.source.cursor_owner.current(&s.core)
                    && old.scope.source.client.same_configuration(&source.client)
                    && old.scope.identity.is_for(c)
                    && old.scope.generation == s.core.workspaces[&source.team].rt_generation
                    && old.scope.channels
                        == s.core.workspaces[&source.team].channel_lifecycle.revision()
            })
            .map(|old| {
                old.rows
                    .into_iter()
                    .filter(|(k, r)| {
                        !r.retired.load(Ordering::Acquire)
                            && old.watches.get(&k.0).is_some_and(|w| w.current())
                            && old.scope.row_matches(s, k)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for k in incoming.1 {
            rows.remove(&k);
        }
        for k in incoming.0 {
            if let Some(i) = item(s, &k) {
                rows.insert(
                    k,
                    Arc::new(Row {
                        snapshot: witness(&i.root_msg)?,
                        retired: AtomicBool::new(false),
                    }),
                );
            }
        }
        if rows.is_empty() || rows.len() > 6000 {
            return None;
        }
        Some(Self {
            source: source.clone(),
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            identity: c.read_identity(),
            generation: s.core.workspaces[&source.team].rt_generation,
            channels: s.core.workspaces[&source.team].channel_lifecycle.revision(),
            sequence: s.core.threads_view.load_seq,
            rows,
            retired: AtomicBool::new(false),
        })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let p = &self.source;
        let ok = s.signed_in
            && s.main_view == MainView::Threads
            && s.thread_generation == p.thread_generation
            && s.core.account_epoch == p.epoch
            && s.core.active_team.as_ref() == Some(&p.team)
            && Arc::ptr_eq(&p.root, &s.thread_feed_reads.incarnation)
            && p.owner.current(&s.core)
            && p.cursor_owner.current(&s.core)
            && p.client.same_configuration(&s.core.client)
            && self.identity.open()
            && connection(s, &p.team).is_some_and(|c| self.identity.is_for(c))
            && s.core.workspaces.get(&p.team).is_some_and(|w| {
                w.rt_generation == self.generation
                    && w.channel_lifecycle.revision() == self.channels
            })
            && s.core.threads_view.matches_scope(p.epoch, &p.team)
            && s.core.threads_view.loaded
            && !s.core.threads_view.loading
            && s.core.threads_view.load_seq == self.sequence
            && s.thread_feed_reads.owned_sequence == Some(self.sequence)
            && s.thread_feed_reads
                .timestamp
                .scope
                .as_ref()
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self));
        if !ok {
            self.retired.store(true, Ordering::Release)
        }
        ok
    }
    fn row_matches(&self, s: &ShellState, k: &Key) -> bool {
        let Some(row) = self.rows.get(k) else {
            return false;
        };
        let Some(i) = item(s, k) else { return false };
        !super_platinum_core::channel_threads::hidden(&i.root_msg)
            && i.root_msg.extra.get("subscribed") != Some(&Value::Bool(false))
            && !deleted(s, &self.source.team, k)
            && !s
                .core
                .workspaces
                .get(&self.source.team)
                .and_then(|w| w.channels.get(&k.0))
                .is_some_and(|c| {
                    c.is_archived
                        || c.is_group
                        || c.is_im
                        || c.is_mpim
                        || c.extra.get("is_member") == Some(&Value::Bool(false))
                        || c.extra.get("is_org_shared") == Some(&Value::Bool(true))
                        || c.extra
                            .get("context_team_id")
                            .is_some_and(|v| v.as_str() != Some(self.source.team.as_str()))
                })
            && witness(&i.root_msg).as_ref() == Some(&row.snapshot)
    }
    pub(super) fn key_current(&self, s: &ShellState, k: &Key) -> bool {
        let Some(row) = self.rows.get(k) else {
            return false;
        };
        if row.retired.load(Ordering::Acquire) || !self.current(s) {
            return false;
        }
        let ok = self.row_matches(s, k);
        if !ok {
            row.retired.store(true, Ordering::Release)
        }
        ok
    }
}
fn deleted(s: &ShellState, team: &str, k: &Key) -> bool {
    s.core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(&k.0))
        .is_some_and(|b| b.was_deleted(&k.1))
        || s.core
            .threads
            .get(&(team.into(), k.0.clone(), k.1.clone()))
            .is_some_and(|b| b.was_deleted(&k.1))
}
#[derive(Clone)]
pub(crate) struct Action {
    pub(super) scope: Arc<Scope>,
    pub(super) key: Key,
    source: Source,
    channel: u64,
    shortcut: u64,
    observed: Option<u64>,
}
impl Action {
    pub(crate) fn control_key(&self) -> String {
        format!("{}-{}", self.button_id(), self.observed.unwrap_or(0))
    }
    pub(super) fn capture(source: &Source, s: &ShellState, i: &ThreadViewItem) -> Option<Self> {
        if s.overlay.is_some() || !source.current(s) {
            return None;
        }
        let scope = s.thread_feed_reads.timestamp.scope.as_ref()?.clone();
        let k = key(i)?;
        // Rendered presence is a cue. Full root/witness revalidation belongs to
        // the callback and every await; no per-render full-message serialization.
        if !scope.current(s) || scope.rows.get(&k)?.retired.load(Ordering::Acquire) {
            return None;
        }
        Some(Self {
            scope,
            key: k,
            source: source.clone(),
            channel: s.channel_generation,
            shortcut: s.shortcut_generation,
            observed: s.thread_feed_reads.timestamp.intent.as_ref().map(|p| p.id),
        })
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.base_current(s)
            && self.observed == s.thread_feed_reads.timestamp.intent.as_ref().map(|p| p.id)
    }
    pub(super) fn base_current(&self, s: &ShellState) -> bool {
        self.route_current(s) && self.scope.key_current(s, &self.key)
    }
    pub(super) fn route_current(&self, s: &ShellState) -> bool {
        s.overlay.is_none()
            && self.source.current(s)
            && s.channel_generation == self.channel
            && s.shortcut_generation == self.shortcut
    }
    pub(super) fn arm(&self, s: &ShellState) -> Option<Arc<ReadWatch>> {
        if !self.current(s) {
            return None;
        }
        let c = connection(s, &self.scope.source.team)?;
        let b = s.thread_feed_reads.applied_read_barrier.as_ref()?;
        if !b.current_for(c) {
            return None;
        }
        let watch = c.watch_message_read(&self.key.0)?;
        watch.armed_after(b).then(|| Arc::new(watch))
    }
    pub(crate) fn button_id(&self) -> String {
        format!(
            "thread-timestamp-{}-{}-{}",
            self.scope.id,
            self.key.0,
            self.key.1.replace('.', "")
        )
    }
}
impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &other.scope)
            && self.key == other.key
            && self.channel == other.channel
            && self.shortcut == other.shortcut
            && self.observed == other.observed
            && self.source.stamp == other.source.stamp
            && self.source.thread_generation == other.source.thread_generation
    }
}
pub(crate) struct Demotion {
    scope: Arc<Scope>,
    keys: Vec<Key>,
    force: bool,
}
pub(crate) fn before_event(
    s: &ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<Demotion> {
    let scope = s.thread_feed_reads.timestamp.scope.as_ref()?.clone();
    if scope.source.team != team || scope.generation != generation || !scope.current(s) {
        return None;
    }
    let (channel, ts, force) = match event {
        RtEvent::Message(m) => (
            m.channel.as_deref(),
            m.thread_ts.as_deref().or(m.ts.as_deref()),
            false,
        ),
        RtEvent::MessageChanged {
            channel, message, ..
        } => (
            Some(channel.as_str()),
            message.ts.as_deref().or(message.thread_ts.as_deref()),
            false,
        ),
        RtEvent::MessageDeleted { channel, .. } => (Some(channel.as_str()), None, false),
        RtEvent::ReactionAdded { channel, ts, .. }
        | RtEvent::ReactionRemoved { channel, ts, .. } => {
            (Some(channel.as_str()), Some(ts.as_str()), false)
        }
        RtEvent::Unknown(r)
            if matches!(r.kind.as_str(), "thread_subscribed" | "thread_unsubscribed") =>
        {
            (None, None, true)
        }
        _ => return None,
    };
    let keys = scope
        .rows
        .keys()
        .filter(|k| channel.is_none_or(|c| c == k.0) && ts.is_none_or(|t| t == k.1))
        .cloned()
        .collect();
    Some(Demotion { scope, keys, force })
}
pub(crate) fn after_event(s: &ShellState, d: Option<Demotion>) {
    let Some(d) = d else { return };
    if !d.scope.current(s) {
        return;
    }
    for k in d.keys {
        if d.force || !d.scope.row_matches(s, &k) {
            if let Some(r) = d.scope.rows.get(&k) {
                r.retired.store(true, Ordering::Release)
            }
        }
    }
}
