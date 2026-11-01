//! Explicit bounded full-body native reader; no Source cache/window equality.
use super::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::AtomicU64},
};
use super_platinum_core::slack::{
    models::{Message, ThreadViewItem},
    realtime::{ReadBarrier, ReadIdentity, ReadWatch},
};
mod load;
mod seed;
#[cfg(test)]
mod test_io;
#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
pub(super) use seed::Candidates;
type Key = (String, String);
#[derive(Default)]
struct Phase {
    serial: u64,
    pending: Option<Arc<Ticket>>,
    unavailable: bool,
    done: Option<Vec<String>>,
}
struct Window {
    seed: seed::Seed,
    phase: Mutex<Phase>,
    retired: AtomicBool,
}
#[derive(Clone)]
pub(super) struct Scope {
    source: Source,
    identity: ReadIdentity,
    windows: BTreeMap<Key, Arc<Window>>,
    retired: Arc<AtomicBool>,
    controls: Arc<AtomicU64>,
}
fn connection<'a>(
    s: &'a ShellState,
    team: &str,
) -> Option<&'a super_platinum_core::slack::realtime::Connection> {
    let super_platinum_core::state::RealtimeStatus::Connected(c) = &s.core.workspaces.get(team)?.rt
    else {
        return None;
    };
    Some(c)
}
pub(crate) fn accept_barrier(
    s: &mut ShellState,
    team: &str,
    generation: u64,
    barrier: ReadBarrier,
) {
    if s.core.active_team.as_deref() == Some(team)
        && s.core
            .workspaces
            .get(team)
            .is_some_and(|w| w.rt_generation == generation)
        && connection(s, team).is_some_and(|c| barrier.belongs_to(c))
    {
        s.thread_feed_reads.applied_read_barrier = Some(barrier);
    }
}
impl Scope {
    pub(super) fn capture(
        source: &Source,
        s: &ShellState,
        incoming: Candidates,
        append: bool,
    ) -> Option<Self> {
        if !source.admitted_current(s)
            || !source.client.same_configuration(&s.core.client)
            || !s.core.threads_view.loaded
            || s.core.threads_view.loading
            || s.thread_feed_reads.owned_sequence != Some(s.core.threads_view.load_seq)
        {
            return None;
        }
        let identity = connection(s, &source.team)?.read_identity();
        let previous = s
            .thread_feed_reads
            .earlier
            .as_ref()
            .filter(|p| append && p.current(s));
        let mut windows = BTreeMap::new();
        if let Some(p) = previous {
            for (k, w) in &p.windows {
                if p.window_current(s, k) {
                    windows.insert(k.clone(), w.clone());
                }
            }
        }
        for (key, seed) in incoming.0 {
            windows.remove(&key);
            if let Some(seed) = seed {
                windows.insert(
                    key,
                    Arc::new(Window {
                        seed,
                        phase: Mutex::new(Phase::default()),
                        retired: AtomicBool::new(false),
                    }),
                );
            }
        }
        let mut scope = Self {
            source: source.clone(),
            identity,
            windows,
            retired: Arc::new(AtomicBool::new(false)),
            controls: Arc::new(AtomicU64::new(0)),
        };
        scope
            .windows
            .retain(|key, w| Self::row_current(s, &source.team, key, w));
        (!scope.windows.is_empty()).then_some(scope)
    }
    fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let p = &self.source;
        let ok = s.signed_in
            && s.core.account_epoch == p.epoch
            && s.core.active_team.as_ref() == Some(&p.team)
            && Arc::ptr_eq(&p.root, &s.thread_feed_reads.incarnation)
            && p.owner.current(&s.core)
            && p.cursor_owner.current(&s.core)
            && p.client.same_configuration(&s.core.client)
            && self.identity.open()
            && connection(s, &p.team).is_some_and(|c| self.identity.is_for(c))
            && s.core.threads_view.matches_scope(p.epoch, &p.team)
            && s.core.threads_view.loaded
            && !s.core.threads_view.loading
            && s.thread_feed_reads.owned_sequence == Some(s.core.threads_view.load_seq)
            && s.thread_feed_reads
                .earlier
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), self));
        if !ok {
            self.retired.store(true, Ordering::Release);
        }
        ok
    }
    fn row_current(s: &ShellState, team: &str, key: &Key, w: &Window) -> bool {
        let Some(i) = seed::item(s, team, key) else {
            return false;
        };
        if !i.unread_replies.is_empty() || !w.seed.root_matches(i, team, key) {
            return false;
        }
        let Ok(phase) = w.phase.lock() else {
            return false;
        };
        let expected = phase.done.as_ref().unwrap_or(&w.seed.ids);
        seed::ids(i).as_ref() == Some(expected)
            && i.latest_replies.iter().chain(&i.unread_replies).all(|m| {
                seed::ordinary(m, team, key, false)
                    && m.ts
                        .as_deref()
                        .is_some_and(|t| !seed::deleted(s, team, key, t))
            })
    }
    fn window_current(&self, s: &ShellState, key: &Key) -> bool {
        let Some(w) = self.windows.get(key) else {
            return false;
        };
        if w.retired.load(Ordering::Acquire) || !self.current(s) {
            return false;
        }
        let ok = Self::row_current(s, &self.source.team, key, w);
        if !ok {
            w.retired.store(true, Ordering::Release);
        }
        ok
    }
}
pub(super) fn retire_missing(s: &ShellState) {
    if let Some(scope) = s.thread_feed_reads.earlier.as_ref() {
        for key in scope.windows.keys() {
            scope.window_current(s, key);
        }
    }
}
#[derive(Clone)]
pub(crate) struct Action {
    source: Source,
    scope: Arc<Scope>,
    key: Key,
    serial: u64,
    controls: u64,
    pending: bool,
}
struct Ticket {
    serial: u64,
    watch: ReadWatch,
}
impl Action {
    pub(super) fn capture(source: &Source, s: &ShellState, i: &ThreadViewItem) -> Option<Self> {
        if !source.current(s) || s.overlay.is_some() {
            return None;
        }
        let key = (i.root_msg.channel.clone()?, i.root_msg.ts.clone()?);
        let scope = s.thread_feed_reads.earlier.as_ref()?.clone();
        if !scope.window_current(s, &key) {
            return None;
        }
        let w = scope.windows.get(&key)?;
        let phase = w.phase.lock().ok()?;
        if phase.unavailable || phase.done.is_some() {
            return None;
        }
        // All original tail rows must already be visible. This does not borrow
        // local expansion or infer an earlier-history capability from its count.
        let shown = crate::view::selected_thread_replies(s, i)
            .iter()
            .filter_map(|m| m.ts.clone())
            .collect::<Vec<_>>();
        if shown != w.seed.ids {
            return None;
        }
        let ready = s
            .thread_feed_reads
            .applied_read_barrier
            .as_ref()
            .is_some_and(|b| connection(s, &source.team).is_some_and(|c| b.current_for(c)));
        let serial = phase.serial;
        let pending = phase.pending.is_some() || !ready;
        drop(phase);
        let controls = scope.controls.load(Ordering::Acquire);
        Some(Self {
            source: source.clone(),
            scope,
            key,
            serial,
            controls,
            pending,
        })
    }
    pub(crate) fn control_id(&self) -> String {
        format!("thread-feed-earlier-network-{}-{}", self.key.0, self.key.1)
    }
    pub(crate) fn pending(&self) -> bool {
        self.pending
    }
    pub(crate) fn start(&self, mut state: Signal<ShellState>) {
        if self.pending {
            return;
        }
        let ticket = {
            let Ok(s) = state.try_write() else { return };
            if !self.source.current(&s)
                || s.overlay.is_some()
                || !self.scope.window_current(&s, &self.key)
            {
                return;
            }
            let Some(c) = connection(&s, &self.source.team) else {
                return;
            };
            let Some(applied) = s.thread_feed_reads.applied_read_barrier.as_ref() else {
                return;
            };
            let Some(watch) = c.watch_message_read(&self.key.0) else {
                return;
            };
            if !watch.armed_after(applied) {
                return;
            }
            let w = &self.scope.windows[&self.key];
            let Ok(mut phase) = w.phase.lock() else {
                return;
            };
            if phase.serial != self.serial
                || phase.pending.is_some()
                || phase.done.is_some()
                || phase.unavailable
            {
                return;
            }
            let Some(serial) = phase.serial.checked_add(1) else {
                w.retired.store(true, Ordering::Release);
                return;
            };
            let Some(next_controls) = self.controls.checked_add(1) else {
                self.scope.retired.store(true, Ordering::Release);
                return;
            };
            if self
                .scope
                .controls
                .compare_exchange(
                    self.controls,
                    next_controls,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                return;
            }
            phase.serial = serial;
            let ticket = Arc::new(Ticket { serial, watch });
            phase.pending = Some(ticket.clone());
            ticket
        };
        dioxus::core::spawn_forever(load::run(self.clone(), state, ticket));
    }
    fn active(&self, s: &ShellState, t: &Arc<Ticket>) -> bool {
        self.source.current(s)
            && s.overlay.is_none()
            && self.scope.window_current(s, &self.key)
            && t.watch.current()
            && self.scope.windows[&self.key].phase.lock().is_ok_and(|p| {
                p.serial == t.serial && p.pending.as_ref().is_some_and(|v| Arc::ptr_eq(v, t))
            })
    }
    fn release(&self, t: &Arc<Ticket>, unavailable: bool, done: Option<Vec<String>>) -> bool {
        let Ok(mut p) = self.scope.windows[&self.key].phase.lock() else {
            return false;
        };
        if p.pending.as_ref().is_none_or(|v| !Arc::ptr_eq(v, t)) {
            return false;
        }
        p.pending = None;
        p.unavailable = unavailable;
        p.done = done;
        if self
            .scope
            .controls
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
            .is_err()
        {
            self.scope.retired.store(true, Ordering::Release);
        }

        if let Some(next) = p.serial.checked_add(1) {
            p.serial = next;
        } else {
            self.scope.windows[&self.key]
                .retired
                .store(true, Ordering::Release);
        }
        true
    }
}
/// Created only after the actual strict IO publication; no public/DTO ctor.
pub(in crate::thread_feed::read) struct Published {
    action: Action,
    ids: Vec<String>,
}
impl Published {
    pub(in crate::thread_feed::read) fn reading(
        self,
        s: &ShellState,
    ) -> Option<(Source, Key, Vec<String>)> {
        if !self.action.scope.window_current(s, &self.action.key) {
            return None;
        }
        let p = self.action.scope.windows[&self.action.key]
            .phase
            .lock()
            .ok()?;
        if p.done.as_ref() != Some(&self.ids) || self.ids.is_empty() || self.ids.len() > 200 {
            return None;
        }
        drop(p);
        Some((self.action.source, self.action.key, self.ids))
    }
}

