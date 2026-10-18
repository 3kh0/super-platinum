//! Local refinement mutation boundary. Save permits are a separate worker path.
use super::*;
use std::sync::Arc;
use super_platinum_core::slack_lists::saved_views::{self, Projection};
use super_platinum_core::slack_lists::{ViewRefinementChanges, ViewRefinementDraft};
pub(crate) mod controls;
mod state;
pub(crate) mod toolbar;
pub(super) use state::State;

#[derive(Clone, Debug)]
pub(crate) struct Action {
    identity: Arc<()>,
    epoch: u64,
    visit: u64,
    read: u64,
    revision: u64,
    selection: u64,
    layout: u64,
    id: String,
    document: Value,
    records: Value,
    state: State,
    provider: Option<super_platinum_core::slack_lists::BoardLaneOrderDraft>,
    projection: Projection,
    find_input: String,
    find_applied: String,
    find_revision: u64,
}
impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && (
                self.epoch,
                self.visit,
                self.read,
                self.revision,
                self.selection,
                self.layout,
                &self.id,
                &self.document,
                &self.records,
                &self.state,
                &self.provider,
                &self.projection,
            ) == (
                other.epoch,
                other.visit,
                other.read,
                other.revision,
                other.selection,
                other.layout,
                &other.id,
                &other.document,
                &other.records,
                &other.state,
                &other.provider,
                &other.projection,
            )
            && (&self.find_input, &self.find_applied, self.find_revision)
                == (&other.find_input, &other.find_applied, other.find_revision)
    }
}
impl Eq for Action {}
fn available(shell: &ShellState) -> bool {
    let Some(scope) = shell.slack_lists.scope.as_ref() else {
        return false;
    };
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    scope.current(shell)
        && shell.overlay == Some(Overlay::SlackList)
        && !scope.deleted(shell, &view.id)
        && !view.loading
        && !view.unavailable
        && view.loaded.is_some()
        && view.editor.is_none()
        && view.detail.is_none()
        && !create::blocks_mutation(shell)
        && !delete::blocks_mutation(shell)
        && !reminder::blocks_mutation(shell)
        && !completion::blocks_mutation(shell)
        && !editor::assignment::blocks_mutation(shell)
        && !shell.slack_lists.operations.contains_key(&view.id)
}
/// Ephemeral renderer input only. Every write retains Loaded.document separately.
pub(super) fn read_document<'a>(
    loaded: &'a Loaded,
    state: &State,
    selected: Option<&str>,
    provider: Option<&super_platinum_core::slack_lists::BoardLaneOrderDraft>,
) -> Result<std::borrow::Cow<'a, Document>, saved_views::UnsupportedView> {
    use std::borrow::Cow;
    let mut doc = if state.is_empty() {
        Cow::Borrowed(&loaded.document)
    } else {
        Cow::Owned(
            state
                .all_document(&loaded.document)
                .map_err(|e| unsupported(&e.to_string()))?,
        )
    };
    if let Some(id) = selected {
        if doc
            .views
            .iter()
            .any(|v| v["id"] == id && v["type"] == "board")
        {
            if !state.is_empty() || provider.is_some() {
                let local = state
                    .draft(&loaded.document, id)
                    .map_err(|e| unsupported(&e.to_string()))?;
                let selected =
                    super_platinum_core::slack_lists::BoardLaneOrder::refined_projection_document(
                        &loaded.document,
                        &local,
                        provider,
                    )
                    .map_err(|e| unsupported(&e.to_string()))?;
                let replacement = selected
                    .views
                    .iter()
                    .find(|v| v["id"] == id)
                    .ok_or_else(|| unsupported("The current view is unavailable."))?
                    .clone();
                let mut raw = doc.raw.clone();
                *raw["list_metadata"]["views"]
                    .as_array_mut()
                    .ok_or_else(|| unsupported("The current views are unavailable."))?
                    .iter_mut()
                    .find(|v| v["id"] == id)
                    .ok_or_else(|| unsupported("The current view is unavailable."))? = replacement;
                doc = Cow::Owned(Document::parse(raw).map_err(|e| unsupported(&e.to_string()))?);
            }
        }
    }
    Ok(doc)
}
pub(super) fn document(
    view: &View,
) -> Result<std::borrow::Cow<'_, Document>, saved_views::UnsupportedView> {
    let loaded = view
        .loaded
        .as_ref()
        .ok_or_else(|| unsupported("The List is unavailable."))?;
    read_document(
        loaded,
        &view.layout.refinements,
        view.layout.selected.as_deref(),
        view.layout.board_order.as_ref(),
    )
}
fn unsupported(reason: &str) -> saved_views::UnsupportedView {
    saved_views::UnsupportedView {
        reason: reason.into(),
    }
}
fn projection(shell: &ShellState) -> Option<Projection> {
    let view = shell.slack_lists.view.as_ref()?;
    let loaded = view.loaded.as_ref()?;
    let id = view.layout.selected.as_deref()?;
    let doc = document(view).ok()?;
    let raw = doc.views.iter().find(|v| v["id"] == id)?;
    let selected = saved_views::SavedView::parse(raw.clone()).ok()?;
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|s| shell.core.workspaces.get(&s.team));
    let context = layout::projection_context(view, loaded, workspace, raw).ok()?;
    saved_views::project(&doc, &loaded.records, &selected, &context).ok()
}
pub(crate) fn capture(shell: &ShellState) -> Option<Action> {
    if !available(shell) || rename::blocks_mutation(shell) {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    let loaded = view.loaded.as_ref()?;
    Some(Action {
        identity: shell.slack_lists.scope.as_ref()?.identity.clone(),
        epoch: shell.core.account_epoch,
        visit: view.generation,
        read: view.read,
        revision: view.revision,
        selection: view.layout.selection_generation,
        layout: view.layout.generation,
        id: view.layout.selected.clone()?,
        document: loaded.document.raw.clone(),
        records: loaded.records.raw.clone(),
        state: view.layout.refinements.clone(),
        provider: view.layout.board_order.clone(),
        projection: projection(shell)?,
        find_input: view.layout.find.input.clone(),
        find_applied: view.layout.find.applied.clone(),
        find_revision: view.layout.find.revision,
    })
}
impl Action {
    pub(super) fn view_id(&self) -> &str {
        &self.id
    }
    pub(super) fn matches_document(&self, document: &Document) -> bool {
        self.document == document.raw
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        !rename::blocks_mutation(shell) && self.current_retained(shell)
    }
    pub(super) fn current_retained(&self, shell: &ShellState) -> bool {
        available(shell)
            && shell.core.account_epoch == self.epoch
            && shell
                .slack_lists
                .scope
                .as_ref()
                .is_some_and(|s| Arc::ptr_eq(&s.identity, &self.identity))
            && projection(shell).as_ref() == Some(&self.projection)
            && shell.slack_lists.view.as_ref().is_some_and(|v| {
                v.generation == self.visit
                    && v.read == self.read
                    && v.revision == self.revision
                    && v.layout.selected.as_deref() == Some(&self.id)
                    && v.layout.selection_generation == self.selection
                    && v.layout.generation == self.layout
                    && v.layout.refinements == self.state
                    && v.layout.board_order == self.provider
                    && v.layout.find.input == self.find_input
                    && v.layout.find.applied == self.find_applied
                    && v.layout.find.revision == self.find_revision
                    && v.loaded.as_ref().is_some_and(|l| {
                        l.document.raw == self.document && l.records.raw == self.records
                    })
            })
    }
    pub(crate) fn draft(&self, shell: &ShellState) -> Option<ViewRefinementDraft> {
        if !self.current(shell) {
            return None;
        }
        self.draft_retained(shell)
    }
    pub(super) fn draft_retained(&self, shell: &ShellState) -> Option<ViewRefinementDraft> {
        if !self.current_retained(shell) {
            return None;
        }
        let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
        self.state.draft(&loaded.document, &self.id).ok()
    }
}
pub(crate) fn changes(shell: &ShellState) -> Option<ViewRefinementChanges> {
    let view = shell.slack_lists.view.as_ref()?;
    let loaded = view.loaded.as_ref()?;
    view.layout
        .refinements
        .changes(&loaded.document, view.layout.selected.as_deref()?)
        .ok()
}
/// Single synchronous publication after actual preview projection succeeds.
pub(crate) fn apply(shell: &mut ShellState, action: &Action, draft: ViewRefinementDraft) -> bool {
    if rename::blocks_mutation(shell) {
        return false;
    }
    apply_retained(shell, action, draft)
}
/// Existing local editor owns the only registry entry while publishing this read
/// draft. Its caller must first validate that exact entry/serial/action.
pub(super) fn apply_retained(
    shell: &mut ShellState,
    action: &Action,
    draft: ViewRefinementDraft,
) -> bool {
    if !action.current_retained(shell) || draft.view_id() != action.id {
        return false;
    }
    let Some(view) = shell.slack_lists.view.as_ref() else {
        return false;
    };
    let Some(loaded) = view.loaded.as_ref() else {
        return false;
    };
    if draft.list_id() != loaded.document.id || draft.projection_document(&loaded.document).is_err()
    {
        return false;
    }
    let mut preview = view.clone();
    if !preview.layout.refinements.replace(draft.clone()) {
        return false;
    }
    let workspace = shell
        .slack_lists
        .scope
        .as_ref()
        .and_then(|s| shell.core.workspaces.get(&s.team));
    layout::rebuild(&mut preview, workspace);
    if preview.layout.notice.is_some() {
        return false;
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    view.layout.refinements.replace(draft);
    view.layout.generation = next_id();
    view.layout.grouping_revision = next_id();
    rebuild_rows(shell);
    true
}
pub(crate) fn reset(shell: &mut ShellState, action: &Action) -> bool {
    if !action.current(shell) {
        return false;
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    if !view.layout.refinements.reset() {
        return false;
    }
    view.layout.generation = next_id();
    view.layout.grouping_revision = next_id();
    rebuild_rows(shell);
    true
}

#[cfg(test)]
mod tests;
