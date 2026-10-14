//! Cleanup must survive a temporary signal borrow during component/future drop.
//! Every action retains its exact lease, so a later callback cannot alter a new visit.
use super::{Lease, Operations, other::Outcome};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Cancel,
    Detach,
    Settle(Outcome),
}

fn apply(operations: &mut Operations, lease: &Lease, action: Action) {
    let _ = match action {
        Action::Cancel => operations.cancel(lease),
        Action::Detach => operations.detach_other(lease),
        Action::Settle(outcome) => operations.settle_other(lease, outcome),
    };
}

pub(crate) fn run(mut operations: Signal<Operations>, lease: Lease, action: Action) {
    match operations.try_write() {
        Ok(mut registry) => {
            apply(&mut registry, &lease, action);
            return;
        }
        Err(BorrowMutError::Dropped(_)) => return, // The app-root registry itself is gone.
        Err(_) => {}
    }
    dioxus::core::spawn_forever(async move {
        loop {
            tokio::task::yield_now().await;
            match operations.try_write() {
                Ok(mut registry) => {
                    apply(&mut registry, &lease, action);
                    return;
                }
                Err(BorrowMutError::Dropped(_)) => return,
                Err(_) => {}
            }
            // A caller must not hold signal borrows over await; still retain this
            // cleanup rather than silently abandoning it if another caller does.
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    });
}

#[cfg(test)]
mod tests;
