//! Captured Board card moves; the shared schema worker owns dispatch and receipts.
use super::*;
use super_platinum_core::slack_lists::saved_views::{self, ProjectionContext};
use super_platinum_core::slack_lists::{BoardRecordMove, BoardRecordMoveAttempt};

#[derive(Clone, Debug)]
pub(super) struct ContextIdentity(pub(super) std::sync::Arc<()>);
impl PartialEq for ContextIdentity {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ContextIdentity {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Drag {
    pub view: String,
    pub source: String,
    pub record: String,
    pub instance: String,
    identity: ContextIdentity,
    epoch: u64,
    canonical: saved_views::Projection,
    visit: u64,
    selection: u64,
    projection: lane_order::ReadProjection,
    document: Value,
    records: Value,
    lanes: Vec<(String, Vec<(String, String)>)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Action {
    pub drag: Drag,
    pub destination: String,
    pub before: Option<String>,
}
fn lanes(shell: &ShellState) -> Option<Vec<(String, Vec<(String, String)>)>> {
    Some(
        shell
            .slack_lists
            .view
            .as_ref()?
            .layout
            .lanes
            .iter()
            .map(|l| {
                (
                    l.key.clone(),
                    l.rows
                        .iter()
                        .map(|r| (r.id.clone(), r.instance_id.clone()))
                        .collect(),
                )
            })
            .collect(),
    )
}
pub(super) fn context(shell: &ShellState, loaded: &Loaded, id: &str) -> Option<ProjectionContext> {
    let view = shell.slack_lists.view.as_ref()?;
    let local = view.layout.refinements.draft(&loaded.document, id).ok()?;
    let read = super_platinum_core::slack_lists::BoardLaneOrder::refined_projection_document(
        &loaded.document,
        &local,
        view.layout.board_order.as_ref(),
    )
    .ok()?;
    let raw = read.views.iter().find(|v| v["id"] == id)?;
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|s| shell.core.workspaces.get(&s.team));
    super::super::layout::projection_context(view, loaded, workspace, raw).ok()
}
pub(super) fn canonical(shell: &ShellState, id: &str) -> Option<saved_views::Projection> {
    let l = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    let view = shell.slack_lists.view.as_ref()?;
    let local = view.layout.refinements.draft(&l.document, id).ok()?;
    let read = super_platinum_core::slack_lists::BoardLaneOrder::refined_projection_document(
        &l.document,
        &local,
        view.layout.board_order.as_ref(),
    )
    .ok()?;
    let raw = read.views.iter().find(|v| v["id"] == id)?;
    let selected = saved_views::SavedView::parse(raw.clone()).ok()?;
    saved_views::project(&read, &l.records, &selected, &context(shell, l, id)?).ok()
}
impl Drag {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.slack_lists.scope.as_ref().is_some_and(|s| {
                s.current(shell) && std::sync::Arc::ptr_eq(&s.identity, &self.identity.0)
            })
            && canonical(shell, &self.view).as_ref() == Some(&self.canonical)
            && self.projection.current(shell)
            && lanes(shell).as_ref() == Some(&self.lanes)
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && v.layout.selection_generation == self.selection
                    && v.layout.selected.as_deref() == Some(&self.view)
                    && v.loaded.as_ref().is_some_and(|l| {
                        l.document.raw == self.document && l.records.raw == self.records
                    })
            })
    }
}
impl Action {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.drag.current(shell)
    }
    pub(super) fn capture(&self, shell: &ShellState, loaded: &Loaded) -> Option<BoardRecordMove> {
        if !self.current(shell) {
            return None;
        }
        let view = shell.slack_lists.view.as_ref()?;
        let local = view
            .layout
            .refinements
            .draft(&loaded.document, &self.drag.view)
            .ok()?;
        BoardRecordMove::capture_refined(
            &loaded.document,
            &loaded.records,
            &local,
            view.layout.board_order.as_ref(),
            &context(shell, loaded, &self.drag.view)?,
            &self.drag.record,
            &self.drag.source,
            &self.destination,
            self.before.as_deref(),
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .ok()
    }
}
pub(crate) fn can_begin(
    shell: &ShellState,
    source: &str,
    instance: &str,
    visit: u64,
    layout: u64,
) -> bool {
    if !owner::available(shell) || blocks_mutation(shell) {
        return false;
    }
    let Some(v) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    if v.generation != visit
        || v.layout.generation != layout
        || v.layout.kind != saved_views::ProjectionLayout::Board
        || v.layout.notice.is_some()
        || v.layout.find.input != v.layout.find.applied
    {
        return false;
    }
    let Some(l) = v.loaded.as_ref() else {
        return false;
    };
    if l.document
        .read_only_reason(l.allow_lists.as_deref())
        .is_some()
        || l.personal_task_lists
            .as_ref()
            .is_none_or(|p| p.contains(&l.document.id))
    {
        return false;
    }
    let Some(raw) = l
        .document
        .views
        .iter()
        .find(|r| r["id"].as_str() == v.layout.selected.as_deref())
    else {
        return false;
    };
    let Some(key) = raw.pointer("/grouping/group_by").and_then(Value::as_str) else {
        return false;
    };
    raw["is_locked"] != true
        && raw["name"].is_string()
        && l.document
            .columns
            .iter()
            .any(|c| c.key == key && matches!(c.kind.as_str(), "select" | "checkbox"))
        && v.layout
            .lanes
            .iter()
            .find(|l| l.key == source)
            .is_some_and(|l| l.rows.iter().any(|r| r.instance_id == instance))
}
/// Capture actual projected membership before native drag starts. It grants no permit.
pub(crate) fn begin(shell: &ShellState, source: &str, instance: &str) -> Option<Drag> {
    if !owner::available(shell) || blocks_mutation(shell) {
        return None;
    }
    let v = shell.slack_lists.view.as_ref()?;
    if v.layout.kind != saved_views::ProjectionLayout::Board
        || v.layout.notice.is_some()
        || v.layout.find.input != v.layout.find.applied
    {
        return None;
    }
    if !can_begin(shell, source, instance, v.generation, v.layout.generation) {
        return None;
    }
    text_create::bounded_population(shell)?;
    let loaded = v.loaded.as_ref()?;
    if serde_json::to_vec(&loaded.document.raw).map_or(true, |b| b.len() > registry::MAX_DOCUMENT) {
        return None;
    }
    let view = v.layout.selected.as_ref()?;
    let row = v
        .layout
        .lanes
        .iter()
        .find(|l| l.key == source)?
        .rows
        .iter()
        .find(|r| r.instance_id == instance)?;
    // Capture the same refined/provider projection used by Drag::current.
    // The retained document and population below remain the canonical raw inputs.
    let canonical = canonical(shell, view)?;
    let actual = lanes(shell)?;
    // A local lane-order draft may reorder groups, but never the cards within one.
    if canonical.groups.len() != actual.len()
        || actual.iter().any(|(key, rows)| {
            canonical
                .groups
                .iter()
                .find(|g| g.key.as_deref() == Some(key.as_str()))
                .is_none_or(|g| {
                    g.rows
                        .iter()
                        .map(|r| (&r.record_id, &r.instance_id))
                        .ne(rows.iter().map(|(r, i)| (r, i)))
                })
        })
    {
        return None;
    }
    Some(Drag {
        identity: ContextIdentity(shell.slack_lists.scope.as_ref()?.identity.clone()),
        epoch: shell.core.account_epoch,
        canonical,
        view: view.clone(),
        source: source.into(),
        record: row.id.clone(),
        instance: instance.into(),
        visit: v.generation,
        selection: v.layout.selection_generation,
        projection: lane_order::ReadProjection::capture(shell)?,
        document: loaded.document.raw.clone(),
        records: loaded.records.raw.clone(),
        lanes: actual,
    })
}
/// `before` is a real projected record ID. None means the entire projected lane's
/// loaded tail, never the last card of a paginated DOM slice or a server tail.
pub(crate) fn destination(
    shell: &ShellState,
    drag: &Drag,
    lane: &str,
    before: Option<&str>,
) -> Option<Action> {
    if !drag.current(shell) {
        return None;
    }
    let a = Action {
        drag: drag.clone(),
        destination: lane.into(),
        before: before.map(str::to_owned),
    };
    let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    a.capture(shell, loaded)?;
    can_target(shell, &Target::CardOrder(a.clone())).then_some(a)
}
pub(super) fn supports(doc: &Document, a: &Action) -> bool {
    doc.raw == a.drag.document
        && doc
            .views
            .iter()
            .any(|v| v["id"] == a.drag.view && v["type"] == "board" && v["name"].is_string())
}
pub(crate) fn open_shell(shell: &mut ShellState, a: Action) -> bool {
    if !a.current(shell)
        || destination(shell, &a.drag, &a.destination, a.before.as_deref()).as_ref() != Some(&a)
    {
        return false;
    }
    let visit = a.drag.visit;
    open_target(shell, Target::CardOrder(a), visit)
}
pub(crate) fn apply(mut state: Signal<ShellState>, action: Action) {
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
pub(super) fn capture(
    shell: &ShellState,
    e: &registry::Entry,
    loaded: &Loaded,
) -> Result<Intent, super_platinum_core::slack_lists::ListError> {
    let Target::CardOrder(a) = &e.target else {
        return text_create::capture(shell, e, loaded);
    };
    if e.input != Input::CardOrder {
        return Err(super_platinum_core::slack_lists::ListError(
            "The card move changed.",
        ));
    }
    a.capture(shell, loaded).map(Intent::CardOrder).ok_or(
        super_platinum_core::slack_lists::ListError(
            "The Board or loaded items changed. Nothing was sent.",
        ),
    )
}
pub(super) fn admit(
    shell: &ShellState,
    intent: &Intent,
    loaded: &network::Metadata,
    page: Option<&RecordsPage>,
) -> Option<Permit> {
    let Intent::CardOrder(i) = intent else {
        return page
            .is_none()
            .then(|| text_create::admit(shell, intent, loaded))
            .flatten();
    };
    let page = page?;
    let current = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    let mut fresh = current.clone();
    fresh.document = loaded.document.clone();
    fresh.records = page.clone();
    fresh.allow_lists = loaded.allow.clone();
    fresh.personal_task_lists = Some(loaded.personal.clone());
    BoardRecordMoveAttempt::new(i.clone())
        .admit(
            &fresh.document,
            &fresh.records,
            &context(shell, &fresh, i.view_id())?,
            fresh.allow_lists.as_deref(),
            fresh.personal_task_lists.as_ref(),
        )
        .ok()
        .map(Permit::CardOrder)
}

#[cfg(test)]
mod tests;
