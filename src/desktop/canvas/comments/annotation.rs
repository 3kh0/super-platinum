//! Root-owned section annotation and separately admitted comment-root creation.
//! Neither restoring a journal row nor refreshing a view replays either write.
use super::*;
use std::sync::Arc;
use super_platinum_core::{
    canvas::{
        comments::bootstrap::BootstrapAccepted,
        edit::{
            annotation::{
                AcknowledgedAnnotation, AnnotationAcknowledgementEvidence, AnnotationIntent,
                AnnotationObservation, VerifiedAnnotation,
            },
            structure::recovery::{
                Scope as DiskScope,
                journal::{Journal, annotations::*},
            },
        },
    },
    slack::{api::CanvasCollabSnapshot, transport::CanvasEditFailure},
};
pub(in crate::canvas) mod fixture;
pub(crate) mod heading_check;
pub(in crate::canvas) mod image;
pub(in crate::canvas) mod range;
mod run;
pub(crate) mod underline_check;
mod view;
pub(super) use view::Author;
const STORAGE: &str = "The local comment record could not be saved. Its original result is retained; retry local recovery before continuing.";
const UNKNOWN: &str = "This comment thread may already exist. Refresh to review the current canvas. It will not be created again automatically.";
#[derive(Default)]
pub(in crate::canvas) struct Root {
    entries: Vec<Entry>,
    fixture: Option<Arc<fixture::State>>,
}
#[derive(Clone, PartialEq, Eq)]
struct Disk {
    account: String,
    scope: DiskScope,
    path: Option<std::path::PathBuf>,
}
impl Disk {
    fn capture(shell: &ShellState, owner: &Owner) -> Option<Self> {
        if !owner.current(shell) {
            return None;
        }
        let account = shell.core.active_account.clone()?;
        uuid::Uuid::parse_str(&account).ok()?;
        let path = if crate::fixture::is_fixture() {
            Some(fixture::path(shell)?)
        } else {
            None
        };
        Some(Self {
            account,
            path,
            scope: DiskScope {
                workspace: owner.scope.team.clone(),
                user: shell.self_account.user_id.clone(),
                file: owner.file.clone(),
            },
        })
    }
    async fn work<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal) -> Result<T, super_platinum_core::error::AppError> + Send + 'static,
    ) -> Result<T, ()> {
        let d = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut j = match d.path {
                Some(path) => Journal::open(path, &d.account),
                None => Journal::open_default(&d.account),
            }
            .map_err(|_| ())?;
            f(&mut j).map_err(|_| ())
        })
        .await
        .map_err(|_| ())?
    }
}
struct RetainedAck {
    pending: AnnotationPending,
    proof: AcknowledgedAnnotation,
    evidence: Option<AnnotationAcknowledgementEvidence>,
}
struct Entry {
    id: String,
    disk: Disk,
    owner: Owner,
    busy: bool,
    error: Option<&'static str>,
    snapshot: Option<Arc<CanvasCollabSnapshot>>,
    choices: Vec<(String, Vec<serde_json::Value>)>,
    existing_controls: std::collections::BTreeMap<String, String>,
    page: usize,
    selected: String,
    selected_range: Option<range::Selected>,
    selected_image: Option<image::Selected>,
    pending: Option<AnnotationPending>,
    acknowledged: Option<RetainedAck>,
    verified: Option<Arc<VerifiedAnnotation>>,
    verified_receipt: Option<AnnotationReceipt>,
    retained_anchor: Option<(AnnotationPending, AnnotationAcknowledgementEvidence)>,
    accepted: Option<(AnnotationPending, Arc<BootstrapAccepted>)>,
    created: Option<Message>,
    history: Vec<AnnotationHistory>,
    history_after: Option<String>,
    history_back: Vec<Option<String>>,
    review: Option<AnnotationObservation>,
    reviewed: bool,
}
#[derive(Clone)]
struct Ticket {
    id: String,
    disk: Disk,
    owner: Owner,
}
fn entry<'a>(s: &'a ShellState, id: &str) -> Option<&'a Entry> {
    s.canvas_checklist
        .comment_annotations
        .entries
        .iter()
        .find(|e| e.id == id)
}
fn entry_mut<'a>(s: &'a mut ShellState, id: &str) -> Option<&'a mut Entry> {
    s.canvas_checklist
        .comment_annotations
        .entries
        .iter_mut()
        .find(|e| e.id == id)
}
impl Ticket {
    fn current(&self, s: &ShellState) -> bool {
        self.owner.current(s)
            && entry(s, &self.id)
                .is_some_and(|e| e.disk == self.disk && e.owner.visit == self.owner.visit)
    }
}
fn retained_ticket(shell: &ShellState) -> Option<Ticket> {
    let owner = Owner::capture(shell)?;
    let e = shell
        .canvas_checklist
        .comment_annotations
        .entries
        .iter()
        .find(|e| {
            e.owner.current(shell)
                && e.owner.visit == owner.visit
                && (e.selected_range.is_some() || e.selected_image.is_some())
                && shell.core.active_account.as_deref() == Some(e.disk.account.as_str())
        })?;
    Some(Ticket {
        id: e.id.clone(),
        disk: e.disk.clone(),
        owner,
    })
}

