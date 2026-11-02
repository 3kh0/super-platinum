use super::*;
use super_platinum_core::thread_reaction::OriginalRoot;
pub(super) type Coordinate = (String, String);
struct Seed {
    original: OriginalRoot,
    witness: Value,
}
pub(super) struct Candidates(BTreeMap<Coordinate, Seed>);
/// Only the original raw execute closure, before serde defaults, fills this.
#[derive(Default, Clone)]
pub(in super::super::super) struct Sink(Arc<Mutex<Option<Candidates>>>);
impl Sink {
    pub(in super::super::super) fn observe(&self, raw: &Value) {
        let candidates = (|| {
            if raw.get("ok") != Some(&Value::Bool(true))
                || raw.get("error").is_some_and(|v| !v.is_null())
            {
                return None;
            }
            let items = raw.get("threads")?.as_array()?;
            if items.len() > 6000 {
                return None;
            }
            let mut counts = BTreeMap::new();
            for item in items {
                if let Some(root) = item.get("root_msg") {
                    if let (Some(c), Some(t)) = (
                        root.get("channel").and_then(Value::as_str),
                        root.get("ts").and_then(Value::as_str),
                    ) {
                        *counts.entry((c.to_owned(), t.to_owned())).or_insert(0usize) += 1;
                    }
                }
            }
            let mut out = BTreeMap::new();
            for item in items {
                let Some(raw) = item.get("root_msg") else {
                    continue;
                };
                let (Some(c), Some(t)) = (
                    raw.get("channel").and_then(Value::as_str),
                    raw.get("ts").and_then(Value::as_str),
                ) else {
                    continue;
                };
                let key = (c.to_owned(), t.to_owned());
                if counts.get(&key) != Some(&1) {
                    continue;
                }
                let Ok(original) = OriginalRoot::capture(c, t, raw) else {
                    continue;
                };
                // Unlike the family roster, no existing-name/array witness is
                // required. Missing here NEVER becomes empty membership.
                let Ok(typed) = serde_json::from_value::<Message>(raw.clone()) else {
                    continue;
                };
                let Some(witness) = super::super::roster::witness(&typed) else {
                    continue;
                };
                out.insert(key, Seed { original, witness });
            }
            Some(Candidates(out))
        })();
        if let Ok(mut slot) = self.0.lock() {
            *slot = candidates;
        }
    }
    pub(super) fn take(&self) -> Option<Candidates> {
        self.0.lock().ok()?.take()
    }
}
pub(super) struct Row {
    pub original: OriginalRoot,
    witness: Value,
    pub retired: AtomicBool,
}
pub(super) struct Scope {
    pub source: Source,
    pub id: u64,
    account: Option<String>,
    sequence: u64,
    pub rows: BTreeMap<Coordinate, Row>,
    retired: AtomicBool,
    bound: Mutex<Option<(u64, ReadIdentity)>>,
}
pub(super) fn publish(source: &Source, s: &mut ShellState, candidates: Option<Candidates>) {
    let scope = (|| {
        let candidates = candidates?;
        if !source.admitted_current(s)
            || s.core.threads_view.loading
            || !s.core.threads_view.loaded
            || s.thread_feed_reads.owned_sequence != Some(s.core.threads_view.load_seq)
        {
            return None;
        }
        let mut rows = BTreeMap::new();
        for (key, seed) in candidates.0 {
            let Some(item) = unique(s, &key) else {
                continue;
            };
            if item
                .root_msg
                .team
                .as_deref()
                .is_some_and(|t| t != source.team)
                || super::super::roster::witness(&item.root_msg) != Some(seed.witness.clone())
                || super::super::roster::known_denied(s, &source.team, &key)
            {
                continue;
            }
            rows.insert(
                key,
                Row {
                    original: seed.original,
                    witness: seed.witness,
                    retired: AtomicBool::new(false),
                },
            );
        }
        if rows.is_empty() {
            return None;
        }
        let bound = super::super::connection(s, &source.team).map(|c| {
            (
                s.core.workspaces[&source.team].rt_generation,
                c.read_identity(),
            )
        });
        Some(Arc::new(Scope {
            source: source.clone(),
            id: super::super::next()?,
            account: s.core.active_account.clone(),
            sequence: s.core.threads_view.load_seq,
            rows,
            retired: AtomicBool::new(false),
            bound: Mutex::new(bound),
        }))
    })();
    s.thread_feed_reads.reaction_toggle.first_add.scope = scope;
}
pub(super) fn coordinate(item: &ThreadViewItem) -> Option<Coordinate> {
    super::super::roster::key(item)
}
fn unique<'a>(s: &'a ShellState, key: &Coordinate) -> Option<&'a ThreadViewItem> {
    let mut found = s
        .core
        .threads_view
        .items
        .iter()
        .filter(|item| coordinate(item).as_ref() == Some(key));
    let item = found.next()?;
    found.next().is_none().then_some(item)
}
impl Scope {
    pub fn physical(&self, key: &Coordinate) -> PhysicalKey {
        PhysicalKey {
            actor: Actor {
                account: self.account.clone().unwrap_or_default(),
                team: self.source.team.clone(),
                user: self.source.session.user_id.clone(),
            },
            channel: key.0.clone(),
            ts: key.1.clone(),
        }
    }
    pub fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let p = &self.source;
        let valid = p.admitted_current(s)
            && self.account == s.core.active_account
            && p.client.same_configuration(&s.core.client)
            && s.core.threads_view.loaded
            && !s.core.threads_view.loading
            && s.core.threads_view.load_seq == self.sequence
            && s.thread_feed_reads.owned_sequence == Some(self.sequence)
            && s.core.workspaces.get(&p.team).is_some_and(|w| {
                w.self_user_id == p.session.user_id && p.session.team_id == p.team
            })
            && s.thread_feed_reads
                .reaction_toggle
                .first_add
                .scope
                .as_ref()
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self));
        if !valid {
            self.retired.store(true, Ordering::Release);
        }
        valid
    }
    pub fn row_current(&self, s: &ShellState, key: &Coordinate, write: bool) -> bool {
        if !self.current(s) {
            return false;
        }
        let Some(row) = self.rows.get(key) else {
            return false;
        };
        if write && row.retired.load(Ordering::Acquire) {
            return false;
        }
        let valid = unique(s, key).is_some_and(|item| {
            item.root_msg
                .team
                .as_deref()
                .is_none_or(|t| t == self.source.team)
                && super::super::roster::witness(&item.root_msg) == Some(row.witness.clone())
        }) && !super::super::roster::known_denied(s, &self.source.team, key);
        if !valid {
            row.retired.store(true, Ordering::Release);
        }
        valid
    }
    pub fn caught_up(&self, s: &ShellState) -> bool {
        let Some(c) = super::super::connection(s, &self.source.team) else {
            return false;
        };
        let Ok(bound) = self.bound.lock() else {
            return false;
        };
        bound.as_ref().is_some_and(|(g, i)| {
            i.open()
                && i.is_for(c)
                && s.core.workspaces[&self.source.team].rt_generation == *g
                && s.thread_feed_reads
                    .reaction_toggle
                    .applied
                    .get(&self.source.team)
                    .is_some_and(|a| a.generation == *g && a.barrier.current_for(c))
        })
    }
    pub fn arm(&self, s: &ShellState, key: &Coordinate) -> Option<Arc<ReadWatch>> {
        if !self.row_current(s, key, true) || !self.caught_up(s) {
            return None;
        }
        let c = super::super::connection(s, &self.source.team)?;
        let applied = &s
            .thread_feed_reads
            .reaction_toggle
            .applied
            .get(&self.source.team)?
            .barrier;
        let watch = c.watch_message_read(&key.0)?;
        watch.armed_after(applied).then(|| Arc::new(watch))
    }
}
pub(in super::super) fn connection_changed(st: &mut State, team: &str) {
    if let Some(scope) = st.scope.as_ref().filter(|p| p.source.team == team) {
        if scope.bound.lock().map_or(true, |b| b.is_some()) {
            scope.retired.store(true, Ordering::Release);
        }
    }
}
pub(in super::super) fn barrier(
    st: &mut State,
    team: &str,
    generation: u64,
    identity: ReadIdentity,
    gap: bool,
) {
    if let Some(scope) = st.scope.as_ref().filter(|p| p.source.team == team) {
        if gap {
            scope.retired.store(true, Ordering::Release);
        }
        if let Ok(mut bound) = scope.bound.lock() {
            if bound.is_none() {
                *bound = Some((generation, identity));
            }
        }
    }
}
pub(in super::super) fn event(st: &mut State, team: &str, event: &RtEvent) {
    let Some(scope) = st.scope.as_ref().filter(|p| p.source.team == team) else {
        return;
    };
    let coordinate = match event {
        RtEvent::Message(m) => (
            m.channel.as_deref(),
            m.thread_ts.as_deref().or(m.ts.as_deref()),
        ),
        RtEvent::MessageChanged {
            channel, message, ..
        } => (
            Some(channel.as_str()),
            message.thread_ts.as_deref().or(message.ts.as_deref()),
        ),
        RtEvent::MessageDeleted { channel, .. } => (Some(channel.as_str()), None),
        RtEvent::ReactionAdded { channel, ts, .. }
        | RtEvent::ReactionRemoved { channel, ts, .. } => {
            (Some(channel.as_str()), Some(ts.as_str()))
        }
        RtEvent::PreferencesChanged { .. } | RtEvent::Unknown(_) | RtEvent::UserChanged(_) => {
            (None, None)
        }
        _ => return,
    };
    for (key, row) in &scope.rows {
        if coordinate.0.is_none_or(|c| c == key.0) && coordinate.1.is_none_or(|t| t == key.1) {
            row.retired.store(true, Ordering::Release);
        }
    }
}
