//! Shared read cooldown data only; no old navigation or invocation is carried.
use super::*;
use std::time::{Duration, Instant};
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Actor(String, String, String);
impl Actor {
    pub(super) fn capture(s: &ShellState) -> Option<Self> {
        let team = s.core.active_team.as_ref()?;
        Some(Self(
            s.core.active_account.clone()?,
            team.clone(),
            s.core.workspaces.get(team)?.self_user_id.clone(),
        ))
    }
}
#[derive(Default)]
struct Cell {
    until: Mutex<BTreeMap<Actor, Option<Instant>>>,
    dirty: AtomicBool,
}
#[derive(Clone)]
pub(super) struct Bank(Vec<Arc<Cell>>);
impl Default for Bank {
    fn default() -> Self {
        Self(vec![Arc::new(Cell::default())])
    }
}
impl Bank {
    pub(super) fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }
    pub(super) fn cooling(&self, actor: &Actor) -> bool {
        self.0.iter().any(|b| {
            b.until.lock().map_or(true, |m| {
                m.get(actor)
                    .is_some_and(|t| t.is_none_or(|t| t > Instant::now()))
            })
        })
    }
    pub(super) fn observe(&self, actor: &Actor, result: &Result<Value, Error>) {
        let seconds = match result {
            Err(Error::RateLimited { retry_after_secs }) => Some(retry_after_secs.unwrap_or(60)),
            Err(Error::HttpStatus {
                retry_after_secs, ..
            }) => *retry_after_secs,
            _ => None,
        };
        let Some(seconds) = seconds else { return };
        let until = Instant::now().checked_add(Duration::from_secs(seconds.max(1)));
        for b in &self.0 {
            if let Ok(mut map) = b.until.lock() {
                map.entry(actor.clone())
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
    pub(super) fn take_dirty(&self) -> bool {
        self.0.iter().fold(false, |dirty, b| {
            b.dirty.swap(false, Ordering::AcqRel) || dirty
        })
    }
}
