//! Deliberate group deletion retains separate schema/cell outcomes.
use super::*;
use super_platinum_core::slack_lists::saved_views;
use super_platinum_core::slack_lists::{BoardGroupDelete, BoardGroupDeletePhase};
pub(super) mod network;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Action {
    pub view: String,
    column: String,
    pub value: String,
    pub matching: usize,
    identity: card_order::ContextIdentity,
    epoch: u64,
    visit: u64,
    selection: u64,
    projection: lane_order::ReadProjection,
    canonical: saved_views::Projection,
    document: Value,
    records: Value,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Outcomes {
    pub schema: BoardGroupDeletePhase,
    pub cells: BoardGroupDeletePhase,
}
#[derive(Clone)]
pub(crate) struct Presentation {
    pub label: String,
    pub value: String,
    pub matching: usize,
    pub schema: Option<&'static str>,
    pub cells: Option<&'static str>,
}
fn displayed(shell: &ShellState, canonical: &saved_views::Projection) -> bool {
    shell.slack_lists.view.as_ref().is_some_and(|v| {
        canonical.groups.len() == v.layout.lanes.len()
            && v.layout.lanes.iter().all(|lane| {
                canonical
                    .groups
                    .iter()
                    .find(|g| g.key.as_deref() == Some(lane.key.as_str()))
                    .is_some_and(|g| {
                        g.rows
                            .iter()
                            .map(|r| (&r.record_id, &r.instance_id))
                            .eq(lane.rows.iter().map(|r| (&r.id, &r.instance_id)))
                    })
            })
    })
}
impl Action {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.epoch == shell.core.account_epoch
            && self.projection.current(shell)
            && shell.slack_lists.scope.as_ref().is_some_and(|s| {
                s.current(shell) && std::sync::Arc::ptr_eq(&s.identity, &self.identity.0)
            })
            && card_order::canonical(shell, &self.view).as_ref() == Some(&self.canonical)
            && displayed(shell, &self.canonical)
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && v.layout.selection_generation == self.selection
                    && v.layout.selected.as_deref() == Some(&self.view)
                    && v.loaded.as_ref().is_some_and(|l| {
                        l.document.raw == self.document && l.records.raw == self.records
                    })
            })
    }
    fn plan(&self, shell: &ShellState, loaded: &Loaded) -> Option<BoardGroupDelete> {
        if !self.current(shell) {
            return None;
        }
        let view = shell.slack_lists.view.as_ref()?;
        let local = view
            .layout
            .refinements
            .draft(&loaded.document, &self.view)
            .ok()?;
        let context = card_order::context(shell, loaded, &self.view)?;
        BoardGroupDelete::capture_refined(
            &loaded.document,
            &loaded.records,
            &local,
            &self.value,
            &context,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .ok()
    }
}
pub(super) fn name<'a>(doc: &'a Document, a: &Action) -> Option<&'a str> {
    doc.column(&a.column)
        .filter(|c| c.kind == "select")?
        .raw
        .pointer("/options/choices")?
        .as_array()?
        .iter()
        .find(|c| c["value"] == a.value)?["label"]
        .as_str()
}
pub(super) fn supports(doc: &Document, a: &Action) -> bool {
    doc.raw == a.document && name(doc, a).is_some()
}
pub(crate) fn available(shell: &ShellState, value: &str) -> Option<Action> {
    if !owner::available(shell) || blocks_mutation(shell) {
        return None;
    }
    let v = shell.slack_lists.view.as_ref()?;
    let loaded = v.loaded.as_ref()?;
    if v.layout.find.input != v.layout.find.applied || v.layout.notice.is_some() {
        return None;
    }
    if serde_json::to_vec(&loaded.document.raw).map_or(true, |b| b.len() > registry::MAX_DOCUMENT) {
        return None;
    }
    text_create::bounded_population(shell)?;
    let view = v.layout.selected.clone()?;
    if !v.layout.lanes.iter().any(|l| l.key == value) {
        return None;
    }
    let local = super::super::refinement::document(v).ok()?;
    let raw = local.views.iter().find(|v| v["id"] == view)?;
    let column = loaded
        .document
        .columns
        .iter()
        .find(|c| {
            Some(c.key.as_str()) == raw.pointer("/grouping/group_by").and_then(Value::as_str)
        })?
        .id
        .clone();
    let mut action = Action {
        column,
        view: view.clone(),
        value: value.into(),
        matching: 0,
        identity: card_order::ContextIdentity(shell.slack_lists.scope.as_ref()?.identity.clone()),
        epoch: shell.core.account_epoch,
        visit: v.generation,
        selection: v.layout.selection_generation,
        projection: lane_order::ReadProjection::capture(shell)?,
        canonical: card_order::canonical(shell, &view)?,
        document: loaded.document.raw.clone(),
        records: loaded.records.raw.clone(),
    };
    action.matching = action.plan(shell, loaded)?.cells().len();
    can_target(shell, &Target::GroupDelete(action.clone())).then_some(action)
}
pub(crate) fn open(mut state: Signal<ShellState>, action: Action) {
    open_shell(&mut state.write(), action);
}
pub(crate) fn open_shell(shell: &mut ShellState, a: Action) -> bool {
    if !a.current(shell) || available(shell, &a.value).as_ref() != Some(&a) {
        return false;
    }
    let visit = a.visit;
    open_target(shell, Target::GroupDelete(a), visit)
}
pub(super) fn capture(
    shell: &ShellState,
    e: &registry::Entry,
    loaded: &Loaded,
) -> Result<Intent, super_platinum_core::slack_lists::ListError> {
    let Target::GroupDelete(a) = &e.target else {
        return card_order::capture(shell, e, loaded);
    };
    if e.input != Input::GroupDelete {
        return Err(super_platinum_core::slack_lists::ListError(
            "The group deletion changed.",
        ));
    }
    a.plan(shell, loaded).map(Intent::GroupDelete).ok_or(
        super_platinum_core::slack_lists::ListError(
            "The group or loaded items changed. Nothing was sent.",
        ),
    )
}
fn phase(p: BoardGroupDeletePhase) -> &'static str {
    match p {
        BoardGroupDeletePhase::Reviewing => "Ready",
        BoardGroupDeletePhase::InFlight => "In progress",
        BoardGroupDeletePhase::NotRequested => "Not sent",
        BoardGroupDeletePhase::Unconfirmed => "Unconfirmed",
        BoardGroupDeletePhase::Observed => "Observed in current data",
        BoardGroupDeletePhase::NotRequired => "No matching items required clearing",
    }
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    let Target::GroupDelete(a) = &e.target else {
        return None;
    };
    let label = |p| {
        if p == BoardGroupDeletePhase::Observed
            && !e
                .review
                .as_ref()
                .is_some_and(|r| r.navigation_current(shell))
        {
            "Previously observed; check current data"
        } else {
            phase(p)
        }
    };
    Some(Presentation {
        label: e.before.clone(),
        value: a.value.clone(),
        matching: a.matching,
        schema: e.group_delete_outcomes.map(|o| label(o.schema)),
        cells: e.group_delete_outcomes.map(|o| label(o.cells)),
    })
}

