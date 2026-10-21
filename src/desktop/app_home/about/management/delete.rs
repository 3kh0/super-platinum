//! Source-owned Delete confirmation and independent, non-replayed receipts.
use super::*;
use crate::app_home::inputs::native::Release;
mod native;
mod receipt;
mod run;
pub(super) use receipt::outcomes;
pub(crate) use receipt::snapshot;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct Book {
    confirmation: Option<Confirmation>,
    pub(super) attempts: Vec<Attempt>,
}
impl Book {
    pub(super) fn retire_confirmation(&mut self) {
        if let Some(c) = self.confirmation.take() {
            c.release.release();
        }
    }
}
#[derive(Clone)]
struct Original {
    id: u64,
    visit: u64,
    document: u64,
    revision: u64,
    title: String,
    description: String,
    editor: Editor,
    policy: policy::Snapshot,
    owner: Owner,
}
struct Confirmation {
    original: Original,
    release: Release,
}
pub(super) struct Attempt {
    original: Original,
    lease: read::Lease,
    plan: MutationPlan,
    replies: Vec<Option<Reply>>,
    finished: bool,
    terminal: bool,
    no_send: bool,
    release: Option<Release>,
}
enum Reply {
    Raw(Value),
    Failure(&'static str),
}
impl Original {
    fn capture(s: &ShellState, visit: u64) -> Option<Self> {
        if !editable(s, visit) {
            return None;
        }
        let d = document(s, visit)?;
        let h = s.app_home.about.management.surface.as_ref()?;
        Some(Self {
            id: next(),
            visit,
            document: d.id,
            revision: d.revision,
            title: d.title.clone(),
            description: d.description.clone(),
            editor: d.editor.clone(),
            policy: h.policy.clone()?,
            owner: h.owner.clone(),
        })
    }
    fn current(&self, s: &ShellState, pending: Option<u64>) -> bool {
        admitted(s, self.visit)
            && self.owner.current(s)
            && self.policy.current(s)
            && self.policy.can_create()
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| h.policy.as_ref().is_some_and(|p| p.same(&self.policy)))
            && document(s, self.visit).is_some_and(|d| {
                d.id == self.document
                    && d.revision == self.revision
                    && d.title == self.title
                    && d.description == self.description
                    && d.pending == pending
                    && !d.activity.active
                    && self.editor.same_revision(&d.editor)
                    && allowed_editor(&d.editor, &self.owner.ctx.session.user_id)
            })
    }
    fn expected(&self) -> Value {
        json!({"id":self.id,"visit":self.visit,"document":self.document,"revision":self.revision,
            "title":self.title,"description":self.description})
    }
}
pub(super) fn confirming(s: &ShellState, visit: u64) -> bool {
    s.app_home
        .about
        .management
        .deletions
        .confirmation
        .as_ref()
        .is_some_and(|c| c.original.visit == visit)
}
pub(super) fn confirmation(s: &ShellState, visit: u64) -> Option<u64> {
    s.app_home
        .about
        .management
        .deletions
        .confirmation
        .as_ref()
        .filter(|c| c.original.visit == visit && c.original.current(s, None))
        .map(|c| c.original.id)
}
pub(super) fn retained(book: &Book, pending: Option<u64>) -> bool {
    pending.is_some_and(|id| book.attempts.iter().any(|a| a.original.id == id))
}
pub(super) fn owns(s: &ShellState, visit: u64) -> bool {
    retained(
        &s.app_home.about.management.deletions,
        document(s, visit).and_then(|d| d.pending),
    )
}
pub(super) fn terminal(s: &ShellState, visit: u64) -> bool {
    let id = document(s, visit).and_then(|d| d.pending);
    s.app_home
        .about
        .management
        .deletions
        .attempts
        .iter()
        .any(|a| Some(a.original.id) == id && a.terminal)
}
pub(super) fn open(mut state: Signal<ShellState>, visit: u64) {
    let Some(original) = Original::capture(&state.peek(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some(release) = native::lock(&original).await else {
            return;
        };
        let mut s = state.write();
        if !original.current(&s, None) || !editable(&s, visit) {
            release.release();
            return;
        }
        s.app_home.about.management.deletions.retire_confirmation();
        let id = original.id;
        s.app_home.about.management.deletions.confirmation =
            Some(Confirmation { original, release });
        drop(s);
        // Only an open confirmation has this bounded ownership watcher. It
        // cannot keep a replaced view locked or authorize another request.
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let retired = {
                let s = state.peek();
                let Some(c) = s
                    .app_home
                    .about
                    .management
                    .deletions
                    .confirmation
                    .as_ref()
                    .filter(|c| c.original.id == id)
                else {
                    break;
                };
                !c.original.current(&s, None)
            };
            if retired {
                let mut s = state.write();
                if s.app_home
                    .about
                    .management
                    .deletions
                    .confirmation
                    .as_ref()
                    .is_some_and(|c| c.original.id == id)
                {
                    s.app_home.about.management.deletions.retire_confirmation();
                }
                break;
            }
        }
    });
}
pub(super) fn cancel(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let original = {
        let s = state.peek();
        let Some(c) = s
            .app_home
            .about
            .management
            .deletions
            .confirmation
            .as_ref()
            .filter(|c| c.original.visit == visit && c.original.id == id)
        else {
            return;
        };
        c.original.clone()
    };
    dioxus::core::spawn_forever(async move {
        // A failed witness must never cause a controlled-value render over a
        // native buffer that has not reached serial state.
        if !native::current(&original).await {
            return;
        }
        let mut s = state.write();
        if !original.current(&s, None) || confirmation(&s, visit) != Some(id) {
            return;
        }
        let c = s
            .app_home
            .about
            .management
            .deletions
            .confirmation
            .take()
            .unwrap();
        document_mut(&mut s, visit).unwrap().revision = next();
        c.release.release();
    });
}
pub(super) fn confirm(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let original = {
        let s = state.peek();
        let Some(c) = s
            .app_home
            .about
            .management
            .deletions
            .confirmation
            .as_ref()
            .filter(|c| {
                c.original.id == id && c.original.visit == visit && c.original.current(&s, None)
            })
        else {
            return;
        };
        c.original.clone()
    };
    dioxus::core::spawn_forever(async move {
        if !native::current(&original).await {
            return;
        }
        let admitted = {
            let mut s = state.write();
            if !original.current(&s, None)
                || s.app_home.about.management.deletions.attempts.len() >= 128
                || confirmation(&s, visit) != Some(id)
            {
                return;
            }
            let Ok(plan) = MutationPlan::new(&original.editor, protocol::Command::Delete) else {
                return;
            };
            let c = s
                .app_home
                .about
                .management
                .deletions
                .confirmation
                .take()
                .unwrap();
            let Some(lease) = read::Lease::begin(&mut s, visit) else {
                c.release.release();
                return;
            };
            document_mut(&mut s, visit).unwrap().pending = Some(id);
            s.app_home
                .about
                .management
                .deletions
                .attempts
                .push(Attempt {
                    replies: (0..plan.slots().len()).map(|_| None).collect(),
                    original,
                    lease,
                    plan,
                    finished: false,
                    terminal: false,
                    no_send: false,
                    release: Some(c.release),
                });
            true
        };
        if admitted {
            run::run(state, id).await;
        }
    });
}
fn exact(s: &ShellState, id: u64) -> bool {
    s.app_home
        .about
        .management
        .deletions
        .attempts
        .iter()
        .find(|a| a.original.id == id)
        .is_some_and(|a| !a.finished && a.lease.current(s) && a.original.current(s, Some(id)))
}
