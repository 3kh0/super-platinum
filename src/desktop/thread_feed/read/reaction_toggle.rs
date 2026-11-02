//! Original Root existing-pill purpose. Data/physical banks confer no old UI grant.
use super::*;
use crate::reaction_writes::{Actor, Key as PhysicalKey, Outcome, Registry, Reservation, Trace};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Mutex, atomic::AtomicU64},
};
use super_platinum_core::slack::{
    events::RtEvent,
    models::{Message, Reaction, ThreadViewItem},
    realtime::{ReadBarrier, ReadIdentity, ReadWatch},
};
pub(crate) mod first_add;
mod roster;
mod run;
pub(super) use roster::Sink;
pub(crate) use roster::{Action, PreparedRow};
use roster::{Key, Scope};
pub(super) fn publish(source: &Source, s: &mut ShellState, sink: &Sink) {
    roster::publish(source, s, sink.take());
}
pub(super) async fn get_view(
    transport: Arc<Transport>,
    request: super_platinum_core::slack::PreparedRequest,
    sink: Sink,
) -> Result<Value, Error> {
    let raw = run::get_view(transport, request).await?;
    sink.observe(&raw);
    Ok(raw)
}
#[cfg(test)]
pub(crate) mod test_io;
#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;

static NEXT: AtomicU64 = AtomicU64::new(1);
fn next() -> Option<u64> {
    NEXT.fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
        .ok()
}
struct Applied {
    generation: u64,
    barrier: ReadBarrier,
}
struct Ingress {
    generation: u64,
    identity: ReadIdentity,
    initial: bool,
    event: bool,
}
#[derive(Default)]
pub(crate) struct State {
    first_add: first_add::State,
    scope: Option<Arc<Scope>>,
    active: Option<Arc<Intent>>,
    applied: BTreeMap<String, Applied>,
    ingress: BTreeMap<String, Ingress>,
    display: Option<Display>,
    checking: Option<Arc<ReadTask>>,
    check_notice: Option<(u64, Key, String)>,
}
struct Display {
    scope: Arc<Scope>,
    key: Key,
    reactions: Vec<Reaction>,
}
struct Intent {
    id: u64,
    action: Action,
    ledger: Registry,
    //0 catalog/reads;1 membership/selection;2 claim;3 write start;4 terminal.
    phase: std::sync::atomic::AtomicU8,
    notice: Mutex<String>,
}
impl Intent {
    fn current(&self, s: &ShellState) -> bool {
        self.action.current(s)
            && s.thread_feed_reads
                .reaction_toggle
                .active
                .as_ref()
                .is_some_and(|p| p.id == self.id)
    }
    fn read_current(&self, s: &ShellState) -> bool {
        self.action.scope.read_current(s, &self.action.key)
            && s.overlay.is_none()
            && s.thread_feed_reads
                .reaction_toggle
                .active
                .as_ref()
                .is_some_and(|p| p.id == self.id)
    }
    fn notice(&self, text: &str) {
        if let Ok(mut n) = self.notice.lock() {
            *n = text.into();
        }
    }
}
fn wake(mut s: Signal<ShellState>) {
    drop(s.try_write());
}
pub(crate) fn action(
    source: &Source,
    s: &ShellState,
    item: &ThreadViewItem,
    name: &str,
) -> Option<Action> {
    Action::capture(source, s, item, name)
}
pub(crate) fn prepare<'a>(
    source: &Source,
    s: &'a ShellState,
    item: &ThreadViewItem,
) -> Option<PreparedRow<'a>> {
    PreparedRow::capture(source, s, item)
}
pub(crate) fn start(mut state: Signal<ShellState>, action: Action) {
    let work = {
        let mut s = state.write();
        if !action.current(&s)
            || !action.scope.caught_up(&s)
            || s.thread_feed_reads
                .reaction_toggle
                .active
                .as_ref()
                .is_some_and(|p| p.phase.load(Ordering::Acquire) < 4)
        {
            return;
        }
        let ledger = crate::reaction_writes::registry(&s);
        let Some(reservation) = ledger.claim(&action.physical()) else {
            return;
        };
        let Some(id) = next() else {
            reservation.finish(crate::reaction_writes::Receipt {
                id: 0,
                name: action.name.clone(),
                adding: false,
                outcome: Outcome::NotSent,
            });
            return;
        };
        let p = Arc::new(Intent {
            id,
            action,
            ledger,
            phase: std::sync::atomic::AtomicU8::new(0),
            notice: Mutex::new("Checking reaction…".into()),
        });
        s.thread_feed_reads.reaction_toggle.active = Some(p.clone());
        s.thread_feed_reads.reaction_toggle.display = None;
        s.thread_feed_reads.reaction_toggle.check_notice = None;
        (p, reservation)
    };
    dioxus::core::spawn_forever(run::toggle(state, work.0, work.1));
}
struct ReadTask {
    id: u64,
    scope: Arc<Scope>,
    key: Key,
    ledger: Registry,
    mutation: Option<Arc<Intent>>,
}
impl ReadTask {
    fn current(&self, s: &ShellState) -> bool {
        if let Some(p) = &self.mutation {
            p.current(s)
        } else {
            self.scope.read_current(s, &self.key)
                && s.thread_feed_reads
                    .reaction_toggle
                    .checking
                    .as_ref()
                    .is_some_and(|p| p.id == self.id)
        }
    }
    fn physical(&self) -> PhysicalKey {
        self.scope.physical(&self.key)
    }
}
#[derive(Clone)]
pub(crate) struct Check {
    scope: Arc<Scope>,
    key: Key,
    receipt: u64,
}
impl PartialEq for Check {
    fn eq(&self, b: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &b.scope) && self.key == b.key && self.receipt == b.receipt
    }
}
impl Check {
    pub(crate) fn start(&self, mut state: Signal<ShellState>) {
        let p = {
            let mut s = state.write();
            let ledger = crate::reaction_writes::registry(&s);
            let physical = self.scope.physical(&self.key);
            if !self.scope.read_current(&s, &self.key)
                || !self.scope.caught_up(&s)
                || ledger.cooling(&physical.actor)
                || ledger.pending(&physical)
                || s.thread_feed_reads.reaction_toggle.checking.is_some()
                || ledger
                    .receipt(&physical)
                    .is_none_or(|r| r.id != self.receipt)
            {
                return;
            }
            let Some(id) = next() else { return };
            let p = Arc::new(ReadTask {
                id,
                scope: self.scope.clone(),
                key: self.key.clone(),
                ledger,
                mutation: None,
            });
            s.thread_feed_reads.reaction_toggle.checking = Some(p.clone());
            p
        };
        dioxus::core::spawn_forever(run::check(state, p));
    }
    pub(crate) fn id(&self) -> String {
        format!(
            "thread-root-reaction-check-{}-{}",
            self.scope.id, self.receipt
        )
    }
}
pub(crate) struct Status {
    pub notice: String,
    pub pending: bool,
    pub check: Option<Check>,
}
pub(crate) fn status(s: &ShellState, item: &ThreadViewItem) -> Option<Status> {
    let st = &s.thread_feed_reads.reaction_toggle;
    let scope = st.scope.as_ref()?;
    let key = roster::key(item)?;
    if !scope.read_current(s, &key) {
        return None;
    }
    let ledger = crate::reaction_writes::registry(s);
    let physical = scope.physical(&key);
    let active = st
        .active
        .as_ref()
        .filter(|p| p.action.key == key && p.read_current(s));
    let checking = st
        .checking
        .as_ref()
        .is_some_and(|p| p.key == key && p.current(s));
    let pending = checking
        || active.is_some_and(|p| p.phase.load(Ordering::Acquire) < 4)
        || ledger.pending(&physical);
    let notice = if checking {
        "Checking current reactions…".into()
    } else if let Some((_, _, n)) = st
        .check_notice
        .as_ref()
        .filter(|(id, k, _)| *id == scope.id && k == &key)
    {
        n.clone()
    } else if let Some(p) = active {
        p.notice.lock().ok()?.clone()
    } else if ledger.blocked(&physical) {
        "An earlier reaction is pending, unconfirmed or cooling down. A check never resends it."
            .into()
    } else {
        return None;
    };
    let check = if !pending && !ledger.cooling(&physical.actor) && scope.caught_up(s) {
        ledger.receipt(&physical).map(|r| Check {
            scope: scope.clone(),
            key,
            receipt: r.id,
        })
    } else {
        None
    };
    Some(Status {
        notice,
        pending,
        check,
    })
}
pub(crate) fn displayed(s: &ShellState, item: &ThreadViewItem) -> Option<Vec<Reaction>> {
    let d = s.thread_feed_reads.reaction_toggle.display.as_ref()?;
    (roster::key(item).as_ref() == Some(&d.key) && d.scope.read_current(s, &d.key))
        .then(|| d.reactions.clone())
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
pub(crate) fn connection_changed(s: &mut ShellState, team: &str) {
    let current = connection(s, team).map(|c| c.read_identity());
    let generation = s
        .core
        .workspaces
        .get(team)
        .map(|w| w.rt_generation)
        .unwrap_or(0);
    let st = &mut s.thread_feed_reads.reaction_toggle;
    first_add::connection_changed(&mut st.first_add, team);
    st.applied.remove(team);
    st.ingress.remove(team);
    if let Some(scope) = st.scope.as_ref().filter(|p| p.source.team == team) {
        // Cold data without any previous socket can bind only at the actual
        // first accepted barrier. An existing connection never rebinds.
        if scope.bound.lock().map_or(true, |v| v.is_some()) {
            scope.retired.store(true, Ordering::Release);
        }
    }
    st.display = None;
    if let Some(identity) = current {
        st.ingress.insert(
            team.into(),
            Ingress {
                generation,
                identity,
                initial: true,
                event: false,
            },
        );
    }
}
pub(crate) fn event(s: &mut ShellState, team: &str, generation: u64, event: &RtEvent) {
    if s.core
        .workspaces
        .get(team)
        .is_none_or(|w| w.rt_generation != generation)
        || connection(s, team).is_none()
    {
        return;
    }
    let benign = matches!(
        event,
        RtEvent::PresenceChange { .. } | RtEvent::UserTyping { .. }
    ) || matches!(event,RtEvent::Unknown(raw) if matches!(raw.kind.as_str(),"hello"|"pong"));
    if benign {
        return;
    }
    let st = &mut s.thread_feed_reads.reaction_toggle;
    first_add::event(&mut st.first_add, team, event);
    if let Some(i) = st
        .ingress
        .get_mut(team)
        .filter(|i| i.generation == generation)
    {
        i.event = true;
    }
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
    st.display = None;
}
pub(crate) fn accept_barrier(
    s: &mut ShellState,
    team: &str,
    generation: u64,
    barrier: ReadBarrier,
) {
    let Some(c) = connection(s, team) else { return };
    if !barrier.belongs_to(c)
        || s.core
            .workspaces
            .get(team)
            .is_none_or(|w| w.rt_generation != generation)
    {
        return;
    }
    let same = s
        .thread_feed_reads
        .reaction_toggle
        .ingress
        .get(team)
        .is_some_and(|i| i.generation == generation && i.identity.is_for(c));
    if !same {
        return;
    }
    let identity = c.read_identity();
    let first_add_identity = c.read_identity();
    let st = &mut s.thread_feed_reads.reaction_toggle;
    let Some(i) = st.ingress.get_mut(team) else {
        return;
    };
    let gap = !i.initial && !i.event;
    i.initial = false;
    i.event = false;
    first_add::barrier(&mut st.first_add, team, generation, first_add_identity, gap);
    if let Some(scope) = st.scope.as_ref().filter(|p| p.source.team == team) {
        if gap {
            scope.retired.store(true, Ordering::Release);
            st.display = None;
        }
        if let Ok(mut bound) = scope.bound.lock() {
            if bound.is_none() {
                *bound = Some((generation, identity));
            }
        }
    }
    st.applied.insert(
        team.into(),
        Applied {
            generation,
            barrier,
        },
    );
}
