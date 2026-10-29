//! Physical exclusion only. No credentials, query, preference or UI authority.
use crate::state::ShellState;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    account: String,
    team: String,
    user: String,
}
impl Key {
    fn capture(s: &ShellState) -> Option<Self> {
        let team = s.core.active_team.clone()?;
        Some(Self {
            account: s.core.active_account.clone()?,
            user: s.core.workspaces.get(&team)?.self_user_id.clone(),
            team,
        })
    }
}
type Slots = BTreeMap<Key, Vec<Arc<AtomicBool>>>;
/// Send is required by the actual background ShellState constructor. Banks
/// preserve every incoming occupied/poisoned registry across shell replacement.
pub(crate) struct Registry(Vec<Arc<Mutex<Slots>>>);
impl Default for Registry {
    fn default() -> Self {
        Self(vec![Arc::new(Mutex::new(BTreeMap::new()))])
    }
}
impl Registry {
    pub(super) fn busy(&self, s: &ShellState) -> bool {
        let Some(key) = Key::capture(s) else {
            return true;
        };
        self.0.iter().any(|bank| {
            bank.lock().map_or(true, |map| {
                map.get(&key)
                    .is_some_and(|slots| slots.iter().any(|s| s.load(Ordering::Acquire)))
            })
        })
    }
    pub(super) fn claim(&self, s: &ShellState) -> Option<Reservation> {
        let key = Key::capture(s)?;
        let mut candidate = None;
        for bank in &self.0 {
            let map = bank.lock().ok()?;
            if let Some(slots) = map.get(&key) {
                if slots.iter().any(|s| s.load(Ordering::Acquire)) {
                    return None;
                }
                if candidate.is_none() {
                    candidate = slots.first().cloned();
                }
            }
        }
        let slot = if let Some(slot) = candidate {
            slot
        } else {
            let mut map = self.0.first()?.lock().ok()?;
            let slots = map.entry(key).or_default();
            if let Some(slot) = slots.first() {
                slot.clone()
            } else {
                let slot = Arc::new(AtomicBool::new(false));
                slots.push(slot.clone());
                slot
            }
        };
        slot.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(Reservation(slot))
    }
}
/// Unique ownership starts synchronously, before the driver is first polled.
/// The private root driver drains the bounded request and exposes no cancel handle.
pub(super) struct Reservation(Arc<AtomicBool>);
impl Reservation {
    pub(super) fn held(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
pub(crate) fn carry(previous: &mut ShellState, loaded: &mut ShellState) {
    let mut carried = std::mem::take(&mut previous.search_file_save_registry);
    // Fresh shells normally have no slots. Merge rather than discard any
    // independently occupied incoming slot; every draining token still counts.
    let incoming = std::mem::take(&mut loaded.search_file_save_registry);
    carried.0.extend(incoming.0);
    loaded.search_file_save_registry = carried;
}
