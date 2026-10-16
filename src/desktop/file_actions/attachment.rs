//! A rendered file must still belong to this exact canonical message.
//! This extends existing file actions; it does not provide a sharing endpoint.
use super::*;
use crate::message_buttons::source::Source;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use super_platinum_core::file_actions::FileIdentity;

pub(crate) mod check;
#[cfg(test)]
pub(crate) mod tests;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Guard {
    source: Source,
    file: FileIdentity,
    title: String,
}
impl Guard {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.canonical_file(shell).is_some()
    }

    /// Full current canonical metadata for a separate admission policy. This
    /// preserves the existing exact message/bag/file guard; it is not Share
    /// permission and must never gate custody after a write was dispatched.
    pub(crate) fn canonical_file<'a>(&self, shell: &'a ShellState) -> Option<&'a File> {
        let message = self.source.message(shell).ok()?;
        let workspace = shell.core.workspaces.get(&self.source.owner().team_id)?;
        if workspace.deleted_files.contains(&self.file.id) {
            return None;
        }
        // Block-action Source revisions intentionally omit files. Check their
        // independent identity/permission/title projection on every admission.
        let mut files = message
            .files
            .iter()
            .filter(|file| file.id.as_deref() == Some(&self.file.id));
        let file = files.next()?;
        (files.next().is_none()
            && policy::identity(file).as_ref() == Some(&self.file)
            && file_title(file) == self.title)
            .then_some(file)
    }
}
pub(crate) fn target(
    shell: &ShellState,
    source: &Source,
    file: &FileIdentity,
    title: &str,
) -> Option<Target> {
    let guard = Guard {
        source: source.clone(),
        file: file.clone(),
        title: title.into(),
    };
    if !guard.current(shell) {
        return None;
    }
    let workspace = shell.core.workspaces.get(&source.owner().team_id)?;
    Some(Target {
        id: file.id.clone(),
        title: title.into(),
        link: file.permalink.clone(),
        owned: !workspace.self_user_id.is_empty()
            && file.uploader.as_deref() == Some(&workspace.self_user_id),
        renameable: file.renameable,
        attachment: Some(guard),
        epoch: shell.core.account_epoch,
        team: source.owner().team_id.clone(),
    })
}

// A title refresh retires action authority but keeps the focus address for this
// same message/file occurrence. Every other Target/Source change retires both.
fn same_focus_origin(left: &Target, right: &Target) -> bool {
    left.id == right.id
        && left.link == right.link
        && left.owned == right.owned
        && left.renameable == right.renameable
        && left.epoch == right.epoch
        && left.team == right.team
        && matches!((&left.attachment, &right.attachment), (Some(a), Some(b))
            if a.source == b.source && a.file == b.file)
}

// DOM identity only, never authority. Same file may occur in two messages or
// simultaneously in the timeline, thread and viewer. Refuse allocation at wrap.
static NEXT: AtomicU64 = AtomicU64::new(1);
fn next_id() -> Option<u64> {
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    })
    .ok()
}
#[component]
pub(crate) fn MessageFile(state: Signal<ShellState>, target: Target, children: Element) -> Element {
    let id = use_hook(next_id);
    let held = use_hook(|| Rc::new(RefCell::new((target.clone(), Some(0_u64), Some(0_u64)))));
    let generations = {
        let mut held = held.borrow_mut();
        if held.0 != target {
            if !same_focus_origin(&held.0, &target) {
                held.2 = held.2.and_then(|n| n.checked_add(1));
            }
            held.0 = target.clone();
            held.1 = held.1.and_then(|n| n.checked_add(1));
        }
        held.1.zip(held.2)
    };
    let Some((id, (generation, focus_generation))) = id.zip(generations) else {
        return children;
    };
    let instance = format!("attachment-{id}-{generation}");
    let share_return = format!("file-share-return:attachment-{id}-{focus_generation}");
    let file_id = target.id.clone();
    rsx! { div {class:"message-file","data-file-id":file_id,
        {children}
        // Keys are reconciled on fragment children, not a static component slot.
        // A new target must drop the old action signals and outstanding UI handles.
        for instance in [instance] {
            super::Actions {key:"{instance}",state,target:target.clone(),instance,share_return:share_return.clone()}
        }
    } }
}
