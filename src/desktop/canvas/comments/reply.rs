//! Existing-root rich replies. Root-owned entries outlive panel/account visits.
use super::*;
use std::path::PathBuf;
use super_platinum_core::{
    canvas::{
        comments::reply::{ReplyAccepted, ReplyIntent},
        edit::structure::recovery::{
            Scope as DiskScope,
            journal::{
                Journal,
                replies::{ReplyDraft, ReplyHistory, ReplyInventory, ReplyReceipt, ReplySave},
            },
        },
    },
    slack::{models::File, transport::CanvasEditFailure},
};
pub(super) mod check;
mod delivery;
mod departure;
pub(super) mod fixture;
mod guarded_departure;
mod native;
mod view;
pub(super) use departure::back_to_canvas;
pub(super) use guarded_departure::prepare as prepare_departure;
pub(super) use view::Composer;
#[cfg(test)]
mod tests;
#[derive(Default)]
pub(in crate::canvas) struct Root {
    entries: Vec<Entry>,
}
pub(super) fn carry_pending(previous: &mut ShellState, loaded: &mut ShellState) {
    loaded
        .canvas_checklist
        .comment_replies
        .entries
        .append(&mut previous.canvas_checklist.comment_replies.entries);
}

#[derive(Clone, PartialEq, Eq)]
struct Disk {
    account: String,
    scope: DiskScope,
    path: Option<PathBuf>,
}
#[derive(Clone)]
struct Binding {
    id: String,
    owner: Owner,
    read: u64,
    root: String,
}
struct Entry {
    id: String,
    disk: Disk,
    root: String,
    text: String,
    blocks: Option<Vec<serde_json::Value>>,
    native: Option<native::Mounted>,
    serial: u64,
    capture: String,
    inventory: Option<ReplyInventory>,
    loading: bool,
    saving: bool,
    sending: bool,
    composing: bool,
    native_busy: bool,
    error: Option<&'static str>,
    reviewed: bool,
    outcome: Option<Outcome>,
    locked: Option<ReplyReceipt>,
    history: Option<Vec<ReplyHistory>>,
    retired_text: Option<String>,
    retired_blocks: Option<Vec<serde_json::Value>>,
}
enum Outcome {
    NotSent(ReplyReceipt),
    Accepted(ReplyReceipt, ReplyAccepted),
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
            scope: DiskScope {
                workspace: owner.scope.team.clone(),
                user: shell.self_account.user_id.clone(),
                file: owner.file.clone(),
            },
            path,
        })
    }
    async fn work<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal) -> Result<T, super_platinum_core::error::AppError> + Send + 'static,
    ) -> Result<T, ()> {
        let disk = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut j = match disk.path {
                Some(p) => Journal::open(p, &disk.account),
                None => Journal::open_default(&disk.account),
            }
            .map_err(|_| ())?;
            f(&mut j).map_err(|_| ())
        })
        .await
        .map_err(|_| ())?
    }
}
fn adopt(e: &mut Entry, inventory: ReplyInventory) {
    if e.inventory
        .as_ref()
        .is_none_or(|old| old.revision() <= inventory.revision())
    {
        e.locked = inventory.attempt().cloned();
        e.inventory = Some(inventory);
    }
}
fn entry<'a>(shell: &'a ShellState, id: &str) -> Option<&'a Entry> {
    shell
        .canvas_checklist
        .comment_replies
        .entries
        .iter()
        .find(|e| e.id == id)
}
fn entry_mut<'a>(shell: &'a mut ShellState, id: &str) -> Option<&'a mut Entry> {
    shell
        .canvas_checklist
        .comment_replies
        .entries
        .iter_mut()
        .find(|e| e.id == id)
}
impl Binding {
    fn current(&self, shell: &ShellState, model: &State) -> bool {
        self.owner.current(shell)
            && model.serial == self.read
            && model.query == Some(Query::Replies(self.root.clone()))
            && entry(shell, &self.id)
                .is_some_and(|e| e.root == self.root && e.disk.scope.file == self.owner.file)
    }
    fn field_scope(&self) -> String {
        format!("{}:{}:{}", self.id, self.owner.visit, self.read)
    }
}
fn durably_idle(e: &Entry) -> bool {
    e.native.is_none()
        && !e.loading
        && !e.saving
        && !e.sending
        && e.error.is_none()
        && e.outcome.is_none()
        && !e.composing
        && !e.native_busy
        && e.inventory.as_ref().is_some_and(|i| match i.drafts() {
            [] => e.text.is_empty(),
            [d] => {
                e.text == d.text() && e.capture == d.capture() && e.blocks.as_deref() == d.blocks()
            }
            _ => false,
        })
}
fn bind(mut state: Signal<ShellState>, model: Signal<State>) -> Option<Binding> {
    let (owner, root, read, disk) = {
        let shell = state.peek();
        let value = model.peek();
        let Query::Replies(root) = value.query.as_ref()? else {
            return None;
        };
        let owner = value.owner.as_ref()?.clone();
        let disk = Disk::capture(&shell, &owner)?;
        (owner, root.clone(), value.serial, disk)
    };
    let existing = state
        .peek()
        .canvas_checklist
        .comment_replies
        .entries
        .iter()
        .find(|e| e.disk == disk && e.root == root)
        .map(|e| e.id.clone());
    let id = match existing {
        Some(id) => id,
        None => {
            // Reclaim only entries exactly represented by durable inventory. Retained
            // unsaved/errors/results are never evicted to make room.
            state
                .write()
                .canvas_checklist
                .comment_replies
                .entries
                .retain(|e| !durably_idle(e));
            if state.peek().canvas_checklist.comment_replies.entries.len() >= 32 {
                return None;
            }
            let id = uuid::Uuid::new_v4().to_string();
            state
                .write()
                .canvas_checklist
                .comment_replies
                .entries
                .push(Entry {
                    id: id.clone(),
                    disk,
                    root: root.clone(),
                    text: String::new(),
                    blocks: None,
                    native: None,
                    serial: 0,
                    capture: uuid::Uuid::new_v4().to_string(),
                    inventory: None,
                    loading: false,
                    saving: false,
                    sending: false,
                    composing: false,
                    native_busy: false,
                    error: None,
                    reviewed: false,
                    outcome: None,
                    locked: None,
                    history: None,
                    retired_text: None,
                    retired_blocks: None,
                });
            id
        }
    };
    let binding = Binding {
        id,
        owner,
        read,
        root,
    };
    load(state, binding.clone());
    Some(binding)
}
fn load(mut state: Signal<ShellState>, binding: Binding) {
    let request = {
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.inventory.is_some() || e.loading {
            return;
        }
        e.loading = true;
        (e.disk.clone(), e.root.clone(), e.serial)
    };
    dioxus::core::spawn_forever(async move {
        let root = request.1;
        let scope = request.0.scope.clone();
        let result = request
            .0
            .work(move |j| {
                fixture::seed_rich(j, &scope, &root)?;
                j.reply_inventory(&scope, &root)
            })
            .await;
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        e.loading = false;
        match result {
            Ok(inventory) => {
                if e.serial == request.2 {
                    e.text = if let [draft] = inventory.drafts() {
                        e.capture = draft.capture().into();
                        e.blocks = draft.blocks().map(<[_]>::to_vec);
                        draft.text().into()
                    } else {
                        String::new()
                    };
                }
                adopt(e, inventory);
                e.error = None
            }
            Err(()) => {
                e.error = Some("The local reply draft could not be loaded. Retry local save.")
            }
        }
    });
}
fn change(mut state: Signal<ShellState>, model: Signal<State>, binding: &Binding, text: String) {
    {
        let mut shell = state.write();
        if !binding.current(&shell, &model.peek()) {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.inventory.as_ref().is_none_or(|i| i.drafts().len() > 1) {
            return;
        }
        if e.text == text {
            return;
        }
        e.text = text;
        e.blocks = None;
        e.retired_text = None;
        e.retired_blocks = None;
        e.serial = e.serial.wrapping_add(1);
        e.capture = uuid::Uuid::new_v4().to_string();
        e.reviewed = false;
        e.error = None;
    }
    save(state, binding.id.clone());
}
fn save(mut state: Signal<ShellState>, id: String) {
    {
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &id) else {
            return;
        };
        if e.saving
            || e.loading
            || e.inventory.is_none()
            || e.blocks.as_ref().is_some_and(|b| {
                super_platinum_core::canvas::comments::content::fallback(b).is_err()
            })
            || retired(e)
        {
            return;
        }
        e.saving = true;
    }
    dioxus::core::spawn_forever(async move {
        loop {
            let request = {
                let shell = state.peek();
                let Some(e) = entry(&shell, &id) else { return };
                let inventory = e.inventory.as_ref().unwrap();
                if inventory.drafts().iter().any(|d| {
                    d.capture() == e.capture
                        && d.text() == e.text
                        && d.blocks() == e.blocks.as_deref()
                }) {
                    break;
                }
                // A newer input arriving while a stale-write conflict commits
                // must itself become a visible version, never a hidden local tail.
                let prior = if inventory.drafts().len() == 1 {
                    inventory.drafts().first().cloned()
                } else {
                    None
                };
                if prior.as_ref().is_some_and(|d| {
                    d.text() == e.text
                        && d.capture() == e.capture
                        && d.blocks() == e.blocks.as_deref()
                }) {
                    break;
                }
                (
                    e.disk.clone(),
                    e.root.clone(),
                    prior,
                    e.capture.clone(),
                    e.text.clone(),
                    e.serial,
                    e.blocks.clone(),
                )
            };
            let scope = request.0.scope.clone();
            let root = request.1.clone();
            let prior = request.2;
            let capture = request.3;
            let text = request.4;
            let blocks = request.6;
            let result = request
                .0
                .work(move |j| {
                    let saved = match blocks {
                        Some(blocks) => j.save_rich_reply_draft(
                            &scope,
                            &root,
                            prior.as_ref(),
                            &capture,
                            blocks,
                        )?,
                        None => {
                            j.save_reply_draft(&scope, &root, prior.as_ref(), &capture, &text)?
                        }
                    };
                    Ok((saved, j.reply_inventory(&scope, &root)?))
                })
                .await;
            let mut shell = state.write();
            let Some(e) = entry_mut(&mut shell, &id) else {
                return;
            };
            match result {
                Ok((ReplySave::Stored(_), inventory)) => {
                    adopt(e, inventory);
                    e.error = None;
                    if e.serial == request.5 {
                        break;
                    }
                }
                Ok((ReplySave::Retired, inventory)) => {
                    adopt(e, inventory);
                    e.error = Some(
                        "This captured version was already reviewed. Your current text is retained; edit it or review saved versions.",
                    );
                    break;
                }
                Err(()) => {
                    e.error = Some(
                        "The reply is retained here but could not be saved locally. Retry local save before leaving the app.",
                    );
                    break;
                }
            }
        }
        if let Some(e) = entry_mut(&mut state.write(), &id) {
            e.saving = false;
        }
    });
}
fn retry(mut state: Signal<ShellState>, binding: Binding) {
    if native::retry(state, binding.clone()) {
        return;
    }
    if let Some(e) = entry_mut(&mut state.write(), &binding.id) {
        e.error = None;
    }
    if entry(&state.peek(), &binding.id).is_some_and(|e| e.inventory.is_none()) {
        load(state, binding)
    } else {
        save(state, binding.id)
    }
}
fn choose_version(
    mut state: Signal<ShellState>,
    model: Signal<State>,
    binding: Binding,
    draft: ReplyDraft,
) {
    let request = {
        let mut shell = state.write();
        if !binding.current(&shell, &model.peek()) {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.saving || e.sending || e.loading || e.native.is_some() {
            return;
        }
        let Some(inventory) = e.inventory.clone() else {
            return;
        };
        e.loading = true;
        (e.disk.clone(), inventory, e.serial)
    };
    dioxus::core::spawn_forever(async move {
        let scope = request.0.scope.clone();
        let root = binding.root.clone();
        let result = request
            .0
            .work(move |j| {
                let chosen = j.choose_reply_version(&request.1, &draft)?;
                Ok((chosen, j.reply_inventory(&scope, &root)?))
            })
            .await;
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        e.loading = false;
        match result {
            Ok((Some(draft), inventory)) if e.serial == request.2 => {
                e.text = draft.text().into();
                e.blocks = draft.blocks().map(<[_]>::to_vec);
                e.capture = draft.capture().into();
                adopt(e, inventory);
                e.serial = e.serial.wrapping_add(1);
                e.error = None
            }
            _ => e.error = Some("Saved versions changed. Reopen the thread and review them again."),
        }
    });
}
pub(in crate::canvas) fn pending_for_current_file(shell: &ShellState) -> bool {
    shell.canvas.as_ref().is_some_and(|file| {
        shell
            .canvas_checklist
            .comment_replies
            .entries
            .iter()
            .any(|e| {
                e.disk.account == shell.core.active_account.as_deref().unwrap_or("")
                    && e.disk.scope.workspace == shell.core.active_team.as_deref().unwrap_or("")
                    && e.disk.scope.user == shell.self_account.user_id
                    && e.disk.scope.file == file.id
                    && (e.native.is_some()
                        || e.saving
                        || e.native_busy
                        || e.sending
                        || e.locked.is_some()
                        || e.inventory.as_ref().is_some_and(|i| i.attempt().is_some()))
            })
    })
}

fn history(mut state: Signal<ShellState>, model: Signal<State>, binding: Binding, more: bool) {
    let request = {
        let mut shell = state.write();
        if !binding.current(&shell, &model.peek()) {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.loading {
            return;
        }
        e.loading = true;
        (
            e.disk.clone(),
            if more {
                e.history
                    .as_ref()
                    .and_then(|h| h.last())
                    .map(|v| v.receipt.id().to_string())
            } else {
                None
            },
        )
    };
    dioxus::core::spawn_forever(async move {
        let scope = request.0.scope.clone();
        let result = request
            .0
            .work(move |j| j.reply_history(&scope, request.1.as_deref()))
            .await;
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        e.loading = false;
        match result {
            Ok(rows) => e.history = Some(rows),
            Err(()) => {
                e.error = Some("Earlier replies could not be read. They have not been removed.")
            }
        }
    });
}

fn retired(e: &Entry) -> bool {
    e.retired_text.as_ref() == Some(&e.text) && e.retired_blocks == e.blocks
}
