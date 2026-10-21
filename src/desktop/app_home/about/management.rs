//! Owned workflow metadata editing. Reads never substitute for write receipts.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::block_actions::{
    home::about::WorkflowEntry,
    workflow_management::{
        self as protocol, Editor, EditorLoad, List, ListPage, ListQuery, ListWorkflows,
        MutationPlan, Observation, Outcome, Sort,
    },
};
pub(crate) mod activity;
pub(crate) mod delete;
pub(crate) mod entry;
pub(crate) mod fixture;
mod native;
mod owner;
pub(crate) mod permissions;
pub(crate) mod unpublish;
use owner::Owner;
mod policy;
mod read;
mod view;
mod write;
pub(crate) use view::panel;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct State {
    surface: Option<Surface>,
    documents: Vec<Document>,
    attempts: Vec<write::Attempt>,
    deletions: delete::Book,
    unpublishing: unpublish::Book,
    permissions: permissions::Book,
    fixture: Option<fixture::Model>,
    exports: Vec<activity::export::Attempt>,
    export_fixture: Option<activity::export::fixture::Model>,
    activity_fixture: Option<activity::fixture::Model>,
    activity_member_fixture: Option<activity::members::fixture::Model>,
}
struct Surface {
    id: u64,
    about: Option<Key>,
    entry: Option<entry::Target>,
    owner: Owner,
    account: Option<String>,
    query: ListQuery,
    list: List,
    document: Option<u64>,
    read: Option<u64>,
    policy: Option<policy::Snapshot>,
    notice: Option<String>,
    fixture: bool,
}
#[derive(Clone)]
struct Document {
    activity: activity::State,
    id: u64,
    account: Option<String>,
    team: String,
    user: String,
    editor: Editor,
    title: String,
    description: String,
    revision: u64,
    observed: Option<Editor>,
    pending: Option<u64>,
    notice: Option<String>,
}
impl Document {
    fn dirty(&self) -> bool {
        self.editor.raw_workflow()["title"].as_str().unwrap_or("") != self.title
            || self.editor.raw_workflow()["description"]
                .as_str()
                .unwrap_or("")
                != self.description
    }
    fn adopt(&mut self, editor: Editor) {
        self.activity = activity::State::default();
        self.title = editor.raw_workflow()["title"].as_str().unwrap_or("").into();
        self.description = editor.raw_workflow()["description"]
            .as_str()
            .unwrap_or("")
            .into();
        self.editor = editor;
        self.observed = None;
        self.revision = next();
    }
}
fn unavailable() -> Error {
    Error::Api("workflow_metadata_currentness_changed".into())
}
fn visible(s: &ShellState, visit: u64) -> bool {
    s.app_home
        .about
        .management
        .surface
        .as_ref()
        .is_some_and(|h| {
            h.id == visit
                && h.about.is_none_or(|key| current(s, key))
                && h.owner.current(s)
                && h.account == s.core.active_account
        })
}
fn admitted(s: &ShellState, visit: u64) -> bool {
    visible(s, visit)
        && s.overlay.is_none()
        && s.app_home
            .about
            .management
            .surface
            .as_ref()
            .is_some_and(|h| !cooling(s, &h.owner.ctx))
}
fn read_current(s: &ShellState, visit: u64, id: u64) -> bool {
    admitted(s, visit)
        && s.app_home
            .about
            .management
            .surface
            .as_ref()
            .is_some_and(|h| h.read == Some(id))
}
pub(super) fn showing(s: &ShellState) -> bool {
    s.app_home
        .about
        .management
        .surface
        .as_ref()
        .is_some_and(|h| visible(s, h.id))
}
fn document(s: &ShellState, visit: u64) -> Option<&Document> {
    if !visible(s, visit) {
        return None;
    }
    let h = s.app_home.about.management.surface.as_ref()?;
    s.app_home.about.management.documents.iter().find(|d| {
        Some(d.id) == h.document
            && d.account == h.account
            && d.team == h.owner.ctx.session.team_id
            && d.user == h.owner.ctx.session.user_id
    })
}
fn document_mut(s: &mut ShellState, visit: u64) -> Option<&mut Document> {
    let id = document(s, visit)?.id;
    s.app_home
        .about
        .management
        .documents
        .iter_mut()
        .find(|d| d.id == id)
}
pub(super) fn capture(s: &ShellState, key: Key) -> Option<WorkflowEntry> {
    if !current(s, key) || s.overlay.is_some() {
        return None;
    }
    let h = s.app_home.about.surface.as_ref()?;
    if h.loading || cooling(s, &h.owner.ctx) {
        return None;
    }
    let user = s
        .core
        .workspaces
        .get(&h.owner.ctx.session.team_id)?
        .users
        .get(&h.owner.ctx.session.user_id)?;
    h.profile.as_ref()?.workflow_entry(
        h.info.as_ref()?,
        &user.id,
        h.owner
            .ctx
            .session
            .enterprise_id
            .as_deref()
            .unwrap_or(&h.owner.ctx.session.team_id),
        user.is_restricted || user.is_ultra_restricted,
        h.flags?.parity,
    )
}
pub(super) fn open(mut state: Signal<ShellState>, key: Key) {
    let (visit, entry) = {
        let mut s = state.write();
        let Some(entry) = capture(&s, key) else {
            return;
        };
        if showing(&s) {
            return;
        }
        let h = s.app_home.about.surface.as_ref().unwrap();
        let surface = Surface {
            id: next(),
            about: Some(key),
            entry: None,
            owner: h.owner.clone().into(),
            account: s.core.active_account.clone(),
            query: ListQuery::default(),
            list: List::default(),
            document: None,
            read: None,
            policy: None,
            notice: None,
            fixture: h.fixture,
        };
        let visit = surface.id;
        s.app_home.about.management.surface = Some(surface);
        (visit, entry)
    };
    match entry {
        WorkflowEntry::Managed => read::list(state, visit, false),
        WorkflowEntry::Editor { app } => read::resolve(state, visit, app),
    }
}
fn edit(mut state: Signal<ShellState>, visit: u64, title: bool, value: String) {
    let mut s = state.write();
    if !admitted(&s, visit)
        || s.app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .read
            .is_some()
    {
        return;
    }
    if delete::confirming(&s, visit)
        || unpublish::confirming(&s, visit)
        || permissions::confirming(&s, visit)
    {
        return;
    }
    if let Some(d) =
        document_mut(&mut s, visit).filter(|d| d.pending.is_none() && !d.activity.active)
    {
        if title {
            d.title = value;
        } else {
            d.description = value;
        }
        d.revision = next();
        d.notice = None;
    }
}
fn editable(s: &ShellState, visit: u64) -> bool {
    let Some(d) = document(s, visit) else {
        return false;
    };
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    admitted(s, visit)
        && h.read.is_none()
        && d.pending.is_none()
        && !delete::confirming(s, visit)
        && !unpublish::confirming(s, visit)
        && !permissions::confirming(s, visit)
        && !d.activity.active
        && h.policy
            .as_ref()
            .is_some_and(|p| p.current(s) && p.can_create())
        && allowed_editor(&d.editor, &h.owner.ctx.session.user_id)
}
fn allowed_editor(e: &Editor, user: &str) -> bool {
    e.is_collaborator(user)
        && !e.has_unavailable_steps()
        && e.restriction().is_some_and(|r| r["is_restricted"] == false)
        && e.trigger().is_none_or(|t| {
            !matches!(t["type"].as_str(), Some("shortcut" | "webhook"))
                || t["available_data"].is_object()
        })
}
pub(crate) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    let old = &mut previous.app_home.about.management;
    old.deletions.retire_confirmation();
    old.unpublishing.retire_confirmation();
    old.permissions.retire_confirmation();
    loaded
        .app_home
        .about
        .management
        .permissions
        .attempts
        .append(&mut old.permissions.attempts);
    loaded
        .app_home
        .about
        .management
        .unpublishing
        .attempts
        .append(&mut old.unpublishing.attempts);
    loaded
        .app_home
        .about
        .management
        .deletions
        .attempts
        .append(&mut old.deletions.attempts);
    // Keep drafts and independent slot receipts through the actual shell swap.
    // The old surface/credential owner cannot authorize any new operation.
    loaded
        .app_home
        .about
        .management
        .documents
        .append(&mut old.documents);
    loaded
        .app_home
        .about
        .management
        .attempts
        .append(&mut old.attempts);
    loaded
        .app_home
        .about
        .management
        .exports
        .append(&mut old.exports);
}
