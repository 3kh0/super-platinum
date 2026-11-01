//! Repaint detection for an actually rendered expiring status, never admission.
//! The existing runtime tick owns the deadline; only the renderer records faces.
use super::{Appearance, Owner, Status, project};
use crate::state::ShellState;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PERIOD: Duration = Duration::from_secs(30);

// Owner contains original opaque account/session guards. Deliberately no
// Debug/Serialize/Eq on this record, and no member rows or ready receipts.
#[derive(Clone)]
struct Face {
    occurrence: Rc<()>,
    owner: Arc<Owner>,
    appearance: Appearance,
    ack: Rc<()>,
}

thread_local! {
    static DISPLAYED: RefCell<Option<Face>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static TEST_NOW: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Nonnegative epoch milliseconds, rounded as Math.round(Date.now()/1000).
pub(super) fn rounded_millis(millis: u128) -> Option<u64> {
    u64::try_from(millis.checked_add(500)? / 1000).ok()
}

pub(super) fn now() -> Option<u64> {
    #[cfg(test)]
    if let Some(value) = TEST_NOW.with(std::cell::Cell::get) {
        return Some(value);
    }
    rounded_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis(),
    )
}

/// None removes the override. Simulated unavailable time uses poll_at(None).
#[cfg(test)]
pub(super) fn test_now(value: Option<u64>) -> Option<u64> {
    TEST_NOW.with(|clock| clock.replace(value))
}

/// Only a current renderer-projected positive-expiry Owned status is recorded.
/// The parent guarantees Layout has no owner and expiry zero. This function
/// does not capture or validate a new owner, and does not render any data.
pub(super) fn display(occurrence: Rc<()>, status: Option<&Status>) {
    let Some(status) = status.filter(|status| status.expires > 0) else {
        retire(&occurrence);
        return;
    };
    let Some(owner) = status.owner.as_ref() else {
        retire(&occurrence);
        return;
    };
    DISPLAYED.with(|displayed| {
        *displayed.borrow_mut() = Some(Face {
            occurrence,
            owner: owner.clone(),
            appearance: status.appearance(),
            ack: Rc::new(()),
        });
    });
}

/// Component cleanup/clear cannot erase a different mounted occurrence.
pub(super) fn retire(occurrence: &Rc<()>) {
    DISPLAYED.with(|displayed| {
        let mut displayed = displayed.borrow_mut();
        if displayed
            .as_ref()
            .is_some_and(|face| Rc::ptr_eq(&face.occurrence, occurrence))
        {
            *displayed = None;
        }
    });
}

fn same_render(a: &Face, b: &Face) -> bool {
    Rc::ptr_eq(&a.occurrence, &b.occurrence)
        && Arc::ptr_eq(&a.owner, &b.owner)
        && Rc::ptr_eq(&a.ack, &b.ack)
}

/// Polls retire only the original receipt they actually observed, including
/// its owner Arc and render acknowledgement. A successor is never cleared.
fn retire_exact(original: &Face) -> bool {
    DISPLAYED.with(|displayed| {
        let mut displayed = displayed.borrow_mut();
        if displayed
            .as_ref()
            .is_some_and(|current| same_render(current, original))
        {
            *displayed = None;
            true
        } else {
            false
        }
    })
}

struct Pending {
    ack: Rc<()>,
    target: Option<Appearance>,
}

#[derive(Default)]
pub(crate) struct Clock {
    observed: Option<Face>,
    deadline: Option<Instant>,
    pending: Option<Pending>,
}

impl Clock {
    fn reset(&mut self) {
        self.observed = None;
        self.deadline = None;
        self.pending = None;
    }

    fn changed(&mut self, face: &Face, target: Option<Appearance>) -> bool {
        if target.as_ref() == Some(&face.appearance) {
            self.pending = None;
            return false;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| Rc::ptr_eq(&pending.ack, &face.ack) && pending.target == target)
        {
            return false;
        }
        self.pending = Some(Pending {
            ack: face.ack.clone(),
            target,
        });
        true
    }

    pub(crate) fn poll(&mut self, state: &ShellState) -> bool {
        self.poll_with(state, Instant::now(), now)
    }

    /// Time only is injectable. Full projection and original owner guards stay
    /// active, and this method is called under the runtime's silent Shell borrow.
    pub(super) fn poll_at(
        &mut self,
        state: &ShellState,
        monotonic: Instant,
        rounded_now: Option<u64>,
    ) -> bool {
        self.poll_with(state, monotonic, || rounded_now)
    }

    fn poll_with(
        &mut self,
        state: &ShellState,
        monotonic: Instant,
        rounded_now: impl FnOnce() -> Option<u64>,
    ) -> bool {
        let Some(face) = DISPLAYED.with(|displayed| displayed.borrow().clone()) else {
            self.reset();
            return false;
        };

        if !face.owner.current(state) {
            // A stale runtime can see another root/owner's thread-local face.
            // It may retire only its own exact previously observed receipt.
            let ours = self
                .observed
                .as_ref()
                .is_some_and(|old| same_render(old, &face));
            let notify = ours && retire_exact(&face);
            if ours
                || self
                    .observed
                    .as_ref()
                    .is_some_and(|old| !old.owner.current(state))
            {
                self.reset();
            }
            return notify;
        }

        let same_face = self.observed.as_ref().is_some_and(|old| {
            Rc::ptr_eq(&old.occurrence, &face.occurrence)
                && old.appearance == face.appearance
                && old.owner.current(state)
        });
        let acknowledged = self
            .observed
            .as_ref()
            .is_none_or(|old| !same_render(old, &face));
        if !same_face {
            self.deadline = monotonic.checked_add(PERIOD);
            self.pending = None;
        } else if acknowledged {
            // Fresh Owner Arcs are permitted on actual renders. Both original
            // and new guards passed, so keep the unchanged face's deadline.
            self.pending = None;
        }
        self.observed = Some(face.clone());

        let Some(deadline) = self.deadline else {
            // An unrepresentable monotonic deadline cannot strand a display.
            let notify = retire_exact(&face);
            self.reset();
            return notify;
        };
        if monotonic < deadline {
            return false;
        }
        // One late/resumed check, never one replay per missed interval.
        self.deadline = monotonic.checked_add(PERIOD);

        let target = rounded_now().and_then(|now| {
            project(state, now).and_then(|status| {
                status
                    .owner
                    .as_ref()
                    .filter(|owner| owner.current(state))
                    .map(|_| status.appearance())
            })
        });
        if !face.owner.current(state) {
            let notify = retire_exact(&face);
            self.reset();
            return notify;
        }
        self.changed(&face, target)
    }
}
