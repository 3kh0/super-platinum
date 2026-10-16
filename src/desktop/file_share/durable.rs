//! Explicit durable note lifecycle. Saved bodies never confer Slack authority.
use super::*;
use draft_io::{Command, Outcome, Queue, Ticket};
use recovery::{Custody, Storage};
use std::sync::Mutex;
use super_platinum_core::file_sharing::store::drafts::{Content, Draft};
pub(super) mod capture;
pub(super) mod close;
pub(super) mod open;
pub(super) mod saved;

const LOAD_ERROR: &str =
    "The saved message could not be loaded. Try again or review saved file shares.";
const SAVE_ERROR: &str = "The message could not be saved. Keep this window open and try again.";
const CONFLICT: &str =
    "This saved message changed elsewhere. Your captured message is retained for review.";
const REVIEW: &str = "This message is saved on this computer. Review it before discarding it.";

fn destination(
    value: Option<&super_platinum_core::file_sharing::store::drafts::Destination>,
) -> Option<Destination> {
    value.map(|d| Destination {
        id: d.id.clone(),
        name: d.name.clone(),
        private: d.private,
    })
}
fn content(expected: &Draft, note: Vec<Value>, destination: Option<Destination>) -> Content {
    Content {
        source: expected.content.source.clone(),
        title: expected.content.title.clone(),
        note,
        destination: destination.map(|d| {
            super_platinum_core::file_sharing::store::drafts::Destination {
                id: d.id,
                name: d.name,
                private: d.private,
            }
        }),
    }
}
pub(super) fn captured(ticket: &Ticket) -> Option<Content> {
    match ticket.input()? {
        Command::Save { content, .. }
        | Command::Clear { content, .. }
        | Command::Retain { content, .. } => Some(content),
        Command::Discard { expected } => Some(expected.content),
        _ => None,
    }
}
pub(super) fn unsaved(ticket: &Ticket) -> bool {
    matches!(
        ticket.input_ref(),
        Some(
            Command::Save { .. }
                | Command::Clear { .. }
                | Command::Retain { .. }
                | Command::Discard { .. }
        )
    ) && matches!(
        ticket.outcome_ref(),
        Some(Outcome::Failed(_) | Outcome::Conflict(_) | Outcome::Blocked(_))
    )
}
pub(crate) fn has_capture(shell: &ShellState, id: &str) -> bool {
    visible(shell).is_some_and(|d| {
        d.id == id
            && d.phase == Phase::Editing
            && !d.closing
            && d.draft_task.as_ref().is_some_and(unsaved)
    })
}
fn same(dialog: &Dialog, id: &str, visit: &Arc<drafts::Visit>) -> bool {
    dialog.id == id
        && dialog
            .draft_visit
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, visit))
}
/// Only load jobs can be canceled. Captured save jobs keep their own input and
/// actual completion across context/UI replacement.
pub(super) fn retire(dialog: &Dialog, custody: &Arc<Mutex<Custody>>) {
    if dialog.phase != Phase::Loading {
        return;
    }
    let Some(ticket) = dialog.draft_task.clone() else {
        return;
    };
    let Ok(held) = custody.lock() else { return };
    cancel_load(held.draft_io.clone(), ticket);
}
/// A published empty reservation remains owned until a current dialog takes
/// its exact version or Saved file shares presents it. Never allocate a second
/// queue slot merely to retire an unobserved result under capacity pressure.
pub(super) fn pending_claim(ticket: &Ticket) -> Option<&Draft> {
    match ticket.outcome_ref()? {
        Outcome::Claimed { draft, .. } if draft.content.empty() => Some(draft),
        Outcome::Canceled {
            claim,
            cleanup: draft_io::Cleanup::Failed(_) | draft_io::Cleanup::Blocked(_),
        } if claim.content.empty() => Some(claim),
        _ => None,
    }
}
fn cancel_load(queue: Queue, ticket: Ticket) {
    ticket.cancel_load();
    tokio::spawn(async move {
        ticket.wait().await;
        if pending_claim(&ticket).is_none() {
            let _ = queue.acknowledge(&ticket);
        }
    });
}

pub(super) fn apply_capture(
    shell: &mut ShellState,
    id: &str,
    visit: &Arc<drafts::Visit>,
    ticket: &Ticket,
    outcome: &Outcome,
) -> bool {
    let Some(dialog) = shell
        .file_share
        .dialog
        .as_mut()
        .filter(|d| same(d, id, visit))
    else {
        return false;
    };
    if !dialog.draft_task.as_ref().is_some_and(|t| t.same(ticket)) {
        return false;
    }
    match outcome {
        Outcome::Saved(draft) => {
            dialog.durable = Some(draft.clone());
            dialog.note = draft.content.note.clone();
            dialog.draft_task = None;
            dialog.error = None;
        }
        Outcome::Discarded => {
            dialog.durable = None;
            dialog.draft_task = None;
            dialog.note.clear();
            dialog.destination = None;
        }
        Outcome::Conflict(_) | Outcome::Blocked(_) => {
            // Keep the same editor/selection; its DOM is the captured input.
            dialog.phase = Phase::DraftReview;
            dialog.draft_local = true;
            dialog.saved_draft_stale = true;
            dialog.error = Some(CONFLICT);
        }
        _ => {
            dialog.phase = if dialog.durable.is_some() {
                Phase::Editing
            } else {
                Phase::DraftReview
            };
            dialog.draft_local = dialog.durable.is_none();
            dialog.saved_draft_stale = dialog.draft_local;
            dialog.error = Some(SAVE_ERROR);
        }
    }
    true
}

pub(super) async fn hide(mut signal: Signal<ShellState>, id: &str, focus: String, epoch: u64) {
    let focus = serde_json::to_string(&focus).unwrap();
    let owner = serde_json::to_string(&epoch.to_string()).unwrap();
    let generation = serde_json::to_string(id).unwrap();
    let script = include_str!("return_focus.js");
    let mut eval = document::eval(&format!(
        "await(async()=>{{const cancel={script}({focus},{owner},{generation});dioxus.send(!!cancel);const disposition=await dioxus.recv();if(disposition!=='closed')cancel?.();}})();"
    ));
    let armed = tokio::time::timeout(std::time::Duration::from_secs(1), eval.recv::<bool>())
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(false);
    let closed = {
        let mut shell = signal.write();
        if visible(&shell).is_some_and(|d| d.id == id && d.closing)
            && shell.core.account_epoch == epoch
        {
            super::close(&mut shell, id);
            true
        } else {
            false
        }
    };
    let _ = eval.send(if armed && closed { "closed" } else { "cancel" });
}
