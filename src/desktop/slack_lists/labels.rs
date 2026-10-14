//! Event-driven cached mention labels. The index is built with the list snapshot;
//! metadata events only reproject rows depending on the changed identity.
use super::*;
use crate::model::RichNode;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};
use super_platinum_core::{slack::events::RtEvent, state::Workspace};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reference {
    User(String),
    Channel(String),
    Group(String),
}
impl Reference {
    fn dm_user(&self, workspace: &Workspace) -> Option<String> {
        match self {
            Self::Channel(id) => workspace
                .channels
                .get(id)
                .filter(|channel| channel.is_im)
                .and_then(super_platinum_core::state::dm_user_id)
                .map(str::to_owned),
            _ => None,
        }
    }
    fn label(&self, workspace: &Workspace) -> Option<String> {
        match self {
            Self::User(id) => workspace
                .users
                .contains_key(id)
                .then(|| workspace.display_name(id)),
            Self::Channel(id) => workspace.channels.get(id).map(|channel| {
                super_platinum_core::state::channel_display_name(workspace, channel)
            }),
            Self::Group(id) => workspace
                .usergroups
                .get(id)
                .map(|group| group.handle.clone()),
        }
    }
}
#[derive(Clone, Default)]
pub(super) struct Index {
    rows: BTreeMap<Reference, BTreeSet<usize>>,
    records: HashMap<String, usize>,
    by_row: HashMap<usize, BTreeSet<Reference>>,
}
impl Index {
    pub(super) fn build(
        loaded: Option<&Loaded>,
        rows: &[Arc<presentation::RowView>],
        workspace: Option<&Workspace>,
    ) -> Self {
        let Some(loaded) = loaded else {
            return Self::default();
        };
        let mut index = Self {
            records: loaded
                .records
                .records
                .iter()
                .enumerate()
                .map(|(i, row)| (row.id.clone(), i))
                .collect(),
            ..Self::default()
        };
        for (position, row) in rows.iter().enumerate() {
            let references = index.references(loaded, row, workspace);
            index.replace(position, references);
        }
        index
    }
    fn references(
        &self,
        loaded: &Loaded,
        row: &presentation::RowView,
        workspace: Option<&Workspace>,
    ) -> BTreeSet<Reference> {
        let mut references = BTreeSet::new();
        for cell in row.cells.iter() {
            if let CellContent::Rich(nodes) = &cell.content {
                collect_nodes(nodes, &mut references);
            }
        }
        // Missing users/channels render as plain IDs; retain their dependency
        // so a later authoritative metadata event can make them navigable.
        if let Some(record) = self
            .records
            .get(&row.id)
            .and_then(|i| loaded.records.records.get(*i))
        {
            for column in &loaded.document.columns {
                let key = match column.kind.as_str() {
                    "user" | "assignee" | "todo_assignee" | "vote" => "user",
                    "channel" => "channel",
                    _ => continue,
                };
                for id in record
                    .fields
                    .get(&column.key)
                    .and_then(|field| field.get(key))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    references.insert(if key == "user" {
                        Reference::User(id.into())
                    } else {
                        Reference::Channel(id.into())
                    });
                }
            }
        }
        // A direct-message channel label is the user's display name.
        let dm_users = references
            .iter()
            .filter_map(|reference| match reference {
                Reference::Channel(id) => workspace?
                    .channels
                    .get(id)
                    .filter(|channel| channel.is_im)
                    .and_then(super_platinum_core::state::dm_user_id)
                    .map(str::to_owned),
                _ => None,
            })
            .collect::<Vec<_>>();
        references.extend(dm_users.into_iter().map(Reference::User));
        references
    }
    fn replace(&mut self, position: usize, references: BTreeSet<Reference>) {
        if let Some(old) = self.by_row.insert(position, references.clone()) {
            for reference in old.difference(&references) {
                if let Some(rows) = self.rows.get_mut(reference) {
                    rows.remove(&position);
                    if rows.is_empty() {
                        self.rows.remove(reference);
                    }
                }
            }
        }
        for reference in references {
            self.rows.entry(reference).or_default().insert(position);
        }
    }
}
fn collect_nodes(nodes: &[RichNode], refs: &mut BTreeSet<Reference>) {
    // Nodes already passed the rich projection's depth/node budget.
    for node in nodes {
        match node {
            RichNode::UserMention { user_id, .. } => {
                refs.insert(Reference::User(user_id.clone()));
            }
            RichNode::ChannelMention { channel_id, .. } => {
                refs.insert(Reference::Channel(channel_id.clone()));
            }
            RichNode::GroupMention { group_id, .. } => {
                refs.insert(Reference::Group(group_id.clone()));
            }
            RichNode::Paragraph(nodes) | RichNode::Quote(nodes) => collect_nodes(nodes, refs),
            RichNode::List { items, .. } => {
                for nodes in items {
                    collect_nodes(nodes, refs);
                }
            }
            _ => {}
        }
    }
}

