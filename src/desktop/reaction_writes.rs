//! Stable exclusion/outcome custody only; never action authority.
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
mod legacy;
mod shapes;
mod trace;
pub(crate) use legacy::toggle as legacy_toggle;
pub(crate) use shapes::known_reply;
pub(crate) use trace::{Trace, classify};
pub(crate) fn registry(s: &crate::state::ShellState) -> Registry {
    crate::bootstrap::search_message_pages::reaction_actions::physical_registry(s)
}
#[derive(Clone)]
pub(crate) struct Actor {
    pub account: String,
    pub team: String,
    pub user: String,
}
// Physical identity is the authenticated Slack principal, independent of local
// account-label materialization. Account remains metadata for original-owner
// publication checks; equality here NEVER authorizes publication or a request.
impl PartialEq for Actor {
    fn eq(&self, other: &Self) -> bool {
        (self.team.as_str(), self.user.as_str()) == (other.team.as_str(), other.user.as_str())
    }
}
impl Eq for Actor {}
impl PartialOrd for Actor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Actor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.team, &self.user).cmp(&(&other.team, &other.user))
    }
}
static NEXT_RECEIPT: AtomicU64 = AtomicU64::new(1);
fn receipt_id() -> Option<u64> {
    NEXT_RECEIPT
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
        .ok()
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub actor: Actor,
    pub channel: String,
    pub ts: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    NotSent,
    Acknowledged,
    Rejected,
    Unknown,
}
#[derive(Clone)]
pub(crate) struct Receipt {
    pub id: u64,
    pub name: String,
    pub adding: bool,
    pub outcome: Outcome,
}
#[derive(Default)]
struct Slot {
    busy: AtomicBool,
    receipt: Mutex<Option<Receipt>>,
}
#[derive(Default)]
pub(crate) struct Bank {
    slots: BTreeMap<Key, Arc<Slot>>,
    // None is an unrepresentable advertised deadline: fail closed.
    pub(crate) cooldowns: BTreeMap<Actor, Option<Instant>>,
    dirty: bool,
}
#[derive(Clone)]
pub(crate) struct Registry(pub(crate) Vec<Arc<Mutex<Bank>>>);
impl Default for Registry {
    fn default() -> Self {
        Self(vec![Arc::new(Mutex::new(Bank::default()))])
    }
}
fn held_until(until: &Option<Instant>) -> bool {
    until.is_none_or(|u| u > Instant::now())
}
fn record_cooldown(banks: &[Arc<Mutex<Bank>>], actor: &Actor, seconds: Option<u64>) {
    let Some(seconds) = seconds else { return };
    let until = Instant::now().checked_add(Duration::from_secs(seconds.max(1)));
    for bank in banks {
        if let Ok(mut b) = bank.lock() {
            b.cooldowns
                .entry(actor.clone())
                .and_modify(|old| {
                    *old = match (*old, until) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        _ => None,
                    };
                })
                .or_insert(until);
            b.dirty = true;
        }
    }
}
impl Registry {
    pub(crate) fn permanent(&self, actor: &Actor) {
        for bank in &self.0 {
            if let Ok(mut bank) = bank.lock() {
                bank.cooldowns.insert(actor.clone(), None);
                bank.dirty = true;
            }
        }
    }
    pub(crate) fn pending(&self, key: &Key) -> bool {
        self.0.iter().any(|b| {
            b.lock().map_or(true, |b| {
                b.slots
                    .get(key)
                    .is_some_and(|s| s.busy.load(Ordering::Acquire))
            })
        })
    }
    pub(crate) fn cooldown(&self, actor: &Actor, seconds: Option<u64>) {
        record_cooldown(&self.0, actor, seconds);
    }
    pub(crate) fn take_dirty(&self) -> bool {
        self.0.iter().fold(false, |changed, b| {
            b.lock().map_or(changed, |mut b| {
                let before = b.cooldowns.len();
                b.cooldowns
                    .retain(|_, deadline| deadline.is_none_or(|d| d > Instant::now()));
                let expired = before != b.cooldowns.len();
                std::mem::take(&mut b.dirty) || expired || changed
            })
        })
    }
    pub(crate) fn cooling(&self, actor: &Actor) -> bool {
        self.0.iter().any(|b| {
            b.lock()
                .map_or(true, |b| b.cooldowns.get(actor).is_some_and(held_until))
        })
    }
    pub(crate) fn blocked(&self, key: &Key) -> bool {
        self.0.iter().any(|bank| {
            bank.lock().map_or(true, |b| {
                b.cooldowns.get(&key.actor).is_some_and(held_until)
                    || b.slots.get(key).is_some_and(|s| {
                        s.busy.load(Ordering::Acquire)
                            || s.receipt.lock().map_or(true, |r| {
                                r.as_ref().is_some_and(|r| r.outcome == Outcome::Unknown)
                            })
                    })
            })
        })
    }
    pub(crate) fn claim(&self, key: &Key) -> Option<Reservation> {
        if self.blocked(key) {
            return None;
        }
        let mut candidate = None;
        for bank in &self.0 {
            if let Some(slot) = bank.lock().ok()?.slots.get(key) {
                candidate = Some(slot.clone());
                break;
            }
        }
        let slot = if let Some(s) = candidate {
            s
        } else {
            let mut bank = self.0.first()?.lock().ok()?;
            if bank.slots.len() >= 256 {
                // Only completed known history can make room. Pending, poisoned
                // and Unknown outcomes are never evicted for capacity.
                let old = bank
                    .slots
                    .iter()
                    .find(|(_, s)| {
                        !s.busy.load(Ordering::Acquire)
                            && Arc::strong_count(s) == 1
                            && s.receipt.lock().is_ok_and(|r| {
                                r.as_ref().is_some_and(|r| r.outcome != Outcome::Unknown)
                            })
                    })
                    .map(|(k, _)| k.clone())?;
                bank.slots.remove(&old);
            }
            bank.slots.entry(key.clone()).or_default().clone()
        };
        let id = receipt_id()?;
        slot.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(Reservation {
            id,
            slot,
            banks: self.0.clone(),
            actor: key.actor.clone(),
            finished: false,
        })
    }
    pub(crate) fn receipt(&self, key: &Key) -> Option<Receipt> {
        self.0
            .iter()
            .filter_map(|b| b.lock().ok()?.slots.get(key)?.receipt.lock().ok()?.clone())
            .max_by_key(|r| r.id)
    }
}
/// Nonclone; owned before first poll and drained by the root driver.
pub(crate) struct Reservation {
    id: u64,
    slot: Arc<Slot>,
    banks: Vec<Arc<Mutex<Bank>>>,
    actor: Actor,
    finished: bool,
}
impl Reservation {
    pub(crate) fn id(&self) -> u64 {
        self.id
    }
    pub(crate) fn held(&self) -> bool {
        self.slot.busy.load(Ordering::Acquire)
    }
    pub(crate) fn cooldown(&self, seconds: Option<u64>) {
        record_cooldown(&self.banks, &self.actor, seconds);
    }
    pub(crate) fn finish(mut self, mut receipt: Receipt) {
        receipt.id = self.id;
        if let Ok(mut r) = self.slot.receipt.lock() {
            // An already-recorded ambiguous operation cannot be downgraded by
            // a later presentation/Check receipt, even under a malformed caller.
            if r.as_ref()
                .is_some_and(|old| old.outcome == Outcome::Unknown)
            {
                receipt.outcome = Outcome::Unknown;
            }
            *r = Some(receipt);
            self.finished = true;
        }
        for bank in &self.banks {
            if let Ok(mut b) = bank.lock() {
                b.dirty = true;
            }
        }
        if self.finished {
            self.slot.busy.store(false, Ordering::Release);
        }
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        // Unexpected cancellation cannot manufacture an empty physical slot.
        if self.finished {
            self.slot.busy.store(false, Ordering::Release);
        }
    }
}
