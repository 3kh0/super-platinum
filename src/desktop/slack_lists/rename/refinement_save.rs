//! Shared Save of a captured local refinement set, through the retained worker.
use super::super::refinement;
use super::*;
use super_platinum_core::slack_lists::{
    BoardLaneOrderDraft, ViewCreate, ViewRefinementDestination, ViewRefinementDraft,
    ViewRefinementSave, ViewRename,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Action {
    context: refinement::Action,
    draft: ViewRefinementDraft,
    provider: Option<BoardLaneOrderDraft>,
    pub new: bool,
}
impl Action {
    pub(crate) fn view_id(&self) -> &str {
        self.context.view_id()
    }
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.context.current_retained(shell)
    }
    pub(super) fn supports(&self, doc: &Document) -> bool {
        self.context.matches_document(doc)
            && self.draft.changes(doc).is_ok_and(|c| c.any())
            && if self.new {
                ViewCreate::supports(doc)
            } else {
                doc.views
                    .iter()
                    .find(|v| v["id"] == self.view_id())
                    .is_some_and(ViewRename::supports)
            }
    }
    pub(super) fn intent(
        &self,
        doc: &Document,
        name: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<ViewRefinementSave, super_platinum_core::slack_lists::ListError> {
        if !self.supports(doc) {
            return Err(super_platinum_core::slack_lists::ListError(
                "The refinement review changed.",
            ));
        }
        ViewRefinementSave::capture(
            doc,
            &self.draft,
            self.provider.as_ref(),
            if self.new {
                ViewRefinementDestination::New { name: name.into() }
            } else {
                ViewRefinementDestination::Existing
            },
            allow,
            personal,
        )
    }
}
pub(crate) fn capture(shell: &ShellState, new: bool) -> Option<Action> {
    let context = refinement::capture(shell)?;
    let view = shell.slack_lists.view.as_ref()?;
    let loaded = view.loaded.as_ref()?;
    if loaded
        .document
        .read_only_reason(loaded.allow_lists.as_deref())
        .is_some()
        || loaded
            .personal_task_lists
            .as_ref()
            .is_none_or(|p| p.contains(&loaded.document.id))
    {
        return None;
    }
    let action = Action {
        draft: context.draft(shell)?,
        context,
        provider: view.layout.board_order.clone(),
        new,
    };
    if !action.supports(&loaded.document) {
        return None;
    }
    Some(action)
}

pub(super) mod network;
pub(super) mod readback;
mod receipt;
pub(super) use receipt::{Custody, Preference, Receipt};
#[derive(Clone)]
pub(crate) struct Presentation {
    pub new: bool,
    pub dismissed_on_ack: bool,
    pub preference: &'static str,
}
pub(super) fn presentation(entry: &registry::Entry) -> Option<Presentation> {
    let Target::Refinement(action) = &entry.target else {
        return None;
    };
    Some(Presentation {
        new: action.new,
        dismissed_on_ack: entry
            .refinement_receipt
            .as_ref()
            .is_some_and(|r| r.dismissed_on_ack),
        preference: entry
            .refinement_receipt
            .as_ref()
            .map(|r| r.preference.label())
            .unwrap_or("Not started"),
    })
}
pub(super) fn current(shell: &ShellState, target: &Target) -> bool {
    match target {
        Target::Refinement(action) => action.current(shell),
        _ => true,
    }
}
pub(crate) fn open(mut state: Signal<ShellState>, action: Action, visit: u64, selection: u64) {
    let serial = open_shell(&mut state.write(), action.clone(), visit, selection);
    if !action.new {
        if let Some(serial) = serial {
            super::confirm(state, serial);
        }
    }
}
pub(in crate::slack_lists) fn open_shell(
    shell: &mut ShellState,
    action: Action,
    visit: u64,
    selection: u64,
) -> Option<u64> {
    if super::selection(shell) != Some(selection) || !action.current(shell) {
        return None;
    }
    if !open_target(shell, Target::Refinement(action), visit) {
        return None;
    }
    entry(shell).map(|e| e.serial)
}
