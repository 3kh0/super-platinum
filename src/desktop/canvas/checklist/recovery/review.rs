//! Deliberate local unlock of a restored unknown receipt. No in-memory request
//! may exist; this path cannot settle a send or produce a new Slack request.
use super::*;
use super_platinum_core::canvas::edit::structure::recovery::journal::{LocalDecision, LocalReview};

const CHANGED: &str = "This canvas changed. Cancel and refresh before reviewing again.";
const STORAGE: &str =
    "The local record could not be updated. It remains locked. Refresh to check its state.";

#[derive(Clone)]
struct Request {
    owner: Owner,
    receipt: Receipt,
    generation: u64,
    read: Read,
    snapshot: Arc<CanvasCollabSnapshot>,
}
impl Request {
    fn capture(shell: &ShellState) -> Option<Self> {
        let view = shell.canvas_checklist.view.as_ref()?;
        if !view_current(shell, view)
            || view.loading
            || view.stale
            || view.error.is_some()
            || operation_index(shell, &view.file).is_some()
        {
            return None;
        }
        let owner = Owner::capture(shell, &view.file)?;
        let disk = &shell.canvas_checklist.recovery;
        if disk.owner.as_ref() != Some(&owner) || !disk.loaded || disk.error {
            return None;
        }
        Some(Self {
            owner,
            receipt: disk.receipt.clone()?,
            generation: disk.generation,
            read: Read {
                scope: view.scope.clone(),
                file: view.file.clone(),
                visit: view.generation,
                serial: view.read,
                revision: view.revision,
            },
            snapshot: view.snapshot.clone()?,
        })
    }
    fn after_read(&self, shell: &ShellState, read: &Read) -> Option<Self> {
        // The opening action owns one original scope/visit/receipt and exactly
        // the read it started. A different completed load cannot supply authority.
        if !self.read.scope.current(shell)
            || !read.current(shell)
            || read.file != self.read.file
            || read.visit != self.read.visit
            || read.serial == self.read.serial
        {
            return None;
        }
        Self::capture(shell).filter(|next| {
            next.owner == self.owner
                && next.receipt == self.receipt
                && next.read.serial == read.serial
                && next.read.visit == self.read.visit
                && next.read.file == self.read.file
        })
    }
    fn current(&self, shell: &ShellState, loaded: bool) -> bool {
        let disk = &shell.canvas_checklist.recovery;
        self.read.current(shell)
            && Owner::capture(shell, &self.read.file).as_ref() == Some(&self.owner)
            && disk.generation == self.generation
            && disk.owner.as_ref() == Some(&self.owner)
            && disk.loaded == loaded
            && !disk.error
            && disk.receipt.as_ref() == Some(&self.receipt)
            && operation_index(shell, &self.read.file).is_none()
            && shell.canvas_checklist.view.as_ref().is_some_and(|view| {
                !view.loading
                    && !view.stale
                    && view.error.is_none()
                    && view
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| Arc::ptr_eq(snapshot, &self.snapshot))
            })
    }
}
struct Ready {
    request: Request,
    review: Arc<LocalReview>,
}
pub(in crate::canvas) struct Draft {
    token: Arc<()>,
    ready: Option<Ready>,
    acknowledged: bool,
    busy: bool,
    error: Option<&'static str>,
}
pub(in crate::canvas) fn available(shell: &ShellState) -> bool {
    Request::capture(shell).is_some()
}

pub(in crate::canvas) fn open(
    mut state: Signal<ShellState>,
    mut reader: Signal<crate::canvas::Reader>,
    target: crate::state::CanvasTarget,
    mut draft: Signal<Option<Draft>>,
) {
    if draft.peek().is_some() {
        return;
    }
    let Some(original) = Request::capture(&state.peek()).filter(|r| r.read.file == target.id)
    else {
        return;
    };
    let token = Arc::new(());
    draft.set(Some(Draft {
        token: token.clone(),
        ready: None,
        acknowledged: false,
        busy: true,
        error: None,
    }));
    spawn(async move {
        // Reserve our exact read before the paired loader's first await. The
        // disconnected fixture takes the same synchronous refresh branch as load.
        let started = {
            let mut shell = state.write();
            if !original.current(&shell, true) {
                None
            } else if crate::canvas::text::structure_fixture::enabled(&shell) {
                crate::canvas::text::structure_fixture::refresh(&mut shell);
                crate::canvas::checklist::reader::update_mapping(&shell, &mut reader.write());
                Request::capture(&shell).map(|r| (r.read, true))
            } else {
                begin_read(&mut shell).map(|read| (read, false))
            }
        };
        let request = if let Some((read, fixture)) = started {
            if !fixture {
                crate::canvas::loader::read_pair(state, reader, read.clone()).await;
            }
            original.after_read(&state.peek(), &read)
        } else {
            None
        };
        let result = if let Some(request) = request {
            let receipt = request.receipt.clone();
            let snapshot = request.snapshot.clone();
            let review = request
                .owner
                .run(move |j| j.local_review(&receipt, &snapshot))
                .await;
            match review {
                Ok(review) if request.current(&state.peek(), true) => Ok(Ready {
                    request,
                    review: Arc::new(review),
                }),
                Ok(_) => Err(CHANGED),
                Err(()) => Err(STORAGE),
            }
        } else {
            Err(CHANGED)
        };
        if let Some(current) = draft
            .write()
            .as_mut()
            .filter(|d| Arc::ptr_eq(&d.token, &token))
        {
            current.busy = false;
            match result {
                Ok(ready) => current.ready = Some(ready),
                Err(error) => current.error = Some(error),
            }
        }
    });
}

