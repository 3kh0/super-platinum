//! Typed local choices share the existing control models, never a write permit.
use super::*;
use super_platinum_core::slack_lists::{
    ViewFilterChoice, ViewGroupingChoice, ViewLayout, ViewSortChoice,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Filter(ViewFilterChoice),
    Sort(ViewSortChoice),
    Visibility { column: String, visible: bool },
    Grouping(ViewGroupingChoice),
    Layout(ViewLayout),
    Completed(bool),
}
/// Complete immutable control value. Number/date inputs must arrive through
/// their existing owned DOM capture; this function never reads a later DOM node.
pub(crate) fn draft(
    shell: &ShellState,
    action: &Action,
    choice: &Choice,
) -> Option<ViewRefinementDraft> {
    if !action.current(shell) {
        return None;
    }
    draft_retained(shell, action, choice)
}
pub(in crate::slack_lists) fn draft_retained(
    shell: &ShellState,
    action: &Action,
    choice: &Choice,
) -> Option<ViewRefinementDraft> {
    let draft = action.draft_retained(shell)?;
    let doc = &shell.slack_lists.view.as_ref()?.loaded.as_ref()?.document;
    match choice {
        Choice::Filter(value) => draft.with_filter_choice(doc, value),
        Choice::Sort(value) => draft.with_sort_choice(doc, value),
        Choice::Visibility { column, visible } => draft.with_visibility(doc, column, *visible),
        Choice::Grouping(value) => draft.with_grouping(
            doc,
            match value {
                ViewGroupingChoice::None => None,
                ViewGroupingChoice::Column { column_id } => Some(column_id),
            },
        ),
        Choice::Layout(value) => draft.with_layout(doc, *value),
        Choice::Completed(value) => draft.with_completed(doc, *value),
    }
    .ok()
}
pub(crate) fn apply_choice(shell: &mut ShellState, action: &Action, choice: Choice) -> bool {
    let Some(draft) = draft(shell, action, &choice) else {
        return false;
    };
    apply(shell, action, draft)
}
