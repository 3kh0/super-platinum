//! Stable exclusion/outcome custody only; never action authority.
use super::*;
use std::time::{Duration, Instant};
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Actor {
    pub account: String,
    pub team: String,
    pub user: String,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Key {
    pub actor: Actor,
    pub channel: String,
    pub ts: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    NotSent,
    Acknowledged,
    Rejected,
    Unknown,
}
#[derive(Clone)]
pub(super) struct Receipt {
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
struct Bank {
    slots: BTreeMap<Key, Arc<Slot>>,
    // None is an unrepresentable advertised deadline: fail closed.
    cooldowns: BTreeMap<Actor, Option<Instant>>,
    dirty: bool,
}
#[derive(Clone)]
pub(super) struct Registry(Vec<Arc<Mutex<Bank>>>);
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
    pub(super) fn pending(&self, key: &Key) -> bool {
        self.0.iter().any(|b| {
            b.lock().map_or(true, |b| {
                b.slots
                    .get(key)
                    .is_some_and(|s| s.busy.load(Ordering::Acquire))
            })
        })
    }
    pub(super) fn cooldown(&self, actor: &Actor, seconds: Option<u64>) {
        record_cooldown(&self.0, actor, seconds);
    }
    pub(super) fn take_dirty(&self) -> bool {
        self.0.iter().fold(false, |changed, b| {
            b.lock()
                .map_or(changed, |mut b| std::mem::take(&mut b.dirty) || changed)
        })
    }
    pub(super) fn cooling(&self, actor: &Actor) -> bool {
        self.0.iter().any(|b| {
            b.lock()
                .map_or(true, |b| b.cooldowns.get(actor).is_some_and(held_until))
        })
    }
    pub(super) fn blocked(&self, key: &Key) -> bool {
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
    pub(super) fn claim(&self, key: &Key) -> Option<Reservation> {
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
        slot.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(Reservation {
            slot,
            banks: self.0.clone(),
            actor: key.actor.clone(),
            finished: false,
        })
    }
    pub(super) fn receipt(&self, key: &Key) -> Option<Receipt> {
        self.0
            .iter()
            .filter_map(|b| b.lock().ok()?.slots.get(key)?.receipt.lock().ok()?.clone())
            .max_by_key(|r| r.id)
    }
}
/// Nonclone; owned before first poll and drained by the root driver.
pub(super) struct Reservation {
    slot: Arc<Slot>,
    banks: Vec<Arc<Mutex<Bank>>>,
    actor: Actor,
    finished: bool,
}
impl Reservation {
    pub(super) fn held(&self) -> bool {
        self.slot.busy.load(Ordering::Acquire)
    }
    pub(super) fn cooldown(&self, seconds: Option<u64>) {
        record_cooldown(&self.banks, &self.actor, seconds);
    }
    pub(super) fn finish(mut self, receipt: Receipt) {
        if let Ok(mut r) = self.slot.receipt.lock() {
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
pub(crate) fn carry(previous: &mut ShellState, loaded: &mut ShellState) {
    let mut banks = std::mem::take(&mut previous.search_reaction_actions.registry);
    banks
        .0
        .extend(std::mem::take(&mut loaded.search_reaction_actions.registry).0);
    loaded.search_reaction_actions.registry = banks;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reaction_actions_retry_after_overflow_holds_actor_across_carry_and_finite_updates() {
        let actor = Actor {
            account: "a".into(),
            team: "T1".into(),
            user: "U1".into(),
        };
        let key = Key {
            actor: actor.clone(),
            channel: "C1".into(),
            ts: "1700000001.000001".into(),
        };
        let mut old = ShellState::fixture(crate::media::MediaRegistry::default());
        let registry = &old.search_reaction_actions.registry;
        let reservation = registry.claim(&key).unwrap();
        reservation.cooldown(Some(u64::MAX));
        reservation.finish(Receipt {
            id: 1,
            name: "wave".into(),
            adding: true,
            outcome: Outcome::Unknown,
        });
        registry.cooldown(&actor, Some(1));
        let mut loaded = ShellState::fixture(crate::media::MediaRegistry::default());
        carry(&mut old, &mut loaded);
        let registry = &loaded.search_reaction_actions.registry;
        let mut peer = key.clone();
        peer.ts = "1700000002.000001".into();
        assert!(registry.cooling(&actor));
        assert!(
            registry.claim(&peer).is_none(),
            "actor cooldown spans messages and replacement Shell"
        );
        peer.actor.user = "U2".into();
        assert!(!registry.cooling(&peer.actor));
        let reservation = registry.claim(&peer).unwrap();
        reservation.finish(Receipt {
            id: 2,
            name: "wave".into(),
            adding: true,
            outcome: Outcome::NotSent,
        });
        registry.cooldown(&peer.actor, Some(1));
        assert!(
            registry.cooling(&peer.actor),
            "ordinary finite cooldown remains enforced"
        );
        registry.cooldown(&peer.actor, Some(u64::MAX));
        registry.cooldown(&peer.actor, Some(1));
        assert!(
            registry
                .0
                .iter()
                .any(|b| b.lock().unwrap().cooldowns.get(&peer.actor) == Some(&None))
        );
        assert!(registry.claim(&peer).is_none());
    }
}
