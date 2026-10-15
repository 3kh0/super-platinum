//! Actual renderer future custody. A root owns requests; executors only poll them.
use crate::drafts::protected::operation::{MAX_OPERATIONS, Operation, Status};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll},
};
use super_platinum_core::drafts::store::protected as durable;

#[derive(Default)]
pub(crate) struct Root {
    entries: RefCell<Vec<Rc<Entry>>>,
}
struct Entry {
    operation: Operation,
    state: RefCell<EntryState>,
}
struct EntryState {
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    polling: bool,
}
// A synchronous poll transfers custody to this armed stack owner. The retained
// Entry remains explicitly Polling; no RefCell borrow survives user future code.
struct PollOwner {
    entry: Rc<Entry>,
    future: Option<Pin<Box<dyn Future<Output = ()>>>>,
    returned: bool,
}
impl Drop for PollOwner {
    fn drop(&mut self) {
        if !self.returned {
            self.entry.operation.unknown();
        }
        if let Ok(mut state) = self.entry.state.try_borrow_mut() {
            state.polling = false;
            if self.returned && self.entry.operation.current() {
                state.future = self.future.take();
            }
        } else {
            self.entry.operation.unknown();
        }
    }
}
struct Completion {
    operation: Operation,
    completed: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.completed {
            self.operation.unknown();
        }
    }
}
impl Root {
    pub(super) fn admit_request<T>(&self, request: T) -> Result<T, (durable::Error, T)> {
        match self.has_capacity() {
            Ok(()) => Ok(request),
            Err(error) => Err((error, request)),
        }
    }
    pub(crate) fn has_capacity(&self) -> Result<(), durable::Error> {
        let entries = self
            .entries
            .try_borrow()
            .map_err(|_| durable::Error::Storage)?;
        if entries.len() >= MAX_OPERATIONS {
            return Err(durable::Error::Pending);
        }
        if dioxus::core::Runtime::try_current().is_none() {
            return Err(durable::Error::Stale);
        }
        Ok(())
    }
    /// Explicit handoff acknowledgement; unresolved outcomes cannot free capacity.
    pub(crate) fn acknowledge(&self, operation: &Operation) -> Result<(), durable::Error> {
        let mut entries = self
            .entries
            .try_borrow_mut()
            .map_err(|_| durable::Error::Storage)?;
        let index = entries
            .iter()
            .position(|entry| entry.operation.same(operation))
            .ok_or(durable::Error::Stale)?;
        {
            let state = entries[index]
                .state
                .try_borrow()
                .map_err(|_| durable::Error::Storage)?;
            if state.polling || state.future.is_some() {
                return Err(durable::Error::Pending);
            }
        }
        operation.acknowledge()?;
        entries.remove(index);
        Ok(())
    }
    /// Private composition point; only typed capture/delivery constructors call it.
    /// The actual non-Send future remains in this root, never only in the Task.
    pub(super) fn install(
        &self,
        operation: Operation,
        future: impl Future<Output = Result<Status, durable::Error>> + 'static,
    ) -> Result<(), durable::Error> {
        let entry = Entry::new(operation.clone(), future);
        let mut entries = self
            .entries
            .try_borrow_mut()
            .map_err(|_| durable::Error::Storage)?;
        if entries.len() >= MAX_OPERATIONS
            || entries.iter().any(|entry| entry.operation.same(&operation))
        {
            return Err(durable::Error::Pending);
        }
        entries.push(entry.clone());
        drop(entries);
        let weak = Rc::downgrade(&entry);
        dioxus::core::spawn_forever(std::future::poll_fn(move |cx| poll(&weak, cx)));
        Ok(())
    }
}
impl Entry {
    fn new(
        operation: Operation,
        future: impl Future<Output = Result<Status, durable::Error>> + 'static,
    ) -> Rc<Self> {
        // Construct this guard synchronously, before the future's first poll or
        // any executor spawn; a refused install also drops this exact guard.
        let guard = Completion {
            operation: operation.clone(),
            completed: false,
        };
        Rc::new(Self {
            operation,
            state: RefCell::new(EntryState {
                polling: false,
                future: Some(Box::pin(async move {
                    let mut guard = guard;
                    let result = future.await;
                    guard.operation.finish(result);
                    guard.completed = true;
                })),
            }),
        })
    }
}

fn poll(entry: &Weak<Entry>, cx: &mut Context<'_>) -> Poll<()> {
    let Some(entry) = entry.upgrade() else {
        return Poll::Ready(());
    };
    let Ok(mut held) = entry.state.try_borrow_mut() else {
        entry.operation.unknown();
        return Poll::Ready(());
    };
    if held.polling {
        cx.waker().wake_by_ref();
        return Poll::Pending;
    }
    let Some(future) = held.future.take() else {
        return Poll::Ready(());
    };
    held.polling = true;
    let mut owner = PollOwner {
        entry: entry.clone(),
        future: Some(future),
        returned: false,
    };
    drop(held);
    let result = owner
        .future
        .as_mut()
        .expect("exact poll owner")
        .as_mut()
        .poll(cx);
    if result.is_ready() {
        owner.future.take();
    }
    owner.returned = true;
    result
}
impl Drop for Root {
    fn drop(&mut self) {
        for entry in self.entries.get_mut().iter() {
            // Revoke first, even if an executor is currently inside a poll and
            // temporarily owns the Entry. Late evidence cannot revive authority.
            entry.operation.unknown();
            if let Ok(mut state) = entry.state.try_borrow_mut() {
                state.future.take();
            }
        }
    }
}

#[cfg(test)]
#[path = "protected_operations/tests.rs"]
mod tests;