fn begin_local_removal(shell: &mut ShellState, request: &Request) -> Option<Request> {
    if !request.current(shell, true) {
        return None;
    }
    let disk = &mut shell.canvas_checklist.recovery;
    disk.generation = disk.generation.wrapping_add(1);
    disk.loaded = false; // Shared edit gate remains locked throughout disk work.
    let mut pending = request.clone();
    pending.generation = disk.generation;
    Some(pending)
}
fn finish_local_removal(shell: &mut ShellState, pending: &Request, result: bool) -> bool {
    if !pending.current(shell, false) {
        return false;
    }
    let disk = &mut shell.canvas_checklist.recovery;
    disk.generation = disk.generation.wrapping_add(1);
    disk.loaded = true;
    if result {
        disk.receipt = None;
        // Explicitly require another canonical read before any new edit.
        if let Some(view) = shell.canvas_checklist.view.as_mut() {
            view.stale = true;
        }
    } else {
        disk.error = true;
    }
    true
}
fn remove(mut state: Signal<ShellState>, mut draft: Signal<Option<Draft>>) {
    let ready = {
        let mut value = draft.write();
        let Some(current) = value.as_mut().filter(|d| !d.busy && d.acknowledged) else {
            return;
        };
        let Some(ready) = current.ready.as_ref() else {
            return;
        };
        let Some(pending) = begin_local_removal(&mut state.write(), &ready.request) else {
            current.error = Some(CHANGED);
            return;
        };
        current.busy = true;
        (pending, ready.review.clone(), current.token.clone())
    };
    spawn(async move {
        let (pending, review, token) = ready;
        let snapshot = pending.snapshot.clone();
        let now = super::super::save::now_usec().unwrap_or(0);
        let done = pending
            .owner
            .run(move |j| {
                j.remove_local_lock(
                    &review,
                    &snapshot,
                    LocalDecision::RemoveEditLockOutcomeUnknown,
                    now,
                )
            })
            .await
            == Ok(true);
        let published = finish_local_removal(&mut state.write(), &pending, done);
        if draft
            .peek()
            .as_ref()
            .is_some_and(|d| Arc::ptr_eq(&d.token, &token))
        {
            if done && published {
                draft.set(None);
                state.write().show_toast(
                    "Local edit lock removed. The outcome remains unknown. Refresh before editing.",
                );
            } else if let Some(current) = draft.write().as_mut() {
                current.busy = false;
                current.error = Some(if published { STORAGE } else { CHANGED });
            }
        }
    });
}

#[component]
pub(in crate::canvas) fn Panel(
    state: Signal<ShellState>,
    mut draft: Signal<Option<Draft>>,
) -> Element {
    let value = draft.read();
    let Some(value) = value.as_ref() else {
        return rsx! {};
    };
    let busy = value.busy;
    let acknowledged = value.acknowledged;
    let enabled = value
        .ready
        .as_ref()
        .is_some_and(|ready| ready.request.current(&state.read(), true));
    let text = value
        .ready
        .as_ref()
        .map(|ready| ready.review.draft_text())
        .unwrap_or_default();
    let action = value.ready.as_ref().map(|ready| {
        if ready.review.is_replacement() {
            "Delete a paragraph and create an empty replacement"
        } else if ready.review.is_delete() {
            "Delete a paragraph"
        } else {
            "Insert a paragraph"
        }
    });
    let error = value.error;
    rsx! {
        section {class:"canvas-local-review","aria-label":"Review unconfirmed canvas change",
            h3 {"Review unconfirmed change"}
            p {"Slack may have applied this change. Check the canvas in Slack and close other copies of Super Platinum before continuing."}
            if let Some(action) = action {p {strong {"Saved request: "}"{action}"}}
            if value.ready.as_ref().is_some_and(|ready| ready.review.is_replacement()) {p {"Either part, both parts, or neither part may have reached Slack. Removing this lock leaves both outcomes unknown."}}
            if !text.is_empty() {div {class:"canvas-review-preview","{text}"}}
            p {"Removing this local edit lock does not confirm success, undo a change, or send it again. The saved record will remain on this computer with an unknown outcome."}
            if busy {p {role:"status","Checking the local record…"}}
            if let Some(error) = error {p {role:"alert","{error}"}}
            label {input {r#type:"checkbox",checked:acknowledged,disabled:busy||!enabled,
                onchange:move |event|{if let Some(d)=draft.write().as_mut().filter(|d|!d.busy){d.acknowledged=event.checked();}}}
                "I checked this canvas in Slack, closed other copies of Super Platinum, and want to remove only the local edit lock."}
            div {class:"canvas-text-actions",
                button {disabled:busy&&value.ready.is_some(),onclick:move |_|draft.set(None),"Cancel"}
                button {disabled:busy||!enabled||!acknowledged,onclick:move |_|remove(state,draft),"Remove local edit lock"}
            }
        }
    }
}

#[cfg(test)]
mod tests;
