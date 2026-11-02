//! Isolated Search reaction purpose. Indexed/shared models never grant writes.
use super::*;
use dioxus::prelude::*;
use serde_json::Value;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use super_platinum_core::{
    composer_emoji::preferences::Preferences,
    search_reaction::FreshPolicy,
    slack::{Error, realtime::ReadWatch},
};
mod owner;
mod registry;
mod run;
#[cfg(test)]
pub(super) mod test_io;
pub(crate) use owner::accept_barrier;
use owner::{Applied, Owner};
pub(crate) use registry::carry;

static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
pub(crate) struct State {
    applied: BTreeMap<String, Applied>,
    picker: Option<Arc<Picker>>,
    display: Option<Display>,
    registry: registry::Registry,
    // Offline layout only; no Owner, watch, policy, reservation or request.
    fixture: Option<bool>,
    #[cfg(test)]
    pub(super) io: Option<Arc<test_io::Io>>,
}
impl State {
    pub(crate) fn retire_ui(&mut self) {
        self.picker = None;
        self.display = None;
        self.fixture = None;
    }
}
pub(crate) fn physical_registry(s: &ShellState) -> registry::Registry {
    s.search_reaction_actions.registry.clone()
}
struct Picker {
    id: u64,
    owner: Owner,
    watch: Arc<ReadWatch>,
    // 0 policy IO; 1 choice; 2 membership IO; 3 issued; 4 terminal.
    phase: AtomicU8,
    policy: Mutex<Option<FreshPolicy>>,
    notice: Mutex<Option<String>>,
    checking: AtomicBool,
    dirty: AtomicBool,
    ledger: registry::Registry,
    review: bool,
}
struct Display {
    owner: Arc<Picker>,
    watch: Arc<ReadWatch>,
    reactions: Vec<super_platinum_core::slack::models::Reaction>,
}
struct Observation {
    loaded: Arc<crate::emoji_catalog::Loaded>,
    preferences: Preferences,
    requested: String,
}
impl Observation {
    fn capture(
        s: &ShellState,
        p: &Picker,
        picked: &crate::emoji_catalog::Selection,
    ) -> Option<Self> {
        let loaded = crate::emoji_catalog::current(s)?;
        let prefs = &s
            .core
            .workspaces
            .get(&p.owner.key.actor.team)?
            .emoji_preferences;
        if !loaded.current(s)
            || !Arc::ptr_eq(&loaded, &picked.loaded)
            || !prefs.loaded()
            || !loaded.picker.iter().any(|c| c.name == picked.name)
        {
            return None;
        }
        Some(Self {
            loaded,
            preferences: prefs.clone(),
            requested: picked.name.clone(),
        })
    }
    fn current(&self, s: &ShellState, p: &Picker) -> bool {
        self.loaded.current(s)
            && crate::emoji_catalog::current(s).is_some_and(|c| Arc::ptr_eq(&c, &self.loaded))
            && s.core
                .workspaces
                .get(&p.owner.key.actor.team)
                .is_some_and(|w| {
                    w.emoji_preferences == self.preferences && w.emoji_preferences.loaded()
                })
    }
}
impl Picker {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && s.search_reaction_actions
                .picker
                .as_ref()
                .is_some_and(|p| p.id == self.id)
    }
    fn readable(&self, s: &ShellState) -> bool {
        self.current(s) && self.watch.current()
    }
    fn notice(&self, text: &str) {
        if let Ok(mut n) = self.notice.lock() {
            *n = Some(text.into())
        }
        self.dirty.store(true, Ordering::Release);
    }
    fn done(&self, text: &str) {
        self.notice(text);
        self.phase.store(4, Ordering::Release);
    }
}
fn picker(s: &ShellState, id: u64) -> Option<Arc<Picker>> {
    s.search_reaction_actions
        .picker
        .as_ref()
        .filter(|p| p.id == id && p.owner.current(s))
        .cloned()
}
pub(crate) fn available(s: &ShellState, index: usize) -> bool {
    if !candidate(s, index) {
        return false;
    }
    let Some(a) = s
        .core
        .active_team
        .as_ref()
        .and_then(|t| s.search_reaction_actions.applied.get(t))
    else {
        return false;
    };
    let Some(w) = s.core.workspaces.get(&a.team) else {
        return false;
    };
    let super_platinum_core::state::RealtimeStatus::Connected(c) = &w.rt else {
        return false;
    };
    a.generation == w.rt_generation
        && a.barrier.current_for(c)
        && s.search_reaction_actions.picker.is_none()
}
/// An original applied-marker/row cue only. This never grants a mutation.
pub(crate) fn candidate(s: &ShellState, index: usize) -> bool {
    let Some(row) = RowSource::capture(s, index) else {
        return false;
    };
    let Some(hit) = row.resolve(s) else {
        return false;
    };
    hit.channel_id.starts_with('C')
        && hit.thread_ts.is_none()
        && s.core
            .active_team
            .as_ref()
            .is_some_and(|t| s.search_reaction_actions.applied.contains_key(t))
}
pub(crate) fn open(mut state: Signal<ShellState>, source: RowSource) {
    let p = {
        let mut s = state.write();
        if s.search_reaction_actions.picker.is_some() {
            return;
        }
        let Some((owner, watch)) = Owner::capture(&s, source) else {
            s.show_toast("Reactions are unavailable for this result right now.");
            return;
        };
        let review = s.search_reaction_actions.registry.blocked(&owner.key);
        let p = Arc::new(Picker {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            owner,
            watch,
            phase: AtomicU8::new(0),
            policy: Mutex::new(None),
            notice: Mutex::new(Some("Checking this message…".into())),
            checking: AtomicBool::new(false),
            dirty: AtomicBool::new(true),
            ledger: s.search_reaction_actions.registry.clone(),
            review,
        });
        if p.review {
            p.done("Check reads current reactions without sending a reaction.");
        }
        s.search_reaction_actions.picker = Some(p.clone());
        p
    };
    if p.phase.load(Ordering::Acquire) == 0 {
        dioxus::core::spawn_forever(run::prepare(state, p));
    }
}
pub(crate) fn close(mut state: Signal<ShellState>, id: u64) {
    let mut s = state.write();
    if id == 0 {
        s.search_reaction_actions.fixture = None;
        return;
    }
    if s.search_reaction_actions
        .picker
        .as_ref()
        .is_some_and(|p| p.id == id)
    {
        s.search_reaction_actions.picker = None;
    }
}
pub(crate) fn choose(
    mut state: Signal<ShellState>,
    id: u64,
    picked: crate::emoji_catalog::Selection,
) {
    let work = {
        let s = state.write();
        let Some(p) = picker(&s, id) else { return };
        if !p.readable(&s) || s.search_reaction_actions.registry.blocked(&p.owner.key) {
            return;
        }
        let Some(observation) = Observation::capture(&s, &p, &picked) else {
            return;
        };
        if !p.policy.lock().is_ok_and(|p| p.is_some())
            || p.phase
                .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return;
        }
        let Some(reservation) = s.search_reaction_actions.registry.claim(&p.owner.key) else {
            p.done("Another reaction is pending or could not be confirmed.");
            return;
        };
        p.notice("Checking your current reaction…");
        (p, observation, reservation)
    };
    dioxus::core::spawn_forever(run::send(state, work.0, work.1, work.2));
}
pub(crate) fn check(mut state: Signal<ShellState>, id: u64) {
    let p = {
        let s = state.write();
        let Some(p) = picker(&s, id) else { return };
        if p.phase.load(Ordering::Acquire) != 4 || p.checking.swap(true, Ordering::AcqRel) {
            return;
        }
        p
    };
    dioxus::core::spawn_forever(run::check(state, p));
}
pub(crate) struct Presentation {
    pub id: u64,
    pub ready: bool,
    pub terminal: bool,
    pub checking: bool,
    pub notice: Option<String>,
    pub opener: String,
}
pub(crate) fn presentation(s: &ShellState) -> Option<Presentation> {
    if s.search_reaction_actions.picker.is_none() {
        if let Some(unknown) = s.search_reaction_actions.fixture {
            return Some(Presentation {
                id: 0,
                ready: false,
                terminal: unknown,
                checking: false,
                notice: Some(
                    if unknown {
                        "The reaction could not be confirmed. Check does not resend it."
                    } else {
                        "Checking this message…"
                    }
                    .into(),
                ),
                opener: String::new(),
            });
        }
    }
    let p = s
        .search_reaction_actions
        .picker
        .as_ref()
        .filter(|p| p.owner.current(s))?;
    let phase = p.phase.load(Ordering::Acquire);
    let current = p.watch.current();
    let blocked = s.search_reaction_actions.registry.blocked(&p.owner.key);
    let mut notice = if phase == 1 && !current {
        Some("This message changed. Close and reopen the picker.".into())
    } else if phase == 1 && blocked {
        Some("Another reaction is pending or its outcome is unconfirmed. Close this picker to review the result.".into())
    } else {
        p.notice.lock().ok().and_then(|n| n.clone())
    };
    if p.review {
        let prior = if p.ledger.pending(&p.owner.key) {
            "A reaction is still pending."
        } else {
            match p.ledger.receipt(&p.owner.key).map(|r| r.outcome) {
                Some(registry::Outcome::Unknown) => {
                    "The previous reaction is unconfirmed; it will not be resent."
                }
                Some(registry::Outcome::Acknowledged) => "The previous reaction was acknowledged.",
                Some(registry::Outcome::Rejected) => "The previous reaction was rejected.",
                Some(registry::Outcome::NotSent) => "The previous reaction was not sent.",
                None => "Reaction requests are temporarily unavailable.",
            }
        };
        notice = Some(format!("{prior} {}", notice.unwrap_or_default()));
    }
    Some(Presentation {
        id: p.id,
        ready: phase == 1 && current && !blocked,
        terminal: phase == 4,
        checking: p.checking.load(Ordering::Acquire),
        notice,
        opener: format!(
            "message-search-react-{}-{}",
            p.owner.row.control.shown, p.owner.row.index
        ),
    })
}
pub(crate) fn poll(s: &mut ShellState) -> bool {
    let mut changed = s.search_reaction_actions.registry.take_dirty();
    if let Some(p) = &s.search_reaction_actions.picker {
        changed |= p.dirty.swap(false, Ordering::AcqRel);
    }
    if s.search_reaction_actions
        .picker
        .as_ref()
        .is_some_and(|p| !p.owner.current(s))
    {
        s.search_reaction_actions.picker = None;
        changed = true;
    }
    if s.search_reaction_actions
        .display
        .as_ref()
        .is_some_and(|d| !d.owner.owner.current(s) || !d.watch.current())
    {
        s.search_reaction_actions.display = None;
        changed = true;
    }
    changed
}
pub(crate) fn fresh_reactions(
    s: &ShellState,
    index: usize,
) -> Option<Vec<crate::state::ReactionVm>> {
    let d = s.search_reaction_actions.display.as_ref()?;
    if d.owner.owner.row.index != index || !d.owner.owner.current(s) || !d.watch.current() {
        return None;
    }
    let w = s.core.workspaces.get(&d.owner.owner.key.actor.team)?;
    let m = super_platinum_core::slack::models::Message {
        ts: Some(d.owner.owner.key.ts.clone()),
        reactions: d.reactions.clone(),
        ..Default::default()
    };
    Some(crate::message_vm::reaction_vms_for_reading(w, &m, &s.media))
}
fn unavailable() -> Error {
    Error::Api("search_reaction_unavailable".into())
}
pub(crate) fn fixture(s: &mut ShellState, unknown: bool) {
    s.search_reaction_actions.fixture = Some(unknown);
}
