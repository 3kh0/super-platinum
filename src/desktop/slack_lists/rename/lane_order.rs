//! Shared Board lane moves through the existing single-use schema worker.
//! The action owns the projected neighbors, not a later DOM index or label.
use super::*;
use super_platinum_core::slack_lists::{BoardLaneOrder, BoardLaneOrderDraft, BoardLanePlacement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Left,
    Right,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Action {
    pub view: String,
    pub source: String,
    pub target: String,
    pub placement: BoardLanePlacement,
    pub draft: Option<BoardLaneOrderDraft>,
    pub(super) refinements: super_platinum_core::slack_lists::ViewRefinementDraft,
    context: super::super::refinement::Action,
    shared: bool,
    visit: u64,
    selection: u64,
    layout: u64,
    grouping: u64,
    find: u64,
    input: String,
    applied: String,
    visible: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReadProjection {
    refinements: super::super::refinement::State,
    pending: Option<String>,
    draft: Option<BoardLaneOrderDraft>,
    layout: u64,
    grouping: u64,
    find: u64,
    input: String,
    applied: String,
    visible: Vec<String>,
}
impl ReadProjection {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let v = shell.slack_lists.view.as_ref()?;
        Some(Self {
            refinements: v.layout.refinements.clone(),
            pending: v.layout.pending_refinement.clone(),
            draft: v.layout.board_order.clone(),
            layout: v.layout.generation,
            grouping: v.layout.grouping_revision,
            find: v.layout.find.revision,
            input: v.layout.find.input.clone(),
            applied: v.layout.find.applied.clone(),
            visible: v.layout.lanes.iter().map(|l| l.key.clone()).collect(),
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell).as_ref() == Some(self)
    }
}
impl Action {
    fn current(&self, shell: &ShellState) -> bool {
        self.context.current_retained(shell)
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.layout.board_order == self.draft
                    && v.generation == self.visit
                    && v.layout.selection_generation == self.selection
                    && v.layout.selected.as_deref() == Some(&self.view)
                    && v.layout.generation == self.layout
                    && v.layout.grouping_revision == self.grouping
                    && v.layout.find.revision == self.find
                    && v.layout.find.input == self.input
                    && v.layout.find.applied == self.applied
                    && v.layout
                        .lanes
                        .iter()
                        .map(|l| &l.key)
                        .eq(self.visible.iter())
            })
    }
}
pub(super) fn projection_current(shell: &ShellState, target: &Target) -> bool {
    match target {
        Target::LaneOrder(a) => a.current(shell),
        Target::BoardChoice(a) => a.current(shell),
        Target::CardOrder(a) => a.current(shell),
        Target::GroupDelete(a) => a.current(shell),
        _ => true,
    }
}
/// Resolve against the entire currently projected lane sequence; paginated DOM
/// positions are never used as saved-order indices. Hidden empty lanes remain in
/// the full core order and can never be fabricated as a visible destination.
pub(crate) fn adjacent(shell: &ShellState, lane: &str, direction: Direction) -> Option<Action> {
    let v = shell.slack_lists.view.as_ref()?;
    let visible: Vec<_> = v.layout.lanes.iter().map(|l| l.key.clone()).collect();
    let at = visible.iter().position(|id| id == lane)?;
    let other = match direction {
        Direction::Left => at.checked_sub(1)?,
        Direction::Right => at.checked_add(1).filter(|i| *i < visible.len())?,
    };
    destination(
        shell,
        lane,
        &visible[other],
        match direction {
            Direction::Left => BoardLanePlacement::Before,
            Direction::Right => BoardLanePlacement::After,
        },
    )
}
pub(crate) fn destination(
    shell: &ShellState,
    source: &str,
    target: &str,
    placement: BoardLanePlacement,
) -> Option<Action> {
    let v = shell.slack_lists.view.as_ref()?;
    let view = v.layout.selected.clone()?;
    let loaded = v.loaded.as_ref()?;
    let visible: Vec<_> = v.layout.lanes.iter().map(|l| l.key.clone()).collect();
    let context = super::super::refinement::capture(shell)?;
    let refinements = context.draft(shell)?;
    let full = BoardLaneOrder::refined_order(
        &loaded.document,
        &refinements,
        v.layout.board_order.as_ref(),
    )
    .ok()?;
    let shared = BoardLaneOrder::refined_uses_shared(&loaded.document, &refinements).ok()?;
    if source == target
        || !visible.iter().any(|k| k == source)
        || !visible.iter().any(|k| k == target)
        || full
            .iter()
            .filter(|k| visible.contains(k))
            .ne(visible.iter())
        || v.layout.find.input != v.layout.find.applied
    {
        return None;
    }
    let action = Action {
        refinements,
        context,
        view,
        source: source.into(),
        target: target.into(),
        placement,
        shared,
        draft: v.layout.board_order.clone(),
        visit: v.generation,
        selection: v.layout.selection_generation,
        layout: v.layout.generation,
        grouping: v.layout.grouping_revision,
        find: v.layout.find.revision,
        input: v.layout.find.input.clone(),
        applied: v.layout.find.applied.clone(),
        visible,
    };
    if shared {
        if !can_target(shell, &Target::LaneOrder(action.clone())) {
            return None;
        }
        BoardLaneOrder::capture_refined(
            &loaded.document,
            &action.refinements,
            action.draft.as_ref(),
            source,
            target,
            placement,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .ok()?;
    } else {
        if !owner::available(shell) || blocks_mutation(shell) {
            return None;
        }
        BoardLaneOrderDraft::moved_refined(
            &loaded.document,
            &action.refinements,
            action.draft.as_ref(),
            source,
            target,
            placement,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .ok()?;
    }
    Some(action)
}
pub(crate) fn drop_action(shell: &ShellState, captured: &Action, target: &str) -> Option<Action> {
    if !captured.current(shell) {
        return None;
    }
    let from = captured
        .visible
        .iter()
        .position(|k| k == &captured.source)?;
    let to = captured.visible.iter().position(|k| k == target)?;
    destination(
        shell,
        &captured.source,
        target,
        if from < to {
            BoardLanePlacement::After
        } else {
            BoardLanePlacement::Before
        },
    )
}
pub(crate) fn open_shell(shell: &mut ShellState, action: Action) -> bool {
    action.shared
        && action.current(shell)
        && open_target(shell, Target::LaneOrder(action.clone()), action.visit)
}
/// A deliberate lane control/drop already specifies the complete move. It starts
/// the existing preflight immediately; the retained modal owns uncertain results.
pub(crate) fn apply(mut state: Signal<ShellState>, action: Action) {
    if !action.shared {
        apply_local(&mut state.write(), action);
        return;
    }
    let prepared = {
        let shell = &mut state.write();
        if !open_shell(shell, action) {
            return;
        }
        let Some(e) = entry(shell) else {
            return;
        };
        prepare(shell, e.serial)
    };
    if let Some((guard, intent)) = prepared {
        network::save(state, guard, intent);
    }
}
pub(crate) fn has_local_order(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .view
        .as_ref()
        .is_some_and(|v| v.layout.board_order.is_some())
}
fn apply_local(shell: &mut ShellState, action: Action) -> bool {
    if action.shared || !action.current(shell) || !owner::available(shell) || blocks_mutation(shell)
    {
        return false;
    }
    let Some(l) = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
    else {
        return false;
    };
    let Ok(draft) = BoardLaneOrderDraft::moved_refined(
        &l.document,
        &action.refinements,
        action.draft.as_ref(),
        &action.source,
        &action.target,
        action.placement,
        l.allow_lists.as_deref(),
        l.personal_task_lists.as_ref(),
    ) else {
        return false;
    };
    let v = shell.slack_lists.view.as_mut().unwrap();
    v.layout.board_order = Some(draft);
    v.layout.generation = next_id();
    v.layout.grouping_revision = next_id();
    rebuild_rows(shell);
    true
}
fn names(doc: &Document, id: &str, keys: &[String]) -> Option<Vec<(String, String)>> {
    let view = doc.views.iter().find(|v| v["id"] == id)?;
    let key = view.pointer("/grouping/group_by")?.as_str()?;
    let col = doc.columns.iter().find(|c| c.key == key)?;
    keys.iter()
        .map(|key| {
            let label = if col.kind == "checkbox" {
                if key.is_empty() {
                    "Unchecked".into()
                } else {
                    "Checked".into()
                }
            } else if key.is_empty() {
                "Ungrouped".into()
            } else {
                col.raw
                    .pointer("/options/choices")?
                    .as_array()?
                    .iter()
                    .find(|v| v["value"] == key.as_str())?["label"]
                    .as_str()?
                    .to_owned()
            };
            Some((key.clone(), label))
        })
        .collect()
}
pub(super) fn presentation(
    shell: &ShellState,
    e: &registry::Entry,
) -> Option<view_order::Presentation> {
    let Target::LaneOrder(a) = &e.target else {
        return None;
    };
    let before =
        BoardLaneOrder::refined_order(&e.original, &a.refinements, a.draft.as_ref()).ok()?;
    let displayed =
        BoardLaneOrder::refined_projection_document(&e.original, &a.refinements, a.draft.as_ref())
            .ok()?;
    // Presentation cannot create a dispatch permit. Known policy was captured in
    // the original entry; this reconstructs only the immutable proposed sequence.
    let mut after = before.clone();
    let at = after.iter().position(|id| id == &a.source)?;
    let source = after.remove(at);
    let target = after.iter().position(|id| id == &a.target)?;
    after.insert(
        target + usize::from(a.placement == BoardLanePlacement::After),
        source,
    );
    let label = names(&displayed, &a.view, std::slice::from_ref(&a.source))?
        .remove(0)
        .1;
    let current = shell
        .slack_lists
        .view
        .as_ref()?
        .loaded
        .as_ref()
        .and_then(|l| {
            let local = shell
                .slack_lists
                .view
                .as_ref()?
                .layout
                .refinements
                .draft(&l.document, &a.view)
                .ok()?;
            let displayed = BoardLaneOrder::refined_projection_document(
                &l.document,
                &local,
                shell.slack_lists.view.as_ref()?.layout.board_order.as_ref(),
            )
            .ok()?;
            names(
                &displayed,
                &a.view,
                &BoardLaneOrder::refined_order(
                    &l.document,
                    &local,
                    shell.slack_lists.view.as_ref()?.layout.board_order.as_ref(),
                )
                .ok()?,
            )
        });
    Some(view_order::Presentation {
        lanes: true,
        direction: if a.placement == BoardLanePlacement::Before {
            "left"
        } else {
            "right"
        },
        name: label,
        before: names(&displayed, &a.view, &before)?,
        after: names(&displayed, &a.view, &after)?,
        current,
    })
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod local_tests;
