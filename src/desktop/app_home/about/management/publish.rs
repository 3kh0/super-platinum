//! Direct publication of acquired, previously published workflows. Validation,
//! delivery and independent readback are deliberately separate observations.
use super::*;
use crate::app_home::inputs::native::Release;
mod native;
mod readback;
mod receipt;
mod run;
mod validation;
pub(super) use readback::check;
pub(super) use receipt::outcomes;
pub(crate) use receipt::snapshot;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct Book {
    pub(super) attempts: Vec<Attempt>,
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
pub(super) struct Attempt {
    original: Original,
    lease: read::Lease,
    plan: MutationPlan,
    replies: Vec<Option<Reply>>,
    validation: validation::Facts,
    validation_only: bool,
    finished: bool,
    complete: bool,
    resolved: bool,
    followup: Option<protocol::FollowUpPlan>,
    follow_replies: Vec<Vec<Option<Reply>>>,
    readback: Option<(Editor, bool)>,
    no_send: bool,
    release: Option<Release>,
}
enum Reply {
    Raw(Value),
    Failure(&'static str),
}
impl Original {
    fn capture(s: &ShellState, visit: u64) -> Option<Self> {
        if !available(s, visit) {
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
fn supported(editor: &Editor) -> bool {
    editor.workflow_id().strip_prefix("Wf").is_some_and(|tail| {
        tail.len() >= 7
            && tail
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    }) && caption(editor).is_some()
        && editor.trigger().is_some_and(|t| t["type"] == "shortcut")
}
fn prior_publication_caption(raw: &Value) -> Option<&'static str> {
    if raw["last_published_version_id"].as_str()?.is_empty() {
        return None;
    }
    let published = raw["is_published"].as_bool()?;
    let count = raw["unpublished_change_count"].as_u64()?;
    match (published, count) {
        (true, 0) => None,
        (false, 0) => Some("Publish"),
        (_, _) => Some("Publish Changes"),
    }
}
pub(super) fn caption(editor: &Editor) -> Option<&'static str> {
    prior_publication_caption(editor.raw_workflow())
}
fn ready_notice(editor: &Editor) -> &'static str {
    if caption(editor) == Some("Publish") {
        "Publication checks are ready. Use Publish to publish the saved workflow."
    } else {
        "Publication checks are ready. Use Publish Changes to publish the saved workflow."
    }
}
pub(super) fn shown(s: &ShellState, visit: u64) -> bool {
    document(s, visit).is_some_and(|d| supported(&d.editor))
}
pub(super) fn available(s: &ShellState, visit: u64) -> bool {
    editable(s, visit) && document(s, visit).is_some_and(|d| supported(&d.editor) && !d.dirty())
}
pub(super) fn retained(book: &Book, pending: Option<u64>) -> bool {
    pending.is_some_and(|id| book.attempts.iter().any(|a| a.original.id == id))
}
pub(super) fn owns(s: &ShellState, visit: u64) -> bool {
    retained(
        &s.app_home.about.management.publishing,
        document(s, visit).and_then(|d| d.pending),
    )
}
pub(super) fn retry_available(s: &ShellState, visit: u64) -> bool {
    available(s, visit)
        && document(s, visit).is_some_and(|d| {
            s.app_home
                .about
                .management
                .publishing
                .attempts
                .iter()
                .rev()
                .find(|a| a.original.document == d.id)
                .is_some_and(|a| a.finished && a.no_send && a.validation.status == "failed")
        })
}
pub(super) fn start(mut state: Signal<ShellState>, visit: u64, validation_only: bool) {
    if validation_only && !retry_available(&state.peek(), visit) {
        return;
    }
    let Some(original) = Original::capture(&state.peek(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some(release) = native::lock(&original).await else {
            return;
        };
        let id = original.id;
        {
            let mut s = state.write();
            if !original.current(&s, None)
                || !available(&s, visit)
                || s.app_home.about.management.publishing.attempts.len() >= 128
            {
                release.release();
                return;
            }
            let Ok(plan) = MutationPlan::new(&original.editor, protocol::Command::Published(true))
            else {
                release.release();
                return;
            };
            let Some(lease) = read::Lease::begin(&mut s, visit) else {
                release.release();
                return;
            };
            document_mut(&mut s, visit).unwrap().pending = Some(id);
            s.app_home
                .about
                .management
                .publishing
                .attempts
                .push(Attempt {
                    replies: (0..plan.slots().len()).map(|_| None).collect(),
                    original,
                    lease,
                    plan,
                    validation: validation::Facts::default(),
                    validation_only,
                    finished: false,
                    complete: false,
                    resolved: false,
                    followup: None,
                    follow_replies: vec![],
                    readback: None,
                    no_send: false,
                    release: Some(release),
                });
        }
        run::run(state, id).await;
    });
}
fn attempt(s: &ShellState, id: u64) -> Option<&Attempt> {
    s.app_home
        .about
        .management
        .publishing
        .attempts
        .iter()
        .find(|a| a.original.id == id)
}
fn attempt_mut(s: &mut ShellState, id: u64) -> Option<&mut Attempt> {
    s.app_home
        .about
        .management
        .publishing
        .attempts
        .iter_mut()
        .find(|a| a.original.id == id)
}
fn exact(s: &ShellState, id: u64) -> bool {
    attempt(s, id)
        .is_some_and(|a| !a.finished && a.lease.current(s) && a.original.current(s, Some(id)))
}
