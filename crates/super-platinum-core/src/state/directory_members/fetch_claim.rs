//! Ephemeral per-team UnknownFetchManager claims; no IO, timers or persistence.
//! The desktop owner supplies one clock and exact account/transport witnesses.
use super::{Store, unavailable};
use crate::{config::WorkspaceSession, slack::Error};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

#[cfg(test)]
mod tests;

pub const DEBOUNCE_MS: u64 = 200;
pub const ATTEMPT_TIMEOUT_MS: u64 = 65_000;
pub const MAX_ATTEMPTS: u8 = 3;
const MAX_IDS: usize = 100_000;
const MAX_SUBSCRIBERS: usize = 1024;

#[derive(Clone)]
struct Token {
    owner: Arc<()>,
    serial: u64,
}
impl Token {
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.serial == other.serial
    }
}
/// Private identity, not a source/read/renderer authorization.
pub struct Subscription(Token);
pub struct Wake {
    token: Token,
    due: u64,
}
impl Wake {
    pub fn due_ms(&self) -> u64 {
        self.due
    }
}
pub struct Dispatch {
    token: Token,
    ids: Vec<String>,
}
impl Dispatch {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
}
pub struct Adoption {
    token: Token,
    subscriber: u64,
    ids: Vec<String>,
}
impl Adoption {
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Complete,
    Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timeout {
    Retry,
    Exhausted,
}

enum Phase<R> {
    Idle,
    Debouncing(Token),
    Running {
        token: Token,
        ids: Vec<String>,
        attempt: u8,
    },
    Received {
        token: Token,
        ids: Vec<String>,
        receipt: Arc<R>,
        adopter: Option<(u64, Token)>,
    },
}
/// One instance per actual account/team context. Never clone this scheduler
/// with a candidate Store; replacing a context creates a new token owner.
pub struct Manager<R> {
    owner: Arc<()>,
    serial: u64,
    last_start: Option<u64>,
    pending: Vec<String>,
    subscribers: BTreeMap<u64, BTreeSet<String>>,
    phase: Phase<R>,
}
impl<R> Default for Manager<R> {
    fn default() -> Self {
        Self {
            owner: Arc::new(()),
            serial: 0,
            last_start: None,
            pending: vec![],
            subscribers: BTreeMap::new(),
            phase: Phase::Idle,
        }
    }
}
impl<R> Manager<R> {
    fn token(&mut self) -> Result<Token, Error> {
        self.serial = self.serial.checked_add(1).ok_or_else(unavailable)?;
        Ok(Token {
            owner: self.owner.clone(),
            serial: self.serial,
        })
    }
    fn owns(&self, token: &Token) -> bool {
        Arc::ptr_eq(&self.owner, &token.owner)
    }
    fn active_ids(&self) -> &[String] {
        match &self.phase {
            Phase::Running { ids, .. } | Phase::Received { ids, .. } => ids,
            _ => &[],
        }
    }
    pub fn register(&mut self, ids: &[String]) -> Result<Subscription, Error> {
        if ids.is_empty() || ids.len() > MAX_IDS || self.subscribers.len() >= MAX_SUBSCRIBERS {
            return Err(unavailable());
        }
        // Reuse the closed member identity validator. Duplicates collapse in
        // first-encounter order exactly as the pending object map does.
        let mut unique = BTreeSet::new();
        let ordered: Vec<_> = ids
            .iter()
            .filter(|id| unique.insert((*id).clone()))
            .cloned()
            .collect();
        for chunk in ordered.chunks(100) {
            crate::block_actions::directory_endpoint::member_info::Read::capture(chunk.to_vec())?;
        }
        let active: BTreeSet<_> = self.active_ids().iter().map(String::as_str).collect();
        let mut present: BTreeSet<_> = self.pending.iter().map(String::as_str).collect();
        let additions: Vec<_> = ordered
            .iter()
            .filter(|id| !active.contains(id.as_str()) && present.insert(id.as_str()))
            .cloned()
            .collect();
        if self.pending.len() + active.len() + additions.len() > MAX_IDS {
            return Err(unavailable());
        }
        let token = self.token()?;
        self.pending.extend(additions);
        self.subscribers
            .insert(token.serial, ordered.into_iter().collect());
        Ok(Subscription(token))
    }
    pub fn status(&self, sub: &Subscription) -> Status {
        if !self.owns(&sub.0) {
            return Status::Retired;
        }
        match self.subscribers.get(&sub.0.serial) {
            Some(ids) if ids.is_empty() => Status::Complete,
            Some(_) => Status::Pending,
            None => Status::Retired,
        }
    }
    /// There is at most one wake owner. Other consumers do not start timers.
    /// Even a zero-delay first wake dispatches later, allowing same-turn merge.
    pub fn arm(&mut self, now_ms: u64) -> Result<Option<Wake>, Error> {
        if !matches!(self.phase, Phase::Idle) || self.pending.is_empty() {
            return Ok(None);
        }
        let due = match self.last_start {
            Some(last) => last
                .checked_add(DEBOUNCE_MS)
                .ok_or_else(unavailable)?
                .max(now_ms),
            None => now_ms,
        };
        let token = self.token()?;
        self.phase = Phase::Debouncing(token.clone());
        Ok(Some(Wake { token, due }))
    }
    pub fn dispatch(&mut self, wake: &Wake, now_ms: u64) -> Result<Dispatch, Error> {
        if now_ms < wake.due
            || !matches!(&self.phase,Phase::Debouncing(t) if t.same(&wake.token))
            || self.pending.is_empty()
        {
            return Err(unavailable());
        }
        let ids = std::mem::take(&mut self.pending);
        self.last_start = Some(now_ms);
        self.phase = Phase::Running {
            token: wake.token.clone(),
            ids: ids.clone(),
            attempt: 1,
        };
        Ok(Dispatch {
            token: wake.token.clone(),
            ids,
        })
    }
    pub fn cancel_wake(&mut self, wake: &Wake) -> Result<(), Error> {
        if !matches!(&self.phase,Phase::Debouncing(t) if t.same(&wake.token)) {
            return Err(unavailable());
        }
        self.phase = Phase::Idle;
        Ok(())
    }
    pub fn dispatch_current(&self, dispatch: &Dispatch) -> bool {
        matches!(&self.phase,Phase::Running{token,ids,..} if token.same(&dispatch.token) && ids==&dispatch.ids)
    }
    /// Caller invokes this only for the owned65s timeout, not an API/HTTP error.
    pub fn timeout(&mut self, dispatch: &Dispatch) -> Result<Timeout, Error> {
        if !self.dispatch_current(dispatch) {
            return Err(unavailable());
        }
        let Phase::Running { attempt, .. } = &mut self.phase else {
            unreachable!()
        };
        if *attempt == MAX_ATTEMPTS {
            Ok(Timeout::Exhausted)
        } else {
            *attempt += 1;
            Ok(Timeout::Retry)
        }
    }
    pub fn publish(&mut self, dispatch: &Dispatch, receipt: R) -> Result<(), Error> {
        if !self.dispatch_current(dispatch) {
            return Err(unavailable());
        }
        self.phase = Phase::Received {
            token: dispatch.token.clone(),
            ids: dispatch.ids.clone(),
            receipt: Arc::new(receipt),
            adopter: None,
        };
        Ok(())
    }
    /// Native Retry-After continuation: only actually completed IDs may be
    /// projected. Deferred IDs retain original order and subscriber custody.
    /// This does not turn an unsent read into missing-member evidence.
    pub fn publish_completed(
        &mut self,
        dispatch: &Dispatch,
        completed: &[String],
        receipt: impl FnOnce(Dispatch) -> R,
    ) -> Result<(), Error> {
        if !self.dispatch_current(dispatch) || completed.is_empty() {
            return Err(unavailable());
        }
        let complete: BTreeSet<_> = completed.iter().collect();
        let expected: Vec<_> = dispatch
            .ids
            .iter()
            .filter(|id| complete.contains(id))
            .cloned()
            .collect();
        if complete.len() != completed.len() || expected != completed {
            return Err(unavailable());
        }
        let mut pending: Vec<_> = dispatch
            .ids
            .iter()
            .filter(|id| !complete.contains(id))
            .cloned()
            .collect();
        let token = dispatch.token.clone();
        let receipt = Arc::new(receipt(Dispatch {
            token: token.clone(),
            ids: completed.to_vec(),
        }));
        pending.append(&mut self.pending);
        self.pending = pending;
        self.phase = Phase::Received {
            token,
            ids: completed.to_vec(),
            receipt,
            adopter: None,
        };
        Ok(())
    }
    /// No completed IDs and no missing projection. One later wake may retry
    /// the same ordered requests after the independently retained cooldown.
    pub fn defer(&mut self, dispatch: &Dispatch) -> Result<(), Error> {
        if !self.dispatch_current(dispatch) {
            return Err(unavailable());
        }
        let mut pending = dispatch.ids.clone();
        pending.append(&mut self.pending);
        self.pending = pending;
        self.phase = Phase::Idle;
        Ok(())
    }
    /// Local/stale refusal never becomes a successful empty terminal or a
    /// placeholder. Subscribers must recapture source before registering again.
    pub fn refuse(&mut self, dispatch: &Dispatch) -> Result<(), Error> {
        if !self.dispatch_current(dispatch) {
            return Err(unavailable());
        }
        let active: BTreeSet<_> = dispatch.ids.iter().collect();
        self.subscribers
            .retain(|_, ids| !ids.iter().any(|id| active.contains(id)));
        self.phase = Phase::Idle;
        Ok(())
    }
    pub fn receipt_current(&self, dispatch: &Dispatch) -> bool {
        matches!(&self.phase,Phase::Received{token,ids,..} if token.same(&dispatch.token) && ids==&dispatch.ids)
    }
    /// Inspect retained evidence before registering a fresh source capture.
    /// This grants no adoption or settlement authority; stale evidence still
    /// requires its exact dispatch token through `refuse_receipt`.
    pub fn retained_receipt(&self) -> Option<Arc<R>> {
        match &self.phase {
            Phase::Received { receipt, .. } => Some(receipt.clone()),
            _ => None,
        }
    }
    /// A source/native/context drift is a local refusal, never missing-user
    /// evidence. Queued IDs from newer registrations remain separate.
    pub fn refuse_receipt(&mut self, dispatch: &Dispatch) -> Result<(), Error> {
        if !self.receipt_current(dispatch) {
            return Err(unavailable());
        }
        let active: BTreeSet<_> = dispatch.ids.iter().collect();
        self.subscribers
            .retain(|_, ids| !ids.iter().any(|id| active.contains(id)));
        self.phase = Phase::Idle;
        Ok(())
    }
    pub fn claim_adoption(
        &mut self,
        sub: &Subscription,
    ) -> Result<Option<(Adoption, Arc<R>)>, Error> {
        if self.status(sub) == Status::Retired {
            return Err(unavailable());
        }
        if !matches!(&self.phase, Phase::Received { adopter: None, .. }) {
            return Ok(None);
        }
        let token = self.token()?;
        let Phase::Received {
            ids,
            receipt,
            adopter,
            ..
        } = &mut self.phase
        else {
            unreachable!()
        };
        *adopter = Some((sub.0.serial, token.clone()));
        Ok(Some((
            Adoption {
                token,
                subscriber: sub.0.serial,
                ids: ids.clone(),
            },
            receipt.clone(),
        )))
    }
    pub fn adoption_current(&self, adoption: &Adoption) -> bool {
        self.subscribers.contains_key(&adoption.subscriber)
            && matches!(&self.phase,Phase::Received{ids,adopter:Some((s,t)),..} if *s==adoption.subscriber && t.same(&adoption.token) && ids==&adoption.ids)
    }
    pub fn release_adoption(&mut self, adoption: &Adoption) -> Result<(), Error> {
        if !self.adoption_current(adoption) {
            return Err(unavailable());
        }
        let Phase::Received { adopter, .. } = &mut self.phase else {
            unreachable!()
        };
        *adopter = None;
        Ok(())
    }
    /// Public settlement checks the actual installed canonical source/native
    /// pair. A caller-supplied bool or raw response cannot release the claim.
    pub fn settle(
        &mut self,
        adoption: &Adoption,
        store: &Store,
        account: &str,
        owner: &WorkspaceSession,
        native: &HashMap<String, crate::slack::models::User>,
    ) -> Result<(), Error> {
        if !self.adoption_current(adoption)
            || !installed(store, account, owner, native, &adoption.ids)?
        {
            return Err(unavailable());
        }
        self.finish(adoption);
        Ok(())
    }
    fn finish(&mut self, adoption: &Adoption) {
        let ids: BTreeSet<_> = adoption.ids.iter().collect();
        for requested in self.subscribers.values_mut() {
            requested.retain(|id| !ids.contains(id));
        }
        self.phase = Phase::Idle;
    }
    pub fn retire(&mut self, sub: &Subscription) {
        if !self.owns(&sub.0) {
            return;
        }
        self.subscribers.remove(&sub.0.serial);
        if let Phase::Received { adopter, .. } = &mut self.phase {
            if adopter.as_ref().is_some_and(|(s, _)| *s == sub.0.serial) {
                *adopter = None;
            }
        }
        // In-flight operation and receipt survive an individual query. With
        // no subscribers a later valid consumer can adopt this bounded batch.
    }
}
fn installed(
    store: &Store,
    account: &str,
    owner: &WorkspaceSession,
    native: &HashMap<String, crate::slack::models::User>,
    ids: &[String],
) -> Result<bool, Error> {
    let saved = store.identity(account, owner)?;
    if saved.overflowed {
        return Ok(false);
    }
    let ids: BTreeSet<_> = ids.iter().map(String::as_str).collect();
    if saved.invalidated.iter().any(|id| ids.contains(id.as_str()))
        || saved.pending.iter().any(|batch| {
            batch
                .rows
                .iter()
                .any(|row| row["id"].as_str().is_some_and(|id| ids.contains(id)))
                || batch
                    .interactions
                    .as_object()
                    .is_some_and(|values| values.keys().any(|id| ids.contains(id.as_str())))
                || batch.disconnection.as_ref().is_some_and(|effect| {
                    effect
                        .requested
                        .iter()
                        .chain(&effect.marked)
                        .any(|id| ids.contains(id.as_str()))
                })
        })
    {
        return Ok(false);
    }
    let records: BTreeMap<_, _> = saved
        .records
        .iter()
        .filter_map(|row| row.source["id"].as_str().map(|id| (id, row)))
        .collect();
    Ok(ids.iter().all(|id| {
        records.get(id).is_some_and(|row| {
            row.source.get("isUnknown") != Some(&serde_json::Value::Bool(true))
                && native
                    .get(*id)
                    .and_then(|u| serde_json::to_value(u).ok())
                    .as_ref()
                    == Some(&row.native_witness)
        })
    }))
}
