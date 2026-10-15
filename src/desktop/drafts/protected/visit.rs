//! A dialog/native visit is independent of a durable inventory capability.
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[derive(Clone)]
pub(crate) struct Visit(Arc<AtomicU8>);
impl Visit {
    pub fn new() -> Self {
        Self(Arc::new(AtomicU8::new(0)))
    }
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn current(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }
    pub fn retire(&self) {
        self.0.store(2, Ordering::Release);
    }
    pub(super) fn claim(&self) -> bool {
        self.0
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloned_visit_is_spent_once_and_cannot_be_rearmed() {
        let first = Visit::new();
        let second = first.clone();
        assert!(first.same(&second));
        assert!(first.claim());
        assert!(!second.claim());
        first.retire();
        assert!(!second.current());
        assert!(!second.claim());
    }
    #[test]
    fn retired_visit_cannot_claim_and_replacement_has_distinct_identity() {
        let previous = Visit::new();
        previous.retire();
        let replacement = Visit::new();
        assert!(!previous.claim());
        assert!(!previous.same(&replacement));
        assert!(replacement.claim());
    }
}
