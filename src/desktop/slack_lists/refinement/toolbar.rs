//! Small rendered clocks; complete drafts are captured only on explicit action.
use super::*;
use super_platinum_core::slack_lists::{ViewLayout, ViewRename};
#[derive(Clone)]
pub(crate) struct Stamp {
    identity: std::sync::Arc<()>,
    epoch: u64,
    pub visit: u64,
    selection: u64,
    layout: u64,
}
impl PartialEq for Stamp {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.identity, &other.identity)
            && (self.epoch, self.visit, self.selection, self.layout)
                == (other.epoch, other.visit, other.selection, other.layout)
    }
}
impl Eq for Stamp {}
impl Stamp {
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        stamp(shell).as_ref() == Some(self)
    }
}
pub(crate) fn stamp(shell: &ShellState) -> Option<Stamp> {
    let v = shell.slack_lists.view.as_ref()?;
    Some(Stamp {
        identity: shell.slack_lists.scope.as_ref()?.identity.clone(),
        epoch: shell.core.account_epoch,
        visit: v.generation,
        selection: v.layout.selection_generation,
        layout: v.layout.generation,
    })
}
pub(crate) struct Presentation {
    pub stamp: Stamp,
    pub enabled: bool,
    pub layout: &'static str,
    pub show_completed: bool,
    pub completion: bool,
    pub save_visible: bool,
    pub save_existing: bool,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let v = shell.slack_lists.view.as_ref()?;
    let loaded = v.loaded.as_ref()?;
    let id = v.layout.selected.as_deref()?;
    let doc = document(v).ok()?;
    let raw = doc.views.iter().find(|v| v["id"] == id)?;
    let layout = match raw["type"].as_str()? {
        "table" => "table",
        "board" => "board",
        _ => return None,
    };
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|s| shell.core.workspaces.get(&s.team));
    let context = super::super::layout::projection_context(v, loaded, workspace, raw).ok()?;
    let save_visible = changes(shell)?.any()
        && loaded
            .document
            .read_only_reason(loaded.allow_lists.as_deref())
            .is_none();
    Some(Presentation {
        stamp: stamp(shell)?,
        enabled: available(shell)
            && !rename::blocks_mutation(shell)
            && v.layout.pending_refinement.is_none(),
        layout,
        show_completed: context.effective_show_completed.unwrap_or(false),
        completion: context.apply_todo_completion_rules == Some(true),
        save_visible,
        save_existing: loaded
            .document
            .views
            .iter()
            .find(|v| v["id"] == id)
            .is_some_and(ViewRename::supports),
    })
}
pub(crate) fn layout(mut state: Signal<ShellState>, stamp: Stamp, value: String) {
    let value = match value.as_str() {
        "table" => ViewLayout::Table,
        "board" => ViewLayout::Board,
        _ => return,
    };
    choose(&mut state.write(), stamp, controls::Choice::Layout(value));
}
pub(crate) fn completed(mut state: Signal<ShellState>, stamp: Stamp, value: bool) {
    choose(
        &mut state.write(),
        stamp,
        controls::Choice::Completed(value),
    );
}
pub(super) fn choose(shell: &mut ShellState, stamp: Stamp, choice: controls::Choice) -> bool {
    if !stamp.current(shell) {
        return false;
    }
    let Some(action) = capture(shell) else {
        return false;
    };
    controls::apply_choice(shell, &action, choice)
}
pub(crate) fn reset_all(mut state: Signal<ShellState>, stamp: Stamp) {
    let mut shell = state.write();
    if !stamp.current(&shell) {
        return;
    }
    if let Some(action) = capture(&shell) {
        reset(&mut shell, &action);
    }
}
pub(crate) fn save(state: Signal<ShellState>, stamp: Stamp, new: bool) {
    let action = {
        let shell = state.peek();
        if !stamp.current(&shell) {
            return;
        }
        rename::refinement_save::capture(&shell, new)
    };
    if let Some(action) = action {
        rename::refinement_save::open(state, action, stamp.visit, stamp.selection);
    }
}
