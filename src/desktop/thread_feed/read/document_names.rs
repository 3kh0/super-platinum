//! Original Doc coordinates own label reads only. Nothing here can construct
//! an actor, ProfileMember/Open, canonical User, navigation or SourceReady.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::Mutex,
};
use super_platinum_core::slack::models::{Message, ThreadViewItem};
mod identity;
mod policy;
mod runner;
#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
pub(crate) use identity::id as document_author_id;
type Key = (String, String, String);

pub(super) struct Candidates {
    rows: BTreeMap<Key, String>,
    touched: BTreeSet<Key>,
}
impl Candidates {
    pub(super) fn from_page(page: &ThreadsViewPage, team: &str) -> Option<Self> {
        let mut rows = BTreeMap::<Key, Option<String>>::new();
        for item in &page.threads {
            let (Some(channel), Some(root)) = (item.channel(), item.root_ts()) else {
                continue;
            };
            if channel.is_empty() || root.is_empty() {
                continue;
            }
            for message in std::iter::once(&item.root_msg)
                .chain(&item.latest_replies)
                .chain(&item.unread_replies)
            {
                let Some(ts) = message.ts.as_ref().filter(|s| !s.is_empty()) else {
                    continue;
                };
                let key = (channel.clone(), root.clone(), ts.clone());
                let id = qualified(team, &key, message).map(str::to_owned);
                rows.entry(key)
                    .and_modify(|old| {
                        if old != &id {
                            *old = None;
                        }
                    })
                    .or_insert(id);
                // Bounded original roster; never infer unseen rows from Core.
                if rows.len() > 6000 {
                    return None;
                }
            }
        }
        Some(Self {
            touched: rows.keys().cloned().collect(),
            rows: rows
                .into_iter()
                .filter_map(|(k, v)| v.map(|id| (k, id)))
                .collect(),
        })
    }
}
fn qualified<'a>(team: &str, key: &Key, message: &'a Message) -> Option<&'a str> {
    if message.ts.as_ref() != Some(&key.2)
        || message.channel.as_deref().is_some_and(|c| c != key.0)
        || message.thread_ts.as_deref().is_some_and(|r| r != key.1)
        || super_platinum_core::channel_threads::hidden(message)
    {
        return None;
    }
    identity::id(Some(team), message)
}
fn message<'a>(s: &'a ShellState, key: &Key, visible: bool) -> Option<&'a Message> {
    let mut items = s
        .core
        .threads_view
        .items
        .iter()
        .filter(|i| i.channel() == Some(&key.0) && i.root_ts() == Some(&key.1));
    let item = items.next()?;
    if items.next().is_some() {
        return None;
    }
    if s.core
        .workspaces
        .get(s.core.active_team.as_ref()?)?
        .messages
        .get(&key.0)
        .is_some_and(|b| b.was_deleted(&key.2))
    {
        return None;
    }
    if key.2 == key.1 {
        return Some(&item.root_msg);
    }
    if visible {
        return crate::view::selected_thread_replies(s, item)
            .into_iter()
            .find(|m| m.ts.as_ref() == Some(&key.2));
    }
    let mut found = item
        .latest_replies
        .iter()
        .chain(&item.unread_replies)
        .filter(|m| m.ts.as_ref() == Some(&key.2));
    let first = found.next()?;
    // Identical overlapping latest/unread occurrences are normal. Conflicts
    // cannot be classified by choosing one side of that overlap.
    if found.any(|m| serde_json::to_value(m).ok() != serde_json::to_value(first).ok()) {
        return None;
    }
    Some(first)
}
fn denied(s: &ShellState, team: &str, id: &str) -> bool {
    s.core.workspaces.get(team).is_none_or(|w| {
        w.users.get(id).is_some_and(|u| {
            u.id != id
                || u.is_bot
                || u.deleted
                || u.extra
                    .get("is_app_user")
                    .is_some_and(|v| !v.is_null() && v != &serde_json::json!(false))
        })
    })
}
#[derive(Clone)]
struct Label {
    id: String,
    value: String,
    policy: policy::Policy,
}
#[derive(Default)]
struct Phase {
    policy: Option<policy::Policy>,
    requested: HashSet<Key>,
    job: Option<Arc<Job>>,
    labels: BTreeMap<Key, Label>,
}
struct Route {
    view: MainView,
    channel: Option<String>,
    channel_generation: u64,
    root: Option<String>,
    thread_generation: u64,
    shortcut: u64,
    overlay: Option<crate::state::Overlay>,
    tab: crate::state::ChannelTab,
}
impl Route {
    fn capture(s: &ShellState) -> Self {
        Self {
            view: s.main_view,
            channel: s.core.active_channel.clone(),
            channel_generation: s.channel_generation,
            root: s.thread_root.clone(),
            thread_generation: s.thread_generation,
            shortcut: s.shortcut_generation,
            overlay: s.overlay.clone(),
            tab: s.channel_tab,
        }
    }
    fn current(&self, s: &ShellState) -> bool {
        self.view == s.main_view
            && self.channel == s.core.active_channel
            && self.channel_generation == s.channel_generation
            && self.root == s.thread_root
            && self.thread_generation == s.thread_generation
            && self.shortcut == s.shortcut_generation
            && self.overlay == s.overlay
            && self.tab == s.channel_tab
    }
}
struct Job {
    policy: policy::Policy,
    targets: BTreeMap<Key, String>,
    route: Route,
}
pub(super) struct Scope {
    origin: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor_owner: Arc<crate::realtime::activity_counts::CursorOwner>,
    client: SlackClient,
    epoch: u64,
    team: String,
    sequence: u64,
    rows: BTreeMap<Key, String>,
    retired: AtomicBool,
    retired_keys: Mutex<HashSet<Key>>,
    phase: Mutex<Phase>,
}
impl Scope {
    pub(super) fn capture(
        source: &Source,
        s: &ShellState,
        candidates: Option<Candidates>,
        append: bool,
    ) -> Option<Self> {
        if !source.admitted_current(s)
            || !s.core.client.same_configuration(&source.client)
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
        let incoming = candidates?;
        let old = s
            .thread_feed_reads
            .document_names
            .as_ref()
            .filter(|v| v.namespace_current(s));
        let mut rows = if append {
            old.map(|v| v.rows.clone()).unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        for key in incoming.touched {
            rows.remove(&key);
        }
        rows.extend(incoming.rows);
        rows.retain(|key, id| {
            message(s, key, false).and_then(|m| qualified(&source.team, key, m))
                == Some(id.as_str())
        });
        if rows.is_empty() {
            return None;
        }
        let mut labels = BTreeMap::new();
        if let Some(old) = old {
            let p = old.phase.lock().ok()?;
            for (key, label) in &p.labels {
                if rows.get(key) == Some(&label.id)
                    && old.current_id(s, key, false) == Some(label.id.as_str())
                    && label.policy.current(source, s)
                {
                    labels.insert(key.clone(), label.clone());
                }
            }
        }
        Some(Self {
            origin: source.root.clone(),
            owner: source.owner.clone(),
            cursor_owner: source.cursor_owner.clone(),
            client: source.client.clone(),
            epoch: source.epoch,
            team: source.team.clone(),
            sequence: s.core.threads_view.load_seq,
            rows,
            retired: AtomicBool::new(false),
            retired_keys: Mutex::default(),
            phase: Mutex::new(Phase {
                labels,
                ..Default::default()
            }),
        })
    }
    fn namespace_current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let valid = s.signed_in
            && s.core.account_epoch == self.epoch
            && s.core.active_team.as_ref() == Some(&self.team)
            && Arc::ptr_eq(&self.origin, &s.thread_feed_reads.incarnation)
            && self.owner.current(&s.core)
            && self.cursor_owner.current(&s.core)
            && self.client.same_configuration(&s.core.client)
            && s.core.threads_view.matches_scope(self.epoch, &self.team)
            && s.core.threads_view.loaded
            && s.thread_feed_reads.owned_sequence == Some(s.core.threads_view.load_seq)
            && s.thread_feed_reads
                .document_names
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), self));
        if !valid {
            self.retired.store(true, Ordering::Release);
        }
        valid
    }
    fn current_id<'a>(&'a self, s: &ShellState, key: &Key, visible: bool) -> Option<&'a str> {
        let id = self.rows.get(key)?;
        if self.retired_keys.lock().ok()?.contains(key) || denied(s, &self.team, id) {
            return None;
        }
        (message(s, key, visible).and_then(|m| qualified(&self.team, key, m)) == Some(id.as_str()))
            .then_some(id)
    }
    fn io_current(&self, source: &Source, s: &ShellState, job: &Arc<Job>) -> bool {
        source.admitted_current(s)
            && self.namespace_current(s)
            && job.policy.current(source, s)
            && job.route.current(s)
            && !s.core.threads_view.loading
            && s.core.threads_view.load_seq == self.sequence
            && s.thread_feed_reads.owned_sequence == Some(self.sequence)
            && self
                .phase
                .lock()
                .is_ok_and(|p| p.job.as_ref().is_some_and(|j| Arc::ptr_eq(j, job)))
    }
    fn reserve(self: &Arc<Self>, source: &Source, s: &ShellState) -> Option<Arc<Job>> {
        if !source.admitted_current(s)
            || !self.namespace_current(s)
            || s.core.threads_view.loading
            || s.core.threads_view.load_seq != self.sequence
            || s.thread_feed_reads.owned_sequence != Some(self.sequence)
        {
            return None;
        }
        // No reservation is consumed while the real policy is missing.
        let policy = policy::Policy::capture(source, s)?;
        let mut p = self.phase.lock().ok()?;
        if p.policy
            .as_ref()
            .is_none_or(|old| !old.store.same_publication(&policy.store))
        {
            p.policy = Some(policy.clone());
            p.requested.clear();
            p.job = None;
        }
        if p.job.is_some() {
            return None;
        }
        let targets = self
            .rows
            .iter()
            .filter(|(key, id)| {
                !p.requested.contains(*key)
                    && self.current_id(s, key, true) == Some(id.as_str())
                    && !p.labels.get(*key).is_some_and(|l| {
                        l.id == **id && l.policy.store.same_publication(&policy.store)
                    })
            })
            .map(|(k, id)| (k.clone(), id.clone()))
            .collect::<BTreeMap<_, _>>();
        if targets.is_empty() {
            return None;
        }
        p.requested.extend(targets.keys().cloned());
        let job = Arc::new(Job {
            policy,
            targets,
            route: Route::capture(s),
        });
        p.job = Some(job.clone());
        Some(job)
    }
}
pub(super) fn retire_missing(s: &ShellState) {
    let Some(scope) = s
        .thread_feed_reads
        .document_names
        .as_ref()
        .filter(|v| v.namespace_current(s))
    else {
        return;
    };
    if let Ok(mut retired) = scope.retired_keys.lock() {
        for (key, id) in &scope.rows {
            if message(s, key, false).and_then(|m| qualified(&scope.team, key, m))
                != Some(id.as_str())
                || denied(s, &scope.team, id)
            {
                retired.insert(key.clone());
            }
        }
    }
}
pub(super) fn label(
    source: &Source,
    s: &ShellState,
    item: &ThreadViewItem,
    m: &Message,
) -> Option<String> {
    if !source.admitted_current(s) {
        return None;
    }
    let id = identity::id(Some(&source.team), m)?;
    // Masking does not classify a human or authorize IO. Do not fall back to
    // an unmasked older name while the current label read is still pending.
    if policy::Policy::hidden(source, s, id) == Some(true) {
        return Some("Name hidden".into());
    }
    let _policy = policy::Policy::capture(source, s)?;
    let ready = (|| {
        let scope = s.thread_feed_reads.document_names.as_ref()?;
        if !scope.namespace_current(s) {
            return None;
        }
        let key = (
            item.channel()?.clone(),
            item.root_ts()?.clone(),
            m.ts.clone()?,
        );
        if scope.current_id(s, &key, true) != Some(id)
            || qualified(&scope.team, &key, m) != Some(id)
        {
            return None;
        }
        let p = scope.phase.lock().ok()?;
        let label = p.labels.get(&key)?;
        (label.id == id && label.policy.current(source, s)).then(|| label.value.clone())
    })();
    Some(ready.unwrap_or_else(|| id.into()))
}
pub(crate) fn schedule(state: Signal<ShellState>) {
    let admitted = (|| {
        let s = state.try_peek().ok()?;
        let source = Source::capture(&s)?;
        let scope = s.thread_feed_reads.document_names.as_ref()?.clone();
        let job = scope.reserve(&source, &s)?;
        Some((source, scope, job))
    })();
    if let Some((source, scope, job)) = admitted {
        dioxus::core::spawn_forever(runner::load(source, state, scope, job));
    }
}
