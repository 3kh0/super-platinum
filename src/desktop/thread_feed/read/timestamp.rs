//! A separate original timestamp-navigation purpose; never a Copy or pane grant.
use super::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Mutex,
        atomic::{AtomicU8, AtomicU64},
    },
};
use super_platinum_core::slack::realtime::ReadWatch;
mod load;
mod rate;
mod roster;
pub(crate) mod target;
#[cfg(test)]
pub(crate) mod test_io;
#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
use roster::Scope;
pub(crate) use roster::{Action, after_event, before_event};
static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
pub(crate) struct State {
    scope: Option<Arc<Scope>>,
    intent: Option<Arc<Intent>>,
    rate: rate::Bank,
    cooling_ui: bool,
    layout: bool,
    #[cfg(test)]
    pub(crate) io: Option<Arc<test_io::Io>>,
}
struct Intent {
    id: u64,
    action: Action,
    watch: Arc<ReadWatch>,
    target: Option<target::Prepared>,
    actor: rate::Actor,
    bank: rate::Bank,
    phase: AtomicU8, // 0 loading, 1 invocation sealed, 2 failed, 3 retired.
    notice: Mutex<String>,
    dirty: AtomicBool,
    closed: AtomicBool,
    #[cfg(test)]
    io: Arc<test_io::Io>,
}
impl Intent {
    fn lineage(&self, s: &ShellState) -> bool {
        !self.closed.load(Ordering::Acquire)
            && self.action.route_current(s)
            && self.action.scope.current(s)
            && s.thread_feed_reads
                .timestamp
                .intent
                .as_ref()
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
    }
    fn owned(&self, s: &ShellState) -> bool {
        !self.closed.load(Ordering::Acquire)
            && self.action.base_current(s)
            && s.thread_feed_reads
                .timestamp
                .intent
                .as_ref()
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
    }
    fn current(&self, s: &ShellState) -> bool {
        self.owned(s)
            && self.phase.load(Ordering::Acquire) == 0
            && self.watch.current()
            && self.target.as_ref().is_some_and(|t| t.current(s))
            && !self.bank.cooling(&self.actor)
            && self.action.scope.source.transport.health()
                != super_platinum_core::slack::transport::Health::Offline
    }
    fn fail(&self, text: &str) {
        if let Ok(mut notice) = self.notice.lock() {
            *notice = text.into();
        }
        self.phase.store(2, Ordering::Release);
        self.dirty.store(true, Ordering::Release);
    }
}
pub(super) struct Candidates(roster::Candidates);
pub(super) struct Carry(roster::Carry);
pub(super) fn candidates(page: &ThreadsViewPage) -> Option<Candidates> {
    roster::Candidates::from_page(page).map(Candidates)
}
pub(super) fn carry_roster(s: &ShellState) -> Option<Carry> {
    Scope::carry(s).map(Carry)
}
pub(super) fn publish(
    source: &Source,
    s: &mut ShellState,
    ticket: &Arc<Ticket>,
    incoming: Option<Candidates>,
    carry: Option<Carry>,
) {
    let original = ticket.owns(s)
        && !ticket.settled.load(Ordering::Acquire)
        && s.thread_feed_reads.owned_sequence == Some(ticket.request.sequence)
        && s.core.threads_view.load_seq == ticket.request.sequence;
    s.thread_feed_reads.timestamp.scope = if original {
        Scope::capture(source, s, incoming.map(|p| p.0), carry.map(|p| p.0)).map(Arc::new)
    } else {
        None
    };
}
pub(super) fn action(
    source: &Source,
    s: &ShellState,
    item: &super_platinum_core::slack::models::ThreadViewItem,
) -> Option<Action> {
    Action::capture(source, s, item)
}
pub(crate) fn start(mut state: Signal<ShellState>, action: Action) {
    let p = {
        let Ok(mut s) = state.try_write() else { return };
        if !action.current(&s) || crate::fixture::is_fixture() {
            return;
        }
        if s.thread_feed_reads
            .timestamp
            .intent
            .as_ref()
            .is_some_and(|p| p.phase.load(Ordering::Acquire) == 0 && p.owned(&s))
        {
            return;
        }
        let Some(actor) = rate::Actor::capture(&s) else {
            return;
        };
        let bank = s.thread_feed_reads.timestamp.rate.clone();
        if bank.cooling(&actor) {
            return;
        }
        let target =
            target::Prepared::capture(&s, &action.scope.source.team, &action.key.0, &action.key.1);
        let Some(watch) = action.arm(&s) else { return };
        #[cfg(test)]
        let io = s
            .thread_feed_reads
            .timestamp
            .io
            .clone()
            .expect("timestamp test requires mandatory offline IO");
        let p = Arc::new(Intent {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            action,
            watch,
            target,
            actor,
            bank,
            phase: AtomicU8::new(0),
            notice: Mutex::new(String::new()),
            dirty: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            #[cfg(test)]
            io,
        });
        s.thread_feed_reads.timestamp.intent = Some(p.clone());
        if p.target.is_none() {
            p.fail("This message needs a current joined channel. Refresh Threads.");
            return;
        }
        p
    };
    dioxus::core::spawn_forever(load::run(state, p));
}
pub(crate) fn status(
    s: &ShellState,
    item: &super_platinum_core::slack::models::ThreadViewItem,
) -> Option<(bool, String)> {
    let p = s.thread_feed_reads.timestamp.intent.as_ref()?;
    if item.channel() != Some(&p.action.key.0)
        || item.root_ts() != Some(&p.action.key.1)
        || !s
            .thread_feed_reads
            .timestamp
            .scope
            .as_ref()
            .is_some_and(|scope| Arc::ptr_eq(scope, &p.action.scope))
        || s.main_view != MainView::Threads
        || s.overlay.is_some()
        || !p.action.scope.current(s)
    {
        return None;
    }
    let phase = p.phase.load(Ordering::Acquire);
    Some((
        phase == 0,
        p.notice.lock().map(|v| v.clone()).unwrap_or_default(),
    ))
}
pub(crate) fn cooling(s: &ShellState) -> bool {
    rate::Actor::capture(s).is_none_or(|a| s.thread_feed_reads.timestamp.rate.cooling(&a))
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let visible = s.main_view == MainView::Threads && s.overlay.is_none();
    let retire = s
        .thread_feed_reads
        .timestamp
        .intent
        .as_ref()
        // Dispatch/publication still validate the complete root witness. Idle
        // cleanup only needs lineage and ingress, without serializing messages.
        .is_some_and(|p| !p.lineage(s) || !p.watch.current());
    let mut changed = false;
    if retire {
        if let Some(p) = s.thread_feed_reads.timestamp.intent.take() {
            p.closed.store(true, Ordering::Release);
        }
        changed = visible;
    }
    let cooling = cooling(s);
    let module = &mut s.thread_feed_reads.timestamp;
    let dirty = module.rate.take_dirty()
        | module
            .intent
            .as_ref()
            .is_some_and(|p| p.dirty.swap(false, Ordering::AcqRel));
    changed |= visible && (module.cooling_ui != cooling || dirty);
    module.cooling_ui = cooling;
    changed
}
pub(crate) fn carry(previous: &mut ShellState, loaded: &mut ShellState) {
    let mut old = std::mem::take(&mut previous.thread_feed_reads.timestamp.rate);
    old.merge(std::mem::take(&mut loaded.thread_feed_reads.timestamp.rate));
    loaded.thread_feed_reads.timestamp.rate = old;
}
pub(crate) fn fixture(s: &mut ShellState) {
    s.thread_feed_reads.timestamp.layout = true;
}
pub(crate) fn layout(s: &ShellState) -> bool {
    s.thread_feed_reads.timestamp.layout
        && s.core.session.is_none()
        && s.core.transport.is_none()
        && s.core.active_account.is_none()
}