fn bind(mut state: Signal<ShellState>) -> Option<Ticket> {
    let (owner, disk) = {
        let shell = state.peek();
        let owner = Owner::capture(&shell)?;
        let disk = Disk::capture(&shell, &owner)?;
        (owner, disk)
    };
    let mut shell = state.write();
    let roots = &mut shell.canvas_checklist.comment_annotations.entries;
    if let Some(e) = roots.iter_mut().find(|e| e.disk == disk) {
        if e.busy {
            return None;
        }
        e.owner = owner.clone();
        e.reviewed = false;
        e.review = None;
        return Some(Ticket {
            id: e.id.clone(),
            disk,
            owner,
        });
    }
    if roots.len() >= 32 {
        return None;
    }
    let id = uuid::Uuid::new_v4().to_string();
    roots.push(Entry {
        id: id.clone(),
        disk: disk.clone(),
        owner: owner.clone(),
        busy: false,
        error: None,
        snapshot: None,
        choices: Vec::new(),
        existing_controls: Default::default(),
        page: 0,
        selected: String::new(),
        selected_range: None,
        selected_image: None,
        pending: None,
        acknowledged: None,
        verified: None,
        verified_receipt: None,
        retained_anchor: None,
        accepted: None,
        created: None,
        history: Vec::new(),
        history_after: None,
        history_back: Vec::new(),
        review: None,
        reviewed: false,
    });
    Some(Ticket { id, disk, owner })
}
fn issue(mut state: Signal<ShellState>, t: &Ticket, message: &'static str) {
    if let Some(e) = entry_mut(&mut state.write(), &t.id) {
        e.error = Some(message);
        e.busy = false;
    }
}
fn start(mut state: Signal<ShellState>, t: &Ticket) -> bool {
    let mut shell = state.write();
    if !t.current(&shell) || rate::current(&shell).is_some() {
        return false;
    }
    let Some(e) = entry_mut(&mut shell, &t.id).filter(|e| !e.busy) else {
        return false;
    };
    e.busy = true;
    e.error = None;
    e.reviewed = false;
    true
}
async fn fresh(mut state: Signal<ShellState>, t: &Ticket) -> Result<CanvasCollabSnapshot, ()> {
    if crate::fixture::is_fixture() {
        let shell = state.peek();
        if !t.current(&shell) {
            return Err(());
        }
        return fixture::canonical(&shell, &t.owner).map_err(|_| ());
    }
    let ctx = {
        let shell = state.peek();
        if !t.current(&shell) {
            return Err(());
        }
        t.owner.scope.request(&shell).ok_or(())?
    };
    let result =
        api::fetch_canvas_collab(&ctx.transport, &ctx.client, &ctx.session, &t.owner.file).await;
    if let Err(error) = &result {
        if t.owner.scope.current(&state.peek()) {
            if let Error::RateLimited { retry_after_secs } = error {
                rate::remember(&state.peek(), *retry_after_secs);
            }
            state.write().report_failure(
                error,
                "You are offline. The comment remains retained.",
                || "This canvas request could not be completed.".into(),
            );
        }
    }
    if !t.current(&state.peek()) {
        return Err(());
    }
    result.map_err(|_| ())
}
fn refresh(mut state: Signal<ShellState>, t: Ticket) {
    if !start(state, &t) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let scope = t.disk.scope.clone();
        let inventory = t
            .disk
            .work(move |j| Ok((j.annotation(&scope)?, j.annotation_history(&scope, None)?)))
            .await;
        let snapshot = fresh(state, &t).await;
        let projection = match snapshot {
            Ok(s) => tokio::task::spawn_blocking(move || {
                use super_platinum_core::canvas::edit::annotation::{
                    AnnotationPresentation, section_previews,
                };
                let presentation = AnnotationPresentation::ordinary(None, None).map_err(|_| ())?;
                let choices = section_previews(&s, &presentation).map_err(|_| ())?;
                Ok::<_, ()>((s, choices))
            })
            .await
            .map_err(|_| ())
            .and_then(|v| v),
            Err(()) => Err(()),
        };
        let mut shell = state.write();
        let current = t.current(&shell);
        if let Some(e) = entry_mut(&mut shell, &t.id) {
            e.busy = false;
            match inventory {
                Ok((pending, history)) => {
                    e.pending = pending;
                    e.history = history;
                    e.history_after = None;
                    e.history_back.clear();
                }
                Err(()) => {
                    e.error = Some(STORAGE);
                    return;
                }
            }
            if current {
                match projection {
                    Ok((s, choices)) => {
                        e.review = e
                            .pending
                            .as_ref()
                            .and_then(|p| p.receipt().record().review_current(&s).ok());
                        e.snapshot = Some(Arc::new(s));
                        e.existing_controls = choices
                            .iter()
                            .filter_map(|c| {
                                c.existing_control
                                    .as_ref()
                                    .map(|id| (c.id.clone(), id.clone()))
                            })
                            .collect();
                        e.choices = choices.into_iter().map(|c| (c.id, c.blocks)).collect();
                        e.page = 0;
                        e.selected.clear();
                        e.selected_range = None;
                        e.selected_image = None;
                    }
                    Err(()) => {
                        e.error =
                            Some("The current canvas could not be checked. No comment was created.")
                    }
                }
            }
        }
    });
}
pub(in crate::canvas) fn pending_for_current_file(shell: &ShellState) -> bool {
    shell
        .canvas_checklist
        .comment_annotations
        .entries
        .iter()
        .any(|e| {
            e.owner.scope.stable_current(shell)
                && shell.canvas.as_ref().is_some_and(|f| f.id == e.owner.file)
                && (e.busy
                    || e.pending.is_some()
                    || e.accepted.is_some()
                    || e.acknowledged.is_some()
                    || e.verified.is_some())
        })
}
pub(super) fn carry_pending(old: &mut ShellState, next: &mut ShellState) {
    if next.canvas_checklist.comment_annotations.fixture.is_none() {
        next.canvas_checklist.comment_annotations.fixture =
            old.canvas_checklist.comment_annotations.fixture.take();
    }
    next.canvas_checklist
        .comment_annotations
        .entries
        .append(&mut old.canvas_checklist.comment_annotations.entries);
}

