//! Bounded process-local notes, separate from durable dispatched attempts.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
pub(super) struct Visit {
    pub key: (admission::Key, String),
    pub source: Value,
    retired: AtomicBool,
    writable: bool,
}
impl Visit {
    pub(super) fn writable(&self) -> bool {
        self.writable
    }
    /// Stored identity only: never constructs a live Source or grants sharing.
    pub(super) fn review(
        key: &admission::Key,
        draft: &super_platinum_core::file_sharing::store::drafts::Draft,
    ) -> Result<Arc<Self>, Refusal> {
        if draft.scope.team != key.team
            || draft.scope.user != key.user
            || draft.content.source.get("id").and_then(Value::as_str)
                != Some(draft.scope.file.as_str())
            || !admission::bounded(&draft.content.source, 256 * 1024, 32)
        {
            return Err(Refusal::Changed);
        }
        Ok(Arc::new(Self {
            key: (key.clone(), draft.scope.file.clone()),
            source: draft.content.source.clone(),
            retired: AtomicBool::new(false),
            writable: false,
        }))
    }
    pub(super) fn active(&self) -> bool {
        !self.retired.load(Ordering::Relaxed)
    }
}
pub(super) struct Draft {
    pub visit: Arc<Visit>,
    pub note: Vec<Value>,
    pub destination: Option<Destination>,
}
impl recovery::Custody {
    pub fn visit(&mut self, source: &admission::Source) -> Result<Arc<Visit>, Refusal> {
        self.draft_visits
            .retain(|_, visit| visit.strong_count() > 0);
        let key = (source.context.key.clone(), source.id.clone());
        if !self.draft_visits.contains_key(&key) && self.draft_visits.len() >= 32 {
            return Err(Refusal::Capacity);
        }
        let visit = Arc::new(Visit {
            key: key.clone(),
            source: source.file.clone(),
            retired: AtomicBool::new(false),
            writable: true,
        });
        if let Some(old) = self
            .draft_visits
            .insert(key.clone(), Arc::downgrade(&visit))
            .and_then(|v| v.upgrade())
        {
            old.retired.store(true, Ordering::Relaxed);
        }
        Ok(visit)
    }
    pub fn capture_note(
        &mut self,
        visit: &Arc<Visit>,
        note: Vec<Value>,
        destination: Option<Destination>,
    ) -> Result<bool, Refusal> {
        if !note.is_empty() {
            api::file_share::FileShareNote::new(note.clone()).map_err(|_| Refusal::Note)?;
        }
        if visit.retired.load(Ordering::Relaxed)
            || !self
                .draft_visits
                .get(&visit.key)
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|v| Arc::ptr_eq(&v, visit))
        {
            return Ok(false);
        }
        self.drafts.insert(
            visit.key.clone(),
            Draft {
                visit: visit.clone(),
                note,
                destination,
            },
        );
        Ok(true)
    }
    pub fn discard_note(&mut self, visit: &Arc<Visit>) -> Result<(), Refusal> {
        if !self
            .drafts
            .get(&visit.key)
            .is_some_and(|d| Arc::ptr_eq(&d.visit, visit))
        {
            return Err(Refusal::Changed);
        }
        self.drafts.remove(&visit.key);
        self.draft_visits.remove(&visit.key);
        visit.retired.store(true, Ordering::Relaxed);
        Ok(())
    }
    pub fn accepted_note(&mut self, visit: &Arc<Visit>, receipt: &Receipt) {
        if !matches!(
            receipt.state(),
            super_platinum_core::file_sharing::State::Server(
                api::file_share::FileShareDisposition::Accepted { .. }
            )
        ) {
            return;
        }
        let exact = self.drafts.get(&visit.key).is_some_and(|d| {
            Arc::ptr_eq(&d.visit, visit)
                && receipt.scope.team == visit.key.0.team
                && receipt.scope.user == visit.key.0.user
                && receipt.scope.file == visit.key.1
                && d.note.as_slice() == receipt.intent.note().unwrap_or(&[])
                && d.destination
                    .as_ref()
                    .is_some_and(|dest| dest.id == receipt.scope.destination)
        });
        if exact {
            let _ = self.discard_note(visit);
        }
    }
}