pub(crate) fn confirm(state: Signal<ShellState>, serial: u64, visit: u64) {
    let shell = state.peek();
    if shell
        .slack_lists
        .view
        .as_ref()
        .is_none_or(|v| v.generation != visit)
        || entry(&shell).is_none_or(|e| e.serial != serial || !e.target.is_group_delete())
    {
        return;
    }
    drop(shell);
    super::confirm(state, serial);
}
pub(super) fn observe(
    mut outcomes: Outcomes,
    plan: &BoardGroupDelete,
    doc: &Document,
    page: &RecordsPage,
) -> Outcomes {
    for (phase, matches) in [
        (&mut outcomes.schema, plan.observes_schema(doc)),
        (&mut outcomes.cells, plan.observes_cells(doc, page)),
    ] {
        if matches!(
            *phase,
            BoardGroupDeletePhase::Unconfirmed | BoardGroupDeletePhase::Observed
        ) {
            *phase = if matches {
                BoardGroupDeletePhase::Observed
            } else {
                BoardGroupDeletePhase::Unconfirmed
            };
        }
    }
    outcomes
}
pub(super) fn observed(o: Outcomes) -> bool {
    o.schema == BoardGroupDeletePhase::Observed
        && matches!(
            o.cells,
            BoardGroupDeletePhase::Observed | BoardGroupDeletePhase::NotRequired
        )
}

#[cfg(test)]
mod tests;

pub(super) fn unconfirmed(mut o: Outcomes) -> Outcomes {
    for p in [&mut o.schema, &mut o.cells] {
        if *p == BoardGroupDeletePhase::Observed {
            *p = BoardGroupDeletePhase::Unconfirmed;
        }
    }
    o
}