/// Page durable evidence without reading Slack or changing an attempt.
fn history_page(mut state: Signal<ShellState>, t: Ticket, forward: bool) {
    let (after, back) = {
        let shell = state.peek();
        if !t.current(&shell) {
            return;
        }
        let Some(e) = entry(&shell, &t.id).filter(|e| !e.busy) else {
            return;
        };
        let mut back = e.history_back.clone();
        let after = if forward {
            if e.history.len() != 20 {
                return;
            }
            let Some(last) = e.history.last() else {
                return;
            };
            back.push(e.history_after.clone());
            Some(last.id.clone())
        } else {
            let Some(previous) = back.pop() else {
                return;
            };
            previous
        };
        (after, back)
    };
    if !start(state, &t) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let scope = t.disk.scope.clone();
        let cursor = after.clone();
        let rows = t
            .disk
            .work(move |j| j.annotation_history(&scope, cursor.as_deref()))
            .await;
        if let Some(e) = entry_mut(&mut state.write(), &t.id) {
            e.busy = false;
            match rows {
                Ok(rows) => {
                    e.history = rows;
                    e.history_after = after;
                    e.history_back = back;
                }
                Err(()) => e.error = Some(STORAGE),
            }
        }
    });
}
/// Release only a settled, durably represented display slot. Outstanding work
/// and actual unsaved proofs always remain charged to their original owner.
fn close(mut state: Signal<ShellState>, t: &Ticket) -> bool {
    let mut shell = state.write();
    if !t.current(&shell) {
        return false;
    }
    let entries = &mut shell.canvas_checklist.comment_annotations.entries;
    let Some(index) = entries.iter().position(|e| e.id == t.id) else {
        return false;
    };
    let e = &entries[index];
    if e.busy
        || e.pending.is_some()
        || e.acknowledged.is_some()
        || e.verified.is_some()
        || e.accepted.is_some()
    {
        return false;
    }
    entries.remove(index);
    true
}