/// Separate profile purpose. The constructor is used only after actual strict
/// raw-response publication, never by reading memory or a loaded row accessor.
pub(in crate::thread_feed::read) struct ProfileIntake {
    action: Action,
    ticket: Arc<Ticket>,
    ids: Vec<String>,
    candidates: profiles::network::Candidates,
}
pub(in crate::thread_feed::read) struct ProfileLineage {
    origin: Arc<()>,
    owner: Arc<crate::bootstrap::ReadOwner>,
    cursor_owner: Arc<crate::realtime::activity_counts::CursorOwner>,
    client: SlackClient,
    epoch: u64,
    team: String,
    sequence: u64,
    identity: ReadIdentity,
    retired: AtomicBool,
}
impl ProfileLineage {
    pub(in crate::thread_feed::read) fn current(&self, s: &ShellState) -> bool {
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        // Profile ownership is independent of the completed body-read window
        // and thread-pane generation. Opening a profile intentionally closes
        // that pane. Exact selected author/member checks remain in profiles.
        let current = s.signed_in
            && s.core.account_epoch == self.epoch
            && s.core.active_team.as_ref() == Some(&self.team)
            && Arc::ptr_eq(&self.origin, &s.thread_feed_reads.incarnation)
            && self.owner.current(&s.core)
            && self.cursor_owner.current(&s.core)
            && self.client.same_configuration(&s.core.client)
            && self.identity.open()
            && connection(s, &self.team).is_some_and(|c| self.identity.is_for(c))
            && s.core.threads_view.matches_scope(self.epoch, &self.team)
            && s.core.threads_view.loaded
            && !s.core.threads_view.loading
            && s.core.threads_view.load_seq == self.sequence
            && s.thread_feed_reads.owned_sequence == Some(self.sequence);
        if !current {
            self.retired.store(true, Ordering::Release);
        }
        current
    }
}
impl ProfileIntake {
    pub(in crate::thread_feed::read) fn details(
        self,
        s: &ShellState,
    ) -> Option<(Source, profiles::network::Candidates, Arc<ProfileLineage>)> {
        if !self.ticket.watch.current()
            || !self.action.scope.window_current(s, &self.action.key)
            || !self.action.scope.windows[&self.action.key]
                .phase
                .lock()
                .is_ok_and(|p| p.done.as_ref() == Some(&self.ids))
            || !self
                .candidates
                .matches(&self.action.source.team, &self.action.key)
        {
            return None;
        }
        let source = self.action.source.clone();
        let lineage = Arc::new(ProfileLineage {
            origin: source.root.clone(),
            owner: source.owner.clone(),
            cursor_owner: source.cursor_owner.clone(),
            client: source.client.clone(),
            epoch: source.epoch,
            team: source.team.clone(),
            sequence: source.stamp.sequence,
            identity: self.action.scope.identity.clone(),
            retired: AtomicBool::new(false),
        });
        lineage
            .current(s)
            .then_some((source, self.candidates, lineage))
    }
}
