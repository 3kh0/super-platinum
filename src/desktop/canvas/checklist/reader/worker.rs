//! One coalesced mapping worker per reader. Only immutable canonical data leaves
//! the UI task; all publication is revalidated against the exact current owner.
use super::*;
use crate::canvas::{Reader, text};

#[derive(Default)]
pub(in crate::canvas) struct State {
    owner: Option<Arc<Owner>>,
    queued: bool,
    running: Option<Arc<Owner>>,
}
struct Owner {
    scope: Scope,
    file: String,
    visit: u64,
    read: u64,
    revision: u64,
    loading: bool,
    stale: bool,
    generation: u64,
    snapshot: Option<Arc<CanvasCollabSnapshot>>,
    document: Option<Arc<CanvasDocument>>,
}
fn same<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}
impl Owner {
    fn capture(shell: &ShellState, reader: &Reader) -> Option<Self> {
        let view = shell
            .canvas_checklist
            .view
            .as_ref()
            .filter(|view| view_current(shell, view))?;
        Some(Self {
            scope: view.scope.clone(),
            file: view.file.clone(),
            visit: view.generation,
            read: view.read,
            revision: view.revision,
            loading: view.loading,
            stale: view.stale,
            generation: reader.generation,
            snapshot: view.snapshot.clone(),
            document: reader.document.clone(),
        })
    }
    fn current(&self, shell: &ShellState, reader: &Reader) -> bool {
        let Some(view) = shell.canvas_checklist.view.as_ref() else {
            return false;
        };
        self.scope.current(shell)
            && view_current(shell, view)
            && self.file == view.file
            && self.visit == view.generation
            && self.read == view.read
            && self.revision == view.revision
            && self.loading == view.loading
            && self.stale == view.stale
            && self.generation == reader.generation
            && same(&self.snapshot, &view.snapshot)
            && same(&self.document, &reader.document)
    }
    fn input(&self) -> Option<Input> {
        if self.loading || self.stale || self.snapshot.as_ref()?.file_id() != self.file {
            return None;
        }
        Some(Input {
            document: self.document.clone()?,
            snapshot: self.snapshot.clone()?,
        })
    }
}
struct Input {
    document: Arc<CanvasDocument>,
    snapshot: Arc<CanvasCollabSnapshot>,
}
struct Prepared {
    checklist: Mapping,
    text: text::Prepared,
}
impl Input {
    fn run(self) -> Prepared {
        Prepared {
            checklist: Mapping::project(&self.document, Some(&self.snapshot)),
            text: text::Prepared::project(&self.document, &self.snapshot),
        }
    }
}
impl State {
    pub(in crate::canvas) fn ready(&self) -> bool {
        self.queued && self.running.is_none()
    }
    fn claim(&mut self) -> Option<(Arc<Owner>, Input)> {
        if !self.ready() {
            return None;
        }
        self.queued = false;
        let owner = self.owner.clone()?;
        let input = owner.input()?;
        self.running = Some(owner.clone());
        Some((owner, input))
    }
}
pub(in crate::canvas) fn needed(shell: &ShellState, reader: &Reader) -> bool {
    match &reader.mapping.owner {
        Some(owner) => !owner.current(shell, reader),
        None => shell
            .canvas_checklist
            .view
            .as_ref()
            .is_some_and(|view| view_current(shell, view)),
    }
}
pub(super) fn queue(shell: &ShellState, reader: &mut Reader) {
    reader.mapping.owner = Owner::capture(shell, reader).map(Arc::new);
    reader.mapping.queued = reader
        .mapping
        .owner
        .as_ref()
        .is_some_and(|owner| owner.input().is_some());
}
fn finish(
    shell: &ShellState,
    reader: &mut Reader,
    owner: &Arc<Owner>,
    prepared: Option<Prepared>,
) -> bool {
    if !reader
        .mapping
        .running
        .as_ref()
        .is_some_and(|running| Arc::ptr_eq(running, owner))
    {
        return false;
    }
    reader.mapping.running = None;
    if !reader
        .mapping
        .owner
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, owner))
        || !owner.current(shell, reader)
    {
        return false;
    }
    let Some(prepared) = prepared else {
        return false;
    };
    let mut text = text::Mapping::assemble(shell, prepared.text, true);
    let mut checklist = prepared.checklist;
    let token = Some((owner.visit, owner.read));
    text.owner = token;
    checklist.owner = token;
    reader.text = text;
    reader.checklist = checklist;
    true
}
pub(in crate::canvas) fn start(
    state: Signal<ShellState>,
    mut reader: Signal<Reader>,
    mut busy: Signal<bool>,
) {
    if *busy.peek() {
        return;
    }
    let Some((owner, input)) = reader.write().mapping.claim() else {
        return;
    };
    busy.set(true);
    // Dioxus owns/cancels this task with the Reader's component. No signal or
    // media registry crosses into spawn_blocking; a panic leaves actions empty.
    spawn(async move {
        let prepared = tokio::task::spawn_blocking(move || input.run()).await.ok();
        finish(&state.peek(), &mut reader.write(), &owner, prepared);
        busy.set(false);
    });
}

#[cfg(test)]
pub(in crate::canvas) fn settle_for_test(shell: &ShellState, reader: &mut Reader) -> bool {
    let Some((owner, input)) = reader.mapping.claim() else {
        return false;
    };
    finish(shell, reader, &owner, Some(input.run()))
}

#[cfg(test)]
mod tests;
