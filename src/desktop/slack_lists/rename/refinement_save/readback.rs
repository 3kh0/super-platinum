//! A metadata Check can transfer child custody only from its exact prior state.
use super::*;
pub(in super::super) fn observes(entry: &registry::Entry, doc: &Document) -> bool {
    let Some(Intent::Refinement(intent)) = entry.intent.as_ref() else {
        return false;
    };
    let Some(id) = intent.observed_view_id(doc) else {
        return false;
    };
    match intent.destination() {
        ViewRefinementDestination::Existing => true,
        ViewRefinementDestination::New { .. } => {
            entry
                .refinement_receipt
                .as_ref()
                .and_then(|r| r.acknowledged_id.as_deref())
                == Some(id)
        }
    }
}
pub(in super::super) fn transfer_allowed(shell: &ShellState, entry: &registry::Entry) -> bool {
    entry
        .refinement_receipt
        .as_ref()
        .and_then(|r| r.custody.as_ref())
        .is_some_and(|c| c.current(shell))
}
pub(in super::super) fn release_pending(shell: &mut ShellState, entry: &registry::Entry) {
    if !entry.target.is_refinement() {
        return;
    }
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return;
    };
    if view.layout.pending_refinement.as_ref().is_some_and(|id| {
        view.loaded
            .as_ref()
            .is_some_and(|l| l.document.views.iter().any(|v| v["id"] == *id))
    }) {
        view.layout.pending_refinement = None;
    }
}
pub(in super::super) fn transfer(shell: &ShellState, key: &Key, serial: u64, allowed: bool) {
    if !allowed {
        return;
    }
    let custody = Custody::capture(shell);
    with_registry(|r| {
        if let Some(e) = r.entries.get_mut(key).filter(|e| e.serial == serial) {
            if let Some(receipt) = e.refinement_receipt.as_mut() {
                receipt.custody = custody;
            }
        }
    });
}
