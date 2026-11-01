//! Native Copy-only intake/data/control lifecycle. No pane or Source-ready grant.
use super::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Mutex,
        atomic::{AtomicU8, AtomicU64},
    },
};
use super_platinum_core::{slack::realtime::ReadWatch, thread_copy_link::FreshRootLink};
mod control;
mod read;
mod registry;
mod roster;
pub(crate) use control::Config;
#[cfg(test)]
pub(crate) mod test_io;
#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
pub(crate) use registry::carry;
use roster::Scope;
pub(crate) use roster::{Action, Demotion, after_event, before_event};

static NEXT: AtomicU64 = AtomicU64::new(1);
pub(crate) fn next_mount() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}
#[derive(Default)]
pub(crate) struct State {
    scope: Option<Arc<Scope>>,
    intent: Option<Arc<Intent>>,
    registry: registry::Registry,
    layout: bool,
    #[cfg(test)]
    pub(crate) io: Option<Arc<test_io::Io>>,
}
struct Ready {
    link: FreshRootLink,
    team: read::Team,
}
// Inert presentation only: this cannot construct intake, data or a gesture.
pub(crate) fn fixture(s: &mut ShellState) {
    s.thread_feed_reads.copy_link.layout = true;
}
pub(crate) fn layout(s: &ShellState) -> bool {
    s.thread_feed_reads.copy_link.layout
}
struct Intent {
    id: u64,
    action: Action,
    watch: Arc<ReadWatch>,
    actor: registry::Actor,
    ledger: registry::Registry,
    phase: AtomicU8, // 0 preparing, 1 ready, 2 issued, 3 terminal.
    ready: Mutex<Option<Ready>>,
    notice: Mutex<String>,
    dirty: AtomicBool,
    closed: AtomicBool,
    mounted: AtomicU64,
    native_ready: AtomicBool,
    review: bool,
}
impl Intent {
    fn current(&self, s: &ShellState) -> bool {
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        let ok = self.action.current(s)
            && s.thread_feed_reads
                .copy_link
                .intent
                .as_ref()
                .is_some_and(|p| p.id == self.id);
        if !ok {
            self.closed.store(true, Ordering::Release)
        }
        ok
    }
    fn data_current(&self, s: &ShellState) -> bool {
        self.current(s)
            && self.watch.current()
            && !self.ledger.cooling(&self.actor)
            && self.ready.lock().is_ok_and(|r| {
                r.as_ref().is_some_and(|r| {
                    r.team.current(&self.action.scope.source, s)
                        && r.link.team() == self.action.scope.source.team
                        && r.link.channel() == self.action.key.0
                        && r.link.timestamp() == self.action.key.1
                })
            })
    }
    fn notice(&self, text: &str) {
        if let Ok(mut n) = self.notice.lock() {
            *n = text.into()
        }
        self.dirty.store(true, Ordering::Release);
    }
    fn fail(&self, text: &str) {
        self.notice(text);
        self.phase.store(3, Ordering::Release);
        self.native_ready.store(false, Ordering::Release);
    }
}
fn wake(mut s: Signal<ShellState>) {
    drop(s.try_write());
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
    // Only the actual original native ticket can seal a Copy roster after
    // successful Core publication. Source.stamp is deliberately pre-read.
    let original = ticket.owns(s)
        && !ticket.settled.load(Ordering::Acquire)
        && s.thread_feed_reads.owned_sequence == Some(ticket.request.sequence)
        && s.core.threads_view.load_seq == ticket.request.sequence;
    s.thread_feed_reads.copy_link.scope = if original {
        Scope::capture(source, s, incoming.map(|c| c.0), carry.map(|c| c.0)).map(Arc::new)
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
pub(crate) fn prepare(mut state: Signal<ShellState>, action: Action) {
    let p = {
        let mut s = state.write();
        if !action.current(&s) {
            return;
        }
        if s.thread_feed_reads
            .copy_link
            .intent
            .as_ref()
            .is_some_and(|p| {
                p.action.key == action.key
                    && Arc::ptr_eq(&p.action.scope, &action.scope)
                    && p.current(&s)
                    && p.phase.load(Ordering::Acquire) != 3
            })
        {
            return;
        }
        let Some(watch) = action.arm(&s) else {
            s.show_toast("Refresh Threads before preparing this link.");
            return;
        };
        let actor = registry::Actor(
            s.core.active_account.clone().unwrap_or_default(),
            action.scope.source.team.clone(),
            action.scope.source.session.user_id.clone(),
        );
        let ledger = s.thread_feed_reads.copy_link.registry.clone();
        let review = ledger.blocked();
        let p = Arc::new(Intent {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            action,
            watch,
            actor,
            ledger,
            phase: AtomicU8::new(if review { 3 } else { 0 }),
            ready: Mutex::new(None),
            notice: Mutex::new(if review {
                "A previous clipboard update is still pending.".into()
            } else {
                "Preparing message link…".into()
            }),
            dirty: AtomicBool::new(true),
            closed: AtomicBool::new(false),
            mounted: AtomicU64::new(0),
            native_ready: AtomicBool::new(false),
            review,
        });
        s.thread_feed_reads.copy_link.intent = Some(p.clone());
        p
    };
    if !p.review {
        dioxus::core::spawn_forever(read::prepare(state, p));
    }
}
pub(crate) struct Presentation {
    pub id: u64,
    pub scope: String,
    pub preparing: bool,
    pub ready: bool,
    pub copying: bool,
    pub native_ready: bool,
    pub notice: String,
}
pub(crate) fn presentation(s: &ShellState, action: &Action) -> Option<Presentation> {
    let p = s.thread_feed_reads.copy_link.intent.as_ref()?;
    if !Arc::ptr_eq(&p.action.scope, &action.scope) || p.action.key != action.key {
        return None;
    }
    let phase = p.phase.load(Ordering::Acquire);
    let current = p.current(s);
    let data = current && p.data_current(s);
    let notice = if p.review {
        if p.ledger.unknown() {
            "A previous clipboard update is unconfirmed. No link was copied again.".into()
        } else if p.ledger.blocked() {
            "A previous clipboard update is still pending.".into()
        } else {
            "The previous clipboard update finished. Prepare this link again.".into()
        }
    } else if !current || phase == 1 && !data {
        "This message changed. Prepare its link again.".into()
    } else {
        p.notice
            .lock()
            .map(|n| n.clone())
            .unwrap_or_else(|_| "Link status is unavailable.".into())
    };
    Some(Presentation {
        id: p.id,
        scope: p.action.scope.id.to_string(),
        preparing: current && phase == 0,
        ready: data && matches!(phase, 1 | 2),
        copying: phase == 2,
        native_ready: p.native_ready.load(Ordering::Acquire),
        notice,
    })
}
pub(crate) fn mounted(state: Signal<ShellState>, id: u64, mount: u64) {
    control::start(state, id, mount);
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let state = &s.thread_feed_reads.copy_link;
    state.registry.take_dirty()
        | state
            .intent
            .as_ref()
            .is_some_and(|p| p.dirty.swap(false, Ordering::AcqRel))
}
