//! Minimal process-lifetime exclusion for every pre-existing creation entry point.
//! No credentials, request or replay authority survives here. The existing full
//! receipt remains the richer view while its ShellState exists.
use super::*;
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};
#[derive(Clone, Copy)]
struct Barrier {
    serial: u64,
    stage: Stage,
    completed: bool,
}
#[cfg(not(test))]
fn with<R>(f: impl FnOnce(&mut BTreeMap<Key, Barrier>) -> R) -> R {
    static REGISTRY: OnceLock<Mutex<BTreeMap<Key, Barrier>>> = OnceLock::new();
    f(&mut REGISTRY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner()))
}
// Each unit-test thread is an isolated serial UI instance. Whole-shell
// replacement in that same instance deliberately retains the map.
#[cfg(test)]
thread_local! {static TEST_REGISTRY:std::cell::RefCell<BTreeMap<Key,Barrier>>=Default::default();}
#[cfg(test)]
fn with<R>(f: impl FnOnce(&mut BTreeMap<Key, Barrier>) -> R) -> R {
    TEST_REGISTRY.with(|r| f(&mut r.borrow_mut()))
}
pub(super) fn contains(key: &Key) -> bool {
    with(|r| r.contains_key(key))
}
pub(super) fn owns(key: &Key, serial: u64) -> bool {
    with(|r| r.get(key).is_some_and(|e| e.serial == serial))
}
pub(super) fn reserve(key: &Key, serial: u64) -> Result<(), &'static str> {
    with(|r| {
        if r.contains_key(key) || r.len() >= 64 {
            return Err("Review retained item requests before adding another.");
        }
        r.insert(
            key.clone(),
            Barrier {
                serial,
                stage: Stage::Preparing,
                completed: false,
            },
        );
        Ok(())
    })
}
pub(super) fn sending(key: &Key, serial: u64) {
    with(|r| {
        if let Some(e) = r
            .get_mut(key)
            .filter(|e| e.serial == serial && e.stage == Stage::Preparing)
        {
            e.stage = Stage::Sending;
        }
    });
}
pub(super) fn reject(key: &Key, serial: u64) {
    with(|r| {
        if r.get(key).is_some_and(|e| e.serial == serial) {
            r.remove(key);
        }
    });
}
pub(super) fn complete(key: &Key, serial: u64, result: &Result<Value, Error>) {
    with(|r| {
        let Some(e) = r
            .get_mut(key)
            .filter(|e| e.serial == serial && e.stage == Stage::Sending && !e.completed)
        else {
            return;
        };
        // This fallback has no frozen-intent proof, so even an ok body is uncertain.
        // A richer current-shell receipt may separately establish an exact identity.
        let _ = result;
        e.completed = true;
        e.stage = Stage::Uncertain;
    });
}
pub(super) fn remove(key: &Key, serial: u64) {
    with(|r| {
        if r.get(key).is_some_and(|e| e.serial == serial) {
            r.remove(key);
        }
    });
}
pub(super) fn retire(key: &Key, serial: u64, stage: Stage) -> bool {
    with(|r| {
        if r.get(key)
            .is_some_and(|e| e.serial == serial && e.stage == stage)
        {
            r.remove(key);
            true
        } else {
            false
        }
    })
}
pub(super) fn presentation(key: &Key) -> Option<CreateOperationView> {
    with(|r| {
        r.get(key).map(|e|CreateOperationView{
    serial:e.serial,pending:matches!(e.stage,Stage::Preparing|Stage::Sending),
    can_cancel:e.stage==Stage::Preparing,can_retire:e.stage==Stage::Uncertain,can_dismiss:false,
    message:match e.stage {Stage::Preparing=>"An earlier item request is being checked. Cancel it before continuing.",Stage::Sending=>"An earlier item request is still finishing. It will not be sent again.",_=>"An earlier item request may have reached Slack. Review the List before explicitly dismissing this receipt; dismissing does not undo it."}
})
    })
}

pub(super) fn pending(key: &Key, serial: u64) -> bool {
    with(|r| {
        r.get(key)
            .is_some_and(|e| e.serial == serial && e.stage == Stage::Sending && !e.completed)
    })
}
