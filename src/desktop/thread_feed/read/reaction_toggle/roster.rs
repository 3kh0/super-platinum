use super::*;
use super_platinum_core::thread_reaction::OriginalRoot;
pub(super) type Key = (String, String);
pub(super) fn key(i: &ThreadViewItem) -> Option<Key> {
    let c = i.root_msg.channel.as_ref()?;
    let t = i.root_msg.ts.as_ref()?;
    (i.channel() == Some(c) && i.root_ts() == Some(t)).then(|| (c.clone(), t.clone()))
}
struct Seed {
    original: OriginalRoot,
    witness: Value,
    names: Vec<String>,
}
pub(super) struct Candidates(BTreeMap<Key, Seed>);
// Empty in every old typed fetch closure. Only the private raw executor fills
// this sink before the existing Core serde decode; a loaded Vec cannot mint it.
#[derive(Default, Clone)]
pub(in super::super) struct Sink(Arc<Mutex<Option<Candidates>>>);
impl Sink {
    pub(super) fn observe(&self, raw: &Value) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Candidates::capture(raw);
        }
    }
    pub(super) fn take(&self) -> Option<Candidates> {
        self.0.lock().ok()?.take()
    }
}
pub(super) fn witness(m: &Message) -> Option<Value> {
    let mut v = serde_json::to_value(m).ok()?;
    for f in [
        "reactions",
        "reply_count",
        "reply_users",
        "reply_users_count",
        "latest_reply",
        "last_read",
    ] {
        v.as_object_mut()?.remove(f);
    }
    Some(v)
}
impl Candidates {
    fn capture(raw: &Value) -> Option<Self> {
        if raw.get("ok") != Some(&Value::Bool(true))
            || raw.get("error").is_some_and(|v| !v.is_null())
        {
            return None;
        }
        let rows = raw.get("threads")?.as_array()?;
        if rows.len() > 6000 {
            return None;
        }
        let mut counts = BTreeMap::new();
        for row in rows {
            if let Some(m) = row.get("root_msg") {
                if let (Some(c), Some(t)) = (
                    m.get("channel").and_then(Value::as_str),
                    m.get("ts").and_then(Value::as_str),
                ) {
                    *counts.entry((c.to_owned(), t.to_owned())).or_insert(0usize) += 1;
                }
            }
        }
        let mut out = BTreeMap::new();
        for row in rows {
            let Some(m) = row.get("root_msg") else {
                continue;
            };
            let (Some(c), Some(t)) = (
                m.get("channel").and_then(Value::as_str),
                m.get("ts").and_then(Value::as_str),
            ) else {
                continue;
            };
            let k = (c.to_owned(), t.to_owned());
            if counts.get(&k) != Some(&1) {
                continue;
            }
            let Ok(original) = OriginalRoot::capture(c, t, m) else {
                continue;
            };
            let Some(reactions) = m.get("reactions").and_then(Value::as_array) else {
                continue;
            };
            if reactions.is_empty() || reactions.len() > 1024 {
                continue;
            }
            let mut names = std::collections::BTreeSet::new();
            let mut valid = true;
            for r in reactions {
                let Some(name) = r.get("name").and_then(Value::as_str) else {
                    valid = false;
                    break;
                };
                if name.is_empty()
                    || name.len() > 256
                    || !names.insert(name.to_owned())
                    || r.get("count")
                        .and_then(Value::as_u64)
                        .is_none_or(|n| n == 0 || n > u64::from(u32::MAX))
                {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            // Coherence only: positive presence/policy above came from RAW.
            let Ok(typed) = serde_json::from_value::<Message>(m.clone()) else {
                continue;
            };
            let Some(witness) = witness(&typed) else {
                continue;
            };
            out.insert(
                k,
                Seed {
                    original,
                    witness,
                    names: names.into_iter().collect(),
                },
            );
        }
        Some(Self(out))
    }
}
pub(super) struct Row {
    pub original: OriginalRoot,
    witness: Value,
    pub names: Vec<String>,
    pub retired: AtomicBool,
}
pub(super) struct Scope {
    pub source: Source,
    pub id: u64,
    sequence: u64,
    account: Option<String>,
    pub rows: BTreeMap<Key, Row>,
    pub retired: AtomicBool,
    pub bound: Mutex<Option<(u64, ReadIdentity)>>,
}
pub(super) fn publish(source: &Source, s: &mut ShellState, incoming: Option<Candidates>) {
    let scope = (|| {
        let incoming = incoming?;
        if !source.admitted_current(s)
            || s.core.threads_view.loading
            || !s.core.threads_view.loaded
            || s.thread_feed_reads.owned_sequence != Some(s.core.threads_view.load_seq)
        {
            return None;
        }
        let mut rows = BTreeMap::new();
        for (k, seed) in incoming.0 {
            let Some(i) = item(s, &k) else { continue };
            if i.root_msg.team.as_deref().is_some_and(|t| t != source.team)
                || witness(&i.root_msg) != Some(seed.witness.clone())
                || known_denied(s, &source.team, &k)
            {
                continue;
            }
            rows.insert(
                k,
                Row {
                    original: seed.original,
                    witness: seed.witness,
                    names: seed.names,
                    retired: AtomicBool::new(false),
                },
            );
        }
        if rows.is_empty() {
            return None;
        }
        let bound = connection(s, &source.team).map(|c| {
            (
                s.core.workspaces[&source.team].rt_generation,
                c.read_identity(),
            )
        });
        Some(Arc::new(Scope {
            source: source.clone(),
            id: next()?,
            sequence: s.core.threads_view.load_seq,
            account: s.core.active_account.clone(),
            rows,
            retired: AtomicBool::new(false),
            bound: Mutex::new(bound),
        }))
    })();
    s.thread_feed_reads.reaction_toggle.scope = scope;
    s.thread_feed_reads.reaction_toggle.display = None;
    s.thread_feed_reads.reaction_toggle.checking = None;
    s.thread_feed_reads.reaction_toggle.check_notice = None;
}
fn item<'a>(s: &'a ShellState, k: &Key) -> Option<&'a ThreadViewItem> {
    let mut rows = s
        .core
        .threads_view
        .items
        .iter()
        .filter(|i| key(i).as_ref() == Some(k));
    let first = rows.next()?;
    rows.next().is_none().then_some(first)
}
impl Scope {
    pub fn physical(&self, k: &Key) -> PhysicalKey {
        PhysicalKey {
            actor: Actor {
                account: self.account.clone().unwrap_or_default(),
                team: self.source.team.clone(),
                user: self.source.session.user_id.clone(),
            },
            channel: k.0.clone(),
            ts: k.1.clone(),
        }
    }
    pub fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let p = &self.source;
        let ok = p.admitted_current(s)
            && self.account == s.core.active_account
            && p.client.same_configuration(&s.core.client)
            && s.core.threads_view.loaded
            && !s.core.threads_view.loading
            && s.core.workspaces.get(&p.team).is_some_and(|w| {
                w.self_user_id == p.session.user_id && p.session.team_id == p.team
            })
            && s.core.threads_view.load_seq == self.sequence
            && s.thread_feed_reads.owned_sequence == Some(self.sequence)
            && s.thread_feed_reads
                .reaction_toggle
                .scope
                .as_ref()
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self));
        if !ok {
            self.retired.store(true, Ordering::Release);
        }
        ok
    }
    pub fn caught_up(&self, s: &ShellState) -> bool {
        let Some(c) = connection(s, &self.source.team) else {
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
    pub fn read_current(&self, s: &ShellState, k: &Key) -> bool {
        if !self.current(s) || s.overlay.is_some() {
            return false;
        }
        let Some(row) = self.rows.get(k) else {
            return false;
        };
        let Some(i) = item(s, k) else { return false };
        i.root_msg
            .team
            .as_deref()
            .is_none_or(|t| t == self.source.team)
            && witness(&i.root_msg) == Some(row.witness.clone())
            && !known_denied(s, &self.source.team, k)
    }
    pub fn write_current(&self, s: &ShellState, k: &Key) -> bool {
        self.read_current(s, k)
            && self
                .rows
                .get(k)
                .is_some_and(|r| !r.retired.load(Ordering::Acquire))
    }
    pub fn arm(&self, s: &ShellState, k: &Key) -> Option<Arc<ReadWatch>> {
        if !self.read_current(s, k) || !self.caught_up(s) {
            return None;
        }
        let c = connection(s, &self.source.team)?;
        let b = &s
            .thread_feed_reads
            .reaction_toggle
            .applied
            .get(&self.source.team)?
            .barrier;
        let watch = c.watch_message_read(&k.0)?;
        watch.armed_after(b).then(|| Arc::new(watch))
    }
}
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Number(v)) => v.as_f64().is_some_and(|n| n != 0.),
        Some(_) => true,
    }
}
pub(super) fn known_denied(s: &ShellState, team: &str, k: &Key) -> bool {
    if crate::reaction_writes::known_reply(s, team, &k.0, &k.1) {
        return true;
    }
    let Some(w) = s.core.workspaces.get(team) else {
        return true;
    };
    if w.channels.get(&k.0).is_some_and(|c| {
        c.is_private
            || c.is_group
            || c.is_im
            || c.is_mpim
            || c.is_archived
            || ["is_frozen", "is_file", "isUnknown", "isNonExistent"]
                .iter()
                .any(|f| truthy(c.extra.get(*f)))
            || c.extra.get("is_member") == Some(&Value::Bool(false))
    }) {
        return true;
    }
    let deny = |m: &Message| {
        m.ts.as_deref() == Some(k.1.as_str())
            && (m
                .thread_ts
                .as_deref()
                .is_some_and(|t| !t.is_empty() && t != k.1)
                || m.kind.as_deref().is_some_and(|kind| kind != "message")
                || m.subtype.is_some()
                || m.bot_id.is_some()
                || m.bot_profile.is_some()
                || !m.files.is_empty()
                || [
                    "is_locked",
                    "is_beyond_free_limit",
                    "isUnknown",
                    "isNonExistent",
                    "isEKMError",
                    "is_ephemeral",
                    "hidden",
                    "is_hidden",
                    "deleted",
                    "is_deleted",
                    "is_tombstone",
                    "is_tombstoned",
                    "no_display",
                    "_hidden_stub",
                ]
                .iter()
                .any(|f| truthy(m.extra.get(*f)))
                || [
                    "app_id",
                    "document_comment",
                    "workflow",
                    "workflow_id",
                    "workflow_responses",
                    "ai_context",
                    "file",
                    "file_id",
                ]
                .iter()
                .any(|f| m.extra.get(*f).is_some_and(|v| !v.is_null())))
    };
    w.messages
        .get(&k.0)
        .is_some_and(|b| b.was_deleted(&k.1) || b.messages.iter().any(deny))
        || s.core.threads.iter().any(|((t, c, _), b)| {
            t == team && c == &k.0 && (b.was_deleted(&k.1) || b.messages.iter().any(deny))
        })
}
/// Render-only common validation over one immutably borrowed Shell snapshot.
/// This value cannot cross an async handoff or replace Action::current at use.
pub(crate) struct PreparedRow<'a> {
    source: Source,
    snapshot: &'a ShellState,
    scope: Arc<Scope>,
    key: Key,
    disabled: bool,
}
impl<'a> PreparedRow<'a> {
    pub(super) fn capture(source: &Source, s: &'a ShellState, i: &ThreadViewItem) -> Option<Self> {
        let scope = s.thread_feed_reads.reaction_toggle.scope.clone()?;
        let key = key(i)?;
        if !source.current(s)
            || s.overlay.is_some()
            || s.core.editing.is_some()
            || !scope.write_current(s, &key)
            || !scope.caught_up(s)
        {
            return None;
        }
        let disabled = crate::reaction_writes::registry(s).blocked(&scope.physical(&key))
            || s.thread_feed_reads
                .reaction_toggle
                .active
                .as_ref()
                .is_some_and(|p| p.phase.load(Ordering::Acquire) < 4);
        Some(Self {
            source: source.clone(),
            snapshot: s,
            scope,
            key,
            disabled,
        })
    }
    pub(crate) fn action(&self, name: &str) -> Option<Action> {
        // Only the per-name supported spelling/actual-original-row membership
        // varies. The full Message witness and known native vetoes above are
        // common to all pills in this one immutable render snapshot.
        if super_platinum_core::state::emoji_glyph(name).starts_with(':')
            || !self
                .scope
                .rows
                .get(&self.key)?
                .names
                .iter()
                .any(|n| n == name)
        {
            return None;
        }
        Some(Action::from_row(
            &self.source,
            self.snapshot,
            self.scope.clone(),
            self.key.clone(),
            name,
        ))
    }
    pub(crate) fn disabled(&self) -> bool {
        self.disabled
    }
}
#[derive(Clone)]
pub(crate) struct Action {
    pub(super) scope: Arc<Scope>,
    pub(super) key: Key,
    pub(super) name: String,
    source: Source,
    channel: u64,
    edit: u64,
    shortcut: u64,
}
impl PartialEq for Action {
    fn eq(&self, b: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &b.scope)
            && self.key == b.key
            && self.name == b.name
            && self.source.stamp == b.source.stamp
            && self.channel == b.channel
            && self.edit == b.edit
            && self.shortcut == b.shortcut
    }
}
impl Action {
    fn from_row(source: &Source, s: &ShellState, scope: Arc<Scope>, key: Key, name: &str) -> Self {
        Self {
            scope,
            key,
            name: name.into(),
            source: source.clone(),
            channel: s.channel_generation,
            edit: s.edit_generation,
            shortcut: s.shortcut_generation,
        }
    }
    pub(super) fn capture(
        source: &Source,
        s: &ShellState,
        i: &ThreadViewItem,
        name: &str,
    ) -> Option<Self> {
        let scope = s.thread_feed_reads.reaction_toggle.scope.clone()?;
        let key = key(i)?;
        // A cheap glyph hint permits a READ intent only; the genuine current
        // catalog independently validates the supported spelling before write.
        if super_platinum_core::state::emoji_glyph(name).starts_with(':')
            || !scope.rows.get(&key)?.names.iter().any(|n| n == name)
        {
            return None;
        }
        let a = Self::from_row(source, s, scope, key, name);
        (a.current(s) && a.scope.caught_up(s)).then_some(a)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.source.current(s)
            && s.overlay.is_none()
            && s.core.editing.is_none()
            && s.channel_generation == self.channel
            && s.edit_generation == self.edit
            && s.shortcut_generation == self.shortcut
            && self.scope.write_current(s, &self.key)
    }
    pub(super) fn physical(&self) -> PhysicalKey {
        self.scope.physical(&self.key)
    }
    pub(crate) fn disabled(&self, s: &ShellState) -> bool {
        !self.current(s)
            || !self.scope.caught_up(s)
            || crate::reaction_writes::registry(s).blocked(&self.physical())
            || s.thread_feed_reads
                .reaction_toggle
                .active
                .as_ref()
                .is_some_and(|p| p.phase.load(Ordering::Acquire) < 4)
    }
    pub(crate) fn id(&self) -> String {
        format!(
            "thread-root-reaction-{}-{}-{}-{}",
            self.scope.id,
            self.key.0,
            self.key.1.replace('.', ""),
            self.name
        )
    }
}
