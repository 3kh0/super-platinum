//! One window's pending clipboard promise; custody alone is never permission.
use super::*;
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Actor(pub String, pub String, pub String);
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Fulfilled,
    Rejected,
    NotRequested,
    Unknown,
}
#[derive(Clone)]
pub(super) struct Receipt {
    pub id: u64,
    pub mount: u64,
    pub seq: u64,
    pub outcome: Outcome,
    // A test transport cannot manufacture an actual native promise receipt.
    pub native: bool,
}
#[derive(Default)]
struct Bank {
    busy: AtomicBool,
    receipt: Mutex<Option<Receipt>>,
    cooldowns: Mutex<BTreeMap<Actor, Option<Instant>>>,
    dirty: AtomicBool,
}
#[derive(Clone)]
pub(super) struct Registry(Vec<Arc<Bank>>);
impl Default for Registry {
    fn default() -> Self {
        Self(vec![Arc::new(Bank::default())])
    }
}
impl Registry {
    pub(super) fn blocked(&self) -> bool {
        self.0.iter().any(|b| {
            b.busy.load(Ordering::Acquire)
                || b.receipt.lock().map_or(true, |r| {
                    r.as_ref().is_some_and(|r| r.outcome == Outcome::Unknown)
                })
        })
    }
    pub(super) fn unknown(&self) -> bool {
        self.0.iter().any(|b| {
            b.receipt.lock().map_or(true, |r| {
                r.as_ref().is_some_and(|r| r.outcome == Outcome::Unknown)
            })
        })
    }
    pub(super) fn cooling(&self, actor: &Actor) -> bool {
        self.0.iter().any(|b| {
            b.cooldowns.lock().map_or(true, |c| {
                c.get(actor)
                    .is_some_and(|u| u.is_none_or(|u| u > Instant::now()))
            })
        })
    }
    pub(super) fn cooldown(&self, actor: &Actor, seconds: Option<u64>) {
        let Some(seconds) = seconds else { return };
        let until = Instant::now().checked_add(Duration::from_secs(seconds.max(1)));
        for b in &self.0 {
            if let Ok(mut c) = b.cooldowns.lock() {
                c.entry(actor.clone())
                    .and_modify(|old| {
                        *old = match (*old, until) {
                            (Some(a), Some(b)) => Some(a.max(b)),
                            _ => None,
                        }
                    })
                    .or_insert(until);
                b.dirty.store(true, Ordering::Release);
            }
        }
    }
    // Called synchronously under the serial shell borrow before first poll.
    pub(super) fn claim(&self) -> Option<Reservation> {
        if self.blocked() {
            return None;
        }
        let bank = self.0.first()?.clone();
        bank.busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(Reservation {
            bank,
            banks: self.0.clone(),
            finished: false,
        })
    }
    pub(super) fn take_dirty(&self) -> bool {
        self.0
            .iter()
            .fold(false, |v, b| b.dirty.swap(false, Ordering::AcqRel) || v)
    }
    #[cfg(test)]
    pub(super) fn receipt(&self) -> Option<Receipt> {
        self.0
            .iter()
            .filter_map(|b| b.receipt.lock().ok()?.clone())
            .max_by_key(|r| (r.id, r.mount, r.seq))
    }
}
pub(super) struct Reservation {
    bank: Arc<Bank>,
    banks: Vec<Arc<Bank>>,
    finished: bool,
}
impl Reservation {
    pub(super) fn pending_unknown(&self, receipt: Receipt) {
        if let Ok(mut r) = self.bank.receipt.lock() {
            *r = Some(receipt)
        }
        for b in &self.banks {
            b.dirty.store(true, Ordering::Release);
        }
    }
    pub(super) fn finish(mut self, receipt: Receipt) {
        let known = receipt.outcome != Outcome::Unknown;
        if let Ok(mut r) = self.bank.receipt.lock() {
            *r = Some(receipt);
            self.finished = known;
        }
        if self.finished {
            self.bank.busy.store(false, Ordering::Release);
        }
        for b in &self.banks {
            b.dirty.store(true, Ordering::Release);
        }
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        // Lost/unknown/cancelled physical work remains occupied. No local UI
        // borrow or a fresh Shell can claim that the original promise settled.
        if self.finished {
            self.bank.busy.store(false, Ordering::Release);
        }
    }
}
pub(crate) fn carry(previous: &mut ShellState, loaded: &mut ShellState) {
    let mut banks = std::mem::take(&mut previous.thread_feed_reads.copy_link.registry);
    banks
        .0
        .extend(std::mem::take(&mut loaded.thread_feed_reads.copy_link.registry).0);
    loaded.thread_feed_reads.copy_link.registry = banks;
}