pub(crate) struct Change {
    team: String,
    rt_generation: u64,
    visit: u64,
    reference: Reference,
    before: Option<String>,
    dm_user: Option<String>,
}
fn event_reference(event: &RtEvent) -> Option<Reference> {
    match event {
        RtEvent::UserChanged(user) => Some(Reference::User(user.id.clone())),
        RtEvent::Message(message)
            if matches!(
                message.subtype.as_deref(),
                Some("channel_name" | "group_name")
            ) =>
        {
            message.channel.clone().map(Reference::Channel)
        }
        RtEvent::Unknown(raw) => match raw.kind.as_str() {
            "channel_rename" | "group_rename" | "channel_created" | "group_created"
            | "channel_joined" | "group_joined" => raw
                .rest
                .get("channel")?
                .get("id")?
                .as_str()
                .map(|id| Reference::Channel(id.into())),
            "subteam_created" | "subteam_updated" => raw
                .rest
                .get("subteam")?
                .get("id")?
                .as_str()
                .map(|id| Reference::Group(id.into())),
            _ => None,
        },
        _ => None,
    }
}
/// Capture before core applies this event; unrelated events do not scan rows.
pub(crate) fn prepare(
    shell: &ShellState,
    team: &str,
    rt_generation: u64,
    event: &RtEvent,
) -> Option<Change> {
    let scope = shell
        .slack_lists
        .scope
        .as_ref()
        .filter(|scope| scope.current(shell) && scope.team == team)?;
    let workspace = shell
        .core
        .workspaces
        .get(&scope.team)
        .filter(|w| w.rt_generation == rt_generation)?;
    let view = shell.slack_lists.view.as_ref()?;
    let reference = event_reference(event)?;
    view.label_rows.rows.get(&reference)?;
    Some(Change {
        team: team.into(),
        rt_generation,
        visit: view.generation,
        before: reference.label(workspace),
        dm_user: reference.dm_user(workspace),
        reference,
    })
}
impl Change {
    /// Apply after the core has accepted/rejected the event. Stale events and
    /// status-only updates preserve the entire cached projection by Arc identity.
    pub(crate) fn apply(self, shell: &mut ShellState) -> bool {
        if !shell
            .slack_lists
            .scope
            .as_ref()
            .is_some_and(|scope| scope.current(shell) && scope.team == self.team)
        {
            return false;
        }
        let Some(workspace) = shell
            .core
            .workspaces
            .get(&self.team)
            .filter(|w| w.rt_generation == self.rt_generation)
        else {
            return false;
        };
        if self.before == self.reference.label(workspace)
            && self.dm_user == self.reference.dm_user(workspace)
        {
            return false;
        }
        let Some(view) = shell
            .slack_lists
            .view
            .as_mut()
            .filter(|view| view.generation == self.visit)
        else {
            return false;
        };
        let Some(loaded) = view.loaded.as_ref() else {
            return false;
        };
        let Some(positions) = view.label_rows.rows.get(&self.reference) else {
            return false;
        };
        let positions = positions.clone();
        let all_rows = Arc::make_mut(&mut view.all_rows);
        let mut updated = HashMap::new();
        for position in &positions {
            let old = &all_rows[*position];
            let Some(record) = view
                .label_rows
                .records
                .get(&old.id)
                .and_then(|i| loaded.records.records.get(*i))
            else {
                continue;
            };
            let (cells, label) =
                presentation::project_cells(loaded, record, Some(workspace), &shell.media);
            let mut row = (**old).clone();
            row.cells = Arc::new(cells);
            row.label = label;
            let row = Arc::new(row);
            let references = view.label_rows.references(loaded, &row, Some(workspace));
            view.label_rows.replace(*position, references);
            updated.insert(row.id.clone(), row.clone());
            all_rows[*position] = row;
        }
        if view.layout.selected.is_some()
            || loaded
                .document
                .columns
                .iter()
                .any(super_platinum_core::slack_lists::timestamps::supports)
        {
            layout::rebuild(view, Some(workspace));
            return true;
        }
        // Preserve every unaffected row Arc, including expanded parents.
        for visible in Arc::make_mut(&mut view.rows) {
            if let Some(base) = updated.get(&visible.id) {
                if visible.expanded {
                    let mut expanded = (**base).clone();
                    expanded.expanded = true;
                    *visible = Arc::new(expanded);
                } else {
                    *visible = base.clone();
                }
            }
        }
        layout::fallback_entries(view);
        true
    }
}

#[cfg(test)]
mod tests;
