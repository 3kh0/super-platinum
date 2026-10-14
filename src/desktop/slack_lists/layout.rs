//! Cached saved-table layouts. Instance identity belongs to the grid; writes
//! always retain the canonical record/column identity from the loaded snapshot.
use super::*;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};
use super_platinum_core::{slack_lists::saved_views, state::Workspace};

#[derive(Clone, Debug)]
pub(crate) struct ViewChoice {
    pub id: String,
    pub name: String,
}
#[derive(Clone)]
pub(crate) struct GroupView {
    pub key: String,
    pub label: String,
    pub count: usize,
    pub collapsed: bool,
}
#[derive(Clone)]
pub(crate) struct BoardLane {
    pub key: String,
    pub label: String,
    pub count: usize,
    pub rows: Arc<Vec<Arc<presentation::RowView>>>,
}
#[derive(Clone)]
pub(crate) enum TableEntry {
    Group(GroupView),
    Row(Arc<presentation::RowView>),
}
#[derive(Clone)]
pub(super) struct Layout {
    pub generation: u64,
    pub selection_generation: u64,
    pub selected: Option<String>,
    pub manual: bool,
    pub selection: Option<saved_views::DefaultSelection>,
    pub read_started: bool,
    pub personal_started: bool,
    pub last_read_timestamp: Option<u64>,
    pub dates: date_context::State,
    pub resolved_present: bool,
    pub choices: Arc<Vec<ViewChoice>>,
    pub columns: Arc<Vec<presentation::ColumnView>>,
    pub entries: Arc<Vec<TableEntry>>,
    pub kind: saved_views::ProjectionLayout,
    pub lanes: Arc<Vec<BoardLane>>,
    pub completion_column: Option<String>,
    pub notice: Option<String>,
    pub matched: usize,
    pub collapsed: BTreeMap<String, BTreeSet<String>>,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            generation: next_id(),
            selection_generation: next_id(),
            selected: None,
            manual: false,
            selection: None,
            read_started: false,
            personal_started: false,
            last_read_timestamp: None,
            dates: Default::default(),
            resolved_present: false,
            choices: Default::default(),
            columns: Default::default(),
            entries: Default::default(),
            kind: saved_views::ProjectionLayout::Table,
            lanes: Default::default(),
            completion_column: None,
            notice: None,
            matched: 0,
            collapsed: Default::default(),
        }
    }
}
fn columns(
    document: &Document,
    ids: &[String],
    fallback: bool,
) -> Arc<Vec<presentation::ColumnView>> {
    let pinned = document
        .primary_column()
        .filter(|column| ids.contains(&column.id))
        .map(|column| column.id.as_str());
    let primary = pinned.or_else(|| fallback.then(|| ids.first().map(String::as_str)).flatten());
    Arc::new(
        ids.iter()
            .filter_map(|id| document.column(id))
            .map(|column| presentation::ColumnView {
                id: column.id.clone(),
                label: column.name.clone(),
                primary: primary == Some(column.id.as_str()),
                pinned: pinned == Some(column.id.as_str()),
                size: presentation::ColumnView::size(&column.kind),
            })
            .collect(),
    )
}
pub(super) fn rebuild(view: &mut View, workspace: Option<&Workspace>) {
    let old_kind = view.layout.kind;
    let old_completion = view.layout.completion_column.clone();
    let old_entries = view.layout.entries.clone();
    let old_columns = view.layout.columns.clone();
    rebuild_inner(view, workspace);
    record_detail::reconcile(view);
    let same_rows = old_entries.len() == view.layout.entries.len()
        && old_entries
            .iter()
            .zip(view.layout.entries.iter())
            .all(|(old, new)| match (old, new) {
                (TableEntry::Row(old), TableEntry::Row(new)) => old.instance_id == new.instance_id,
                (TableEntry::Group(old), TableEntry::Group(new)) => {
                    old.key == new.key && old.collapsed == new.collapsed
                }
                _ => false,
            });
    let same_columns = old_columns.len() == view.layout.columns.len()
        && old_columns
            .iter()
            .zip(view.layout.columns.iter())
            .all(|(old, new)| old.id == new.id);
    if old_kind != view.layout.kind
        || old_completion != view.layout.completion_column
        || !same_rows
        || !same_columns
    {
        view.layout.generation = next_id();
    }
}
fn rebuild_inner(view: &mut View, workspace: Option<&Workspace>) {
    let Some(loaded) = view.loaded.as_ref() else {
        return;
    };
    if view.layout.selection.is_none() {
        if let Ok(mut selection) =
            saved_views::DefaultSelection::new(&loaded.document, None, false, None)
        {
            if view.layout.manual {
                let _ = selection.choose(&loaded.document, view.layout.selected.as_deref());
            }
            view.layout.selected = selection.selected_id.clone();
            view.layout.selection = Some(selection);
        }
    }
    let catalog = saved_views::catalog(&loaded.document);
    view.layout.choices = Arc::new(
        catalog
            .as_ref()
            .map(|views| {
                views
                    .iter()
                    .filter(|view| !matches!(view.kind.as_str(), "record" | "canvas_inline"))
                    .map(|view| ViewChoice {
                        id: view.id.clone(),
                        name: view.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    );
    view.layout.notice = None;
    view.layout.kind = saved_views::ProjectionLayout::Table;
    view.layout.lanes = Default::default();
    view.layout.completion_column = None;
    let selected = catalog.as_ref().ok().and_then(|views| {
        views
            .iter()
            .find(|candidate| Some(candidate.id.as_str()) == view.layout.selected.as_deref())
    });
    date_context::prepare(
        &mut view.layout.dates,
        &loaded.document,
        selected,
        view.layout.last_read_timestamp,
    );
    presentation::project_timestamps(
        loaded,
        &mut view.all_rows,
        workspace,
        view.layout.dates.context.as_ref(),
    );
    if view.layout.selected.is_none() {
        view.layout.columns = columns(
            &loaded.document,
            &loaded
                .document
                .columns
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>(),
            true,
        );
        super::rebuild_visible(view);
        return;
    }
    let id = view.layout.selected.as_deref().unwrap();
    let result = catalog.and_then(|views| {
        let selected = views.iter().find(|view| view.id == id).ok_or_else(|| {
            saved_views::UnsupportedView {
                reason: "This saved view was removed. Choose another view or All loaded items."
                    .into(),
            }
        })?;
        if loaded
            .document
            .raw
            .pointer("/list_metadata/todo_mode")
            .is_some_and(|value| !value.is_boolean())
        {
            return Err(saved_views::UnsupportedView {
                reason: "This saved view has an unsupported task-mode setting.".into(),
            });
        }
        let saved_completed = match selected.raw.get("show_completed_items") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(value)) => Some(*value),
            Some(_) => {
                return Err(saved_views::UnsupportedView {
                    reason: "This saved view has an unsupported completed-items setting.".into(),
                });
            }
        };
        let mut context = context(loaded, workspace, view.layout.collapsed.get(id));
        context.date = view.layout.dates.context.clone();
        context.date_uses_timestamps = date_context::uses_timestamps(loaded);
        let policy = saved_views::CompletionPolicy::resolve(
            context.todo_mode,
            loaded
                .list_experiments
                .as_ref()
                .map(|flags| flags.grid_todos),
            loaded
                .personal_task_lists
                .as_ref()
                .map(|policy| policy.contains(&loaded.document.id)),
            loaded
                .list_experiments
                .as_ref()
                .map(|flags| flags.todos_rollback),
            saved_completed,
            None,
        )?;
        context.apply_todo_completion_rules = Some(policy.apply_rules);
        context.effective_show_completed = Some(policy.effective_show_completed);
        saved_views::project(&loaded.document, &loaded.records, selected, &context)
    });
    match result {
        Ok(projected) => {
            view.layout.kind = projected.layout;
            view.layout.completion_column = projected.board_completion_column_id.clone();
            view.layout.columns = columns(&loaded.document, &projected.column_ids, false);
            let mut cell_ids = projected.column_ids.clone();
            if let Some(id) = &projected.board_completion_column_id {
                if !cell_ids.contains(id) {
                    cell_ids.push(id.clone());
                }
            }
            let bases = view
                .all_rows
                .iter()
                .map(|row| (row.id.as_str(), row))
                .collect::<HashMap<_, _>>();
            let mut instances = Vec::new();
            let mut entries = Vec::new();
            let mut selected_cells = HashMap::new();
            let mut lanes = Vec::new();
            for group in projected.groups {
                let mut lane = BoardLane {
                    key: group.key.clone().unwrap_or_default(),
                    label: group.label.clone(),
                    count: group.count,
                    rows: Default::default(),
                };
                let mut lane_rows = Vec::new();
                if let Some(key) = group.key {
                    entries.push(TableEntry::Group(GroupView {
                        key,
                        label: group.label,
                        count: group.count,
                        collapsed: group.collapsed,
                    }));
                }
                for instance in group.rows {
                    let Some(base) = bases.get(instance.record_id.as_str()) else {
                        continue;
                    };
                    let mut row = (***base).clone();
                    row.instance_id = instance.instance_id;
                    row.depth = 0;
                    row.has_children = false;
                    row.expanded = false;
                    row.parent_id = None;
                    row.parent_notice = None;
                    row.cells = selected_cells
                        .entry(row.id.clone())
                        .or_insert_with(|| {
                            Arc::new(
                                cell_ids
                                    .iter()
                                    .filter_map(|id| {
                                        base.cells.iter().find(|cell| &cell.column_id == id)
                                    })
                                    .cloned()
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .clone();
                    let row = Arc::new(row);
                    instances.push(row.clone());
                    lane_rows.push(row.clone());
                    entries.push(TableEntry::Row(row));
                }
                lane.rows = Arc::new(lane_rows);
                lanes.push(lane);
            }
            view.layout.lanes = Arc::new(lanes);
            view.layout.matched = projected.unique_record_count;
            view.rows = Arc::new(instances);
            view.layout.entries = Arc::new(entries);
        }
        Err(error) => {
            view.layout.notice = Some(error.reason);
            view.layout.columns = Default::default();
            view.layout.entries = Default::default();
            view.layout.matched = 0;
            view.rows = Default::default();
        }
    }
}
pub(super) fn fallback_entries(view: &mut View) {
    record_detail::reconcile(view);
    view.layout.matched = view.all_rows.len();
    view.layout.entries = Arc::new(view.rows.iter().cloned().map(TableEntry::Row).collect());
}
fn context(
    loaded: &Loaded,
    workspace: Option<&Workspace>,
    collapsed: Option<&BTreeSet<String>>,
) -> saved_views::ProjectionContext {
    let mut context = saved_views::ProjectionContext {
        editable: loaded.document.editable(loaded.allow_lists.as_deref()),
        viewer_id: workspace.map(|w| w.self_user_id.clone()),
        collapsed_groups: collapsed.cloned().unwrap_or_default(),
        todo_mode: loaded.document.raw.pointer("/list_metadata/todo_mode")
            == Some(&Value::Bool(true)),
        ..Default::default()
    };
    let Some(workspace) = workspace else {
        return context;
    };
    // Only collect referenced identities, never clone the whole workspace roster.
    let mut users = BTreeSet::new();
    let mut channels = BTreeSet::new();
    for record in &loaded.records.records {
        for field in record.fields.values() {
            for (key, ids) in [("user", &mut users), ("channel", &mut channels)] {
                for id in field
                    .get(key)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    ids.insert(id.to_owned());
                }
            }
            if let Some(nodes) = field
                .get("rich_text")
                .or_else(|| field.get("richText"))
                .and_then(Value::as_array)
            {
                collect_refs(nodes, 0, &mut 2048, &mut users, &mut channels);
            }
        }
    }
    for id in users {
        if let Some(user) = workspace.users.get(&id) {
            context.user_real_names.insert(
                id.clone(),
                user.real_name
                    .clone()
                    .or_else(|| {
                        user.profile
                            .as_ref()
                            .and_then(|profile| profile.real_name.clone())
                    })
                    .unwrap_or_default(),
            );
            context
                .user_display_names
                .insert(id.clone(), workspace.display_name(&id));
        }
    }
    for id in channels {
        if let Some(channel) = workspace.channels.get(&id) {
            context.channel_names.insert(
                id,
                super_platinum_core::state::channel_display_name(workspace, channel),
            );
        }
    }
    context
}
fn collect_refs(
    nodes: &[Value],
    depth: usize,
    budget: &mut usize,
    users: &mut BTreeSet<String>,
    channels: &mut BTreeSet<String>,
) {
    if depth > 32 {
        return;
    }
    for node in nodes.iter().take(*budget) {
        if *budget == 0 {
            break;
        }
        *budget = budget.saturating_sub(1);
        match node["type"].as_str() {
            Some("user") => {
                if let Some(id) = node["user_id"].as_str() {
                    users.insert(id.to_owned());
                }
            }
            Some("channel") => {
                if let Some(id) = node["channel_id"].as_str() {
                    channels.insert(id.to_owned());
                }
            }
            _ => {}
        }
        if let Some(children) = node["elements"].as_array() {
            collect_refs(children, depth + 1, budget, users, channels);
        }
    }
}
pub(super) fn current(shell: &ShellState, visit: u64, layout: u64) -> bool {
    checkbox_visit_current(shell, visit)
        && shell
            .slack_lists
            .view
            .as_ref()
            .is_some_and(|view| view.layout.generation == layout)
}
pub(crate) fn select_view(
    mut state: Signal<ShellState>,
    id: Option<String>,
    visit: u64,
    layout: u64,
) {
    select_shell(&mut state.write(), id, visit, layout);
}
pub(super) fn select_shell(
    shell: &mut ShellState,
    id: Option<String>,
    visit: u64,
    layout: u64,
) -> bool {
    if !current(shell, visit, layout) {
        return false;
    }
    let team = shell.slack_lists.scope.as_ref().unwrap().team.clone();
    let view = shell.slack_lists.view.as_mut().unwrap();
    if id
        .as_ref()
        .is_some_and(|id| !view.layout.choices.iter().any(|choice| &choice.id == id))
    {
        return false;
    }
    if let Some(selection) = view.layout.selection.as_mut() {
        if selection
            .choose(&view.loaded.as_ref().unwrap().document, id.as_deref())
            .is_err()
        {
            return false;
        }
    }
    if view.layout.selected == id {
        view.layout.manual = true;
        return true;
    }
    create::close_visit(shell);
    delete::close_visit(shell);
    record_detail::dismiss_shell(shell);
    let view = shell.slack_lists.view.as_mut().unwrap();
    view.layout.selected = id;
    view.layout.selection_generation = next_id();
    view.layout.manual = true;
    view.layout.generation = next_id();
    rebuild(view, shell.core.workspaces.get(&team));
    true
}
pub(crate) fn toggle_group(mut state: Signal<ShellState>, group: String, visit: u64, layout: u64) {
    toggle_group_shell(&mut state.write(), &group, visit, layout);
}
pub(super) fn toggle_group_shell(
    shell: &mut ShellState,
    group: &str,
    visit: u64,
    layout: u64,
) -> bool {
    if !current(shell, visit, layout) {
        return false;
    }
    let team = shell.slack_lists.scope.as_ref().unwrap().team.clone();
    let view = shell.slack_lists.view.as_mut().unwrap();
    if view.detail.is_some() {
        return false;
    }
    let Some(selected) = view.layout.selected.clone() else {
        return false;
    };
    if !view
        .layout
        .entries
        .iter()
        .any(|entry| matches!(entry,TableEntry::Group(candidate) if candidate.key==group))
    {
        return false;
    }
    let collapsed = view.layout.collapsed.entry(selected).or_default();
    if !collapsed.remove(group) {
        collapsed.insert(group.to_owned());
    }
    protect_selection(view);
    view.layout.generation = next_id();
    rebuild(view, shell.core.workspaces.get(&team));
    true
}
pub(super) fn cell_visible(view: &View, row: &str, column: &str) -> bool {
    view.rows.iter().any(|candidate| {
        candidate.id == row && candidate.cells.iter().any(|cell| cell.column_id == column)
    })
}
pub(super) fn instance_record(
    shell: &ShellState,
    instance: &str,
    column: &str,
    visit: u64,
    layout: u64,
) -> Option<String> {
    if !current(shell, visit, layout) || shell.slack_lists.view.as_ref()?.detail.is_some() {
        return None;
    }
    shell
        .slack_lists
        .view
        .as_ref()?
        .rows
        .iter()
        .find(|row| {
            row.instance_id == instance && row.cells.iter().any(|cell| cell.column_id == column)
        })
        .map(|row| row.id.clone())
}
#[cfg(test)]
mod tests;

pub(super) fn protect_selection(view: &mut View) {
    view.layout.manual = true;
    if let (Some(selection), Some(loaded)) = (view.layout.selection.as_mut(), view.loaded.as_ref())
    {
        let _ = selection.choose(&loaded.document, view.layout.selected.as_deref());
    }
}
