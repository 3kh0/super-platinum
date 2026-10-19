//! Raw-preserving ordinary Lists model. Protocol evidence: docs/slack-lists-protocol.md.
use crate::slack::models::File;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

mod board_record_move;
mod cell_edit;
pub mod comments;
pub use board_record_move::{
    BoardRecordMove, BoardRecordMoveAttempt, BoardRecordMovePermit, BoardRecordMovePhase,
};
mod column_create;
mod rating;
pub use rating::RatingOptions;
mod vote;
pub use vote::{MAX_VOTERS, VoteIntent, VoteOptions};
mod column_delete;
mod column_precision;
mod column_rename;
pub use column_delete::{ColumnDelete, ColumnDeleteAttempt, ColumnDeletePermit, ColumnDeletePhase};
mod board_lane_order;
mod field_visibility;
mod view_create;
mod view_filter;
mod view_grouping;
mod view_refinement;
pub use board_lane_order::{
    BoardLaneOrder, BoardLaneOrderAttempt, BoardLaneOrderDraft, BoardLaneOrderPermit,
    BoardLaneOrderPhase, BoardLanePlacement,
};
pub use column_create::{
    ColumnCreate, ColumnCreateAttempt, ColumnCreateKind, ColumnCreatePermit, ColumnCreatePhase,
    EntityCreateDefaults, EntityCreateDomain, EntityCreateFormat, EntityCreateOptions,
    NumberCreatePrecision, SelectCreateFormat, SelectCreateOption, SelectCreateOptions,
};
pub use view_refinement::{ViewRefinementChanges, ViewRefinementDraft};
mod view_rename;
mod view_reorder;
pub use view_reorder::{
    ViewReorder, ViewReorderAttempt, ViewReorderDirection, ViewReorderPermit, ViewReorderPhase,
};
mod view_sort;
pub use column_precision::{
    ColumnPrecision, ColumnPrecisionAttempt, ColumnPrecisionPermit, ColumnPrecisionPhase,
};
pub use column_rename::{ColumnRename, ColumnRenameAttempt, ColumnRenamePermit, ColumnRenamePhase};
pub use field_visibility::{
    ViewFieldVisibility, ViewFieldVisibilityAttempt, ViewFieldVisibilityPermit,
    ViewFieldVisibilityPhase,
};
pub use view_create::{
    ViewCreate, ViewCreateAttempt, ViewCreatePermit, ViewCreatePhase, ViewLayout,
};
pub use view_filter::{
    DateFilterOperator, NumberFilterOperator, SelectFilterOperator, ViewFilter, ViewFilterAttempt,
    ViewFilterChoice, ViewFilterPermit, ViewFilterPhase,
};
pub use view_grouping::{
    ViewGrouping, ViewGroupingAttempt, ViewGroupingChoice, ViewGroupingPermit, ViewGroupingPhase,
};
pub use view_rename::{ViewRename, ViewRenameAttempt, ViewRenamePermit, ViewRenamePhase};
mod subitem;
pub use subitem::{
    SUBITEM_INPUT_LIMIT, SubitemCreateAttempt, SubitemCreatePermit, SubitemCreationIntent,
    SubitemReadback, subitem_input_bytes,
};
mod creation;
pub use creation::{CreationIntent, ListClock, ListTimestamp};
mod deletion;
pub use deletion::{
    AcceptedDeletion, AcceptedDeletionState, DeletionIntent, DeletionStateIntent,
    ListRecordReadback,
};
mod cells;
mod edits;
mod hierarchy;
mod hydration;
pub use hydration::ListRecordHydration;
pub mod find;
pub mod numbers;
mod ordering;
mod policy;
pub mod saved_views;
pub mod timestamps;
pub use hierarchy::{HierarchyRow, ParentRelation};
mod presentation;
pub use cell_edit::{
    CellEdit, CellEditValue, LinkValue, TaskAssignment, TaskAssignmentDecision, TaskCompletion,
    TaskCompletionCell, TaskCompletionReference,
};
mod reminder;
pub use cells::CellValue;
pub use edits::CheckboxEdit;
pub use policy::PersonalTaskLists;
pub use presentation::{CellPresentation, ReferenceKind, SelectLabel};
pub use reminder::{ListReminderChange, ListReminderEdit};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListError(pub &'static str);
impl std::fmt::Display for ListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ListError {}

pub fn is_list(file: &File) -> bool {
    (file.filetype.as_deref() == Some("list")
        || file.extra.get("mode").and_then(Value::as_str) == Some("list"))
        && file.extra.get("subtype").and_then(Value::as_str) != Some("poll")
}
fn is_list_raw(file: &Value) -> bool {
    (file["filetype"] == "list" || file["mode"] == "list") && file["subtype"] != "poll"
}
fn unavailable(raw: &Value) -> bool {
    // Keep the two canonical file tombstone markers aligned with file_actions::deleted.
    raw["mode"] == "tombstone"
        || raw["is_tombstoned"] == true
        || raw
            .get("is_deleted")
            .is_some_and(|v| v != &Value::Bool(false) && !v.is_null())
}
fn identity(raw: &Value, key: &str) -> Result<String, ListError> {
    raw.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .map(str::to_owned)
        .ok_or(ListError("List identity is missing or malformed."))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub columns: Vec<Column>,
    pub views: Vec<Value>,
    pub raw: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub id: String,
    pub key: String,
    pub name: String,
    pub kind: String,
    pub raw: Value,
}
impl Document {
    pub fn from_file(file: &File) -> Result<Self, ListError> {
        Self::parse(
            serde_json::to_value(file).map_err(|_| ListError("List file could not be read."))?,
        )
    }
    pub fn parse(raw: Value) -> Result<Self, ListError> {
        if !is_list_raw(&raw) {
            return Err(ListError("This file is not a List."));
        }
        if unavailable(&raw) {
            return Err(ListError("This List is deleted or unavailable."));
        }
        let id = identity(&raw, "id")?;
        let schema = raw
            .pointer("/list_metadata/schema")
            .and_then(Value::as_array)
            .ok_or(ListError("List schema is missing or malformed."))?;
        let mut ids = HashSet::new();
        let mut keys = HashSet::new();
        let mut columns = Vec::new();
        for item in schema {
            let id = identity(item, "id")?;
            let key = identity(item, "key")?;
            if !ids.insert(id.clone()) || !keys.insert(key.clone()) {
                return Err(ListError("List columns have ambiguous identities."));
            }
            columns.push(Column {
                id,
                key,
                name: identity(item, "name")?,
                kind: identity(item, "type")?,
                raw: item.clone(),
            });
        }
        let views = match raw.pointer("/list_metadata/views") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(views)) => views.clone(),
            _ => return Err(ListError("List views are malformed.")),
        };
        let mut view_ids = HashSet::new();
        for view in &views {
            if !view_ids.insert(identity(view, "id")?) {
                return Err(ListError("List views have ambiguous identities."));
            }
        }
        let title = raw
            .get("title")
            .and_then(Value::as_str)
            .or_else(|| raw.get("name").and_then(Value::as_str))
            .unwrap_or("List")
            .to_owned();
        Ok(Self {
            id,
            title,
            columns,
            views,
            raw,
        })
    }
    pub fn editable(&self, allow_lists: Option<&str>) -> bool {
        self.read_only_reason(allow_lists).is_none()
    }
    pub fn read_only_reason(&self, allow_lists: Option<&str>) -> Option<&'static str> {
        if allow_lists != Some("full") {
            return Some("List editing is unavailable for this workspace.");
        }
        if !matches!(
            self.raw.get("access").and_then(Value::as_str),
            Some("write" | "owner")
        ) {
            return Some("You have read-only access to this List.");
        }
        if self
            .raw
            .get("is_locked")
            .is_some_and(|v| v != &Value::Bool(false) && !v.is_null())
        {
            return Some("This List is locked.");
        }
        if unavailable(&self.raw) {
            return Some("This List is unavailable.");
        }
        None
    }
    pub fn column(&self, id: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.id == id)
    }
    pub fn primary_column(&self) -> Option<&Column> {
        self.columns
            .iter()
            .find(|c| c.raw["is_primary_column"] == true)
            .or_else(|| {
                self.columns.iter().find(|c| {
                    matches!(c.kind.as_str(), "text" | "rich_text")
                        && c.key.eq_ignore_ascii_case("name")
                })
            })
            .or_else(|| {
                self.columns
                    .iter()
                    .find(|c| matches!(c.kind.as_str(), "text" | "rich_text"))
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub id: String,
    pub raw: Value,
    /// Entire typed field object, keyed by its canonical schema key.
    pub fields: BTreeMap<String, Value>,
}
impl Record {
    pub fn parse(raw: Value) -> Result<Self, ListError> {
        let id = identity(&raw, "id")?;
        let input = raw
            .get("fields")
            .and_then(Value::as_array)
            .ok_or(ListError("List record fields are missing or malformed."))?;
        let mut fields = BTreeMap::new();
        for field in input {
            let key = identity(field, "key")?;
            if fields.insert(key, field.clone()).is_some() {
                return Err(ListError("List record fields have ambiguous identities."));
            }
        }
        for key in ["position", "date_created"] {
            if raw
                .get(key)
                .is_some_and(|v| !v.is_null() && !v.is_string() && !v.is_number())
            {
                return Err(ListError("List record ordering is malformed."));
            }
        }
        if let Some(positions) = raw.get("view_positions").filter(|v| !v.is_null()) {
            let positions = positions
                .as_object()
                .ok_or(ListError("List view positions are malformed."))?;
            let mut keys = HashSet::new();
            for (key, value) in positions {
                if !keys.insert(key.to_lowercase())
                    || (!value.is_string() && !value.is_number() && !value.is_null())
                {
                    return Err(ListError("List view positions are ambiguous or malformed."));
                }
            }
        }
        Ok(Self { id, raw, fields })
    }
    pub fn fallback(&self, key: &str) -> Option<&str> {
        self.fields.get(key)?.get("text")?.as_str()
    }
    pub fn parent_id(&self) -> Option<&str> {
        self.raw
            .get("parent_record_id")?
            .as_str()
            .filter(|id| !id.is_empty())
    }
    pub fn archived(&self) -> bool {
        self.raw["archived"] == true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordsPage {
    pub records: Vec<Record>,
    /// Nonempty cursor means incomplete. No continuation request is inferred.
    pub next_cursor: Option<String>,
    pub raw: Value,
}
impl RecordsPage {
    pub fn parse(raw: Value) -> Result<Self, ListError> {
        if raw["ok"] != true {
            return Err(ListError("List records response was not successful."));
        }
        let rows = raw
            .get("records")
            .and_then(Value::as_array)
            .ok_or(ListError("List records are missing or malformed."))?;
        let mut ids = HashSet::new();
        let mut records = Vec::new();
        for row in rows {
            let row = Record::parse(row.clone())?;
            if !ids.insert(row.id.clone()) {
                return Err(ListError("List records have ambiguous identities."));
            }
            records.push(row);
        }
        let next_cursor = match raw.get("response_metadata") {
            None | Some(Value::Null) => None,
            Some(Value::Object(metadata)) => match metadata.get("next_cursor") {
                None | Some(Value::Null) => None,
                Some(Value::String(cursor)) => {
                    if cursor.is_empty() {
                        None
                    } else {
                        Some(cursor.clone())
                    }
                }
                _ => return Err(ListError("List continuation metadata is malformed.")),
            },
            _ => return Err(ListError("List continuation metadata is malformed.")),
        };
        Ok(Self {
            records,
            next_cursor,
            raw,
        })
    }
    pub fn complete(&self) -> bool {
        self.next_cursor.is_none()
    }
    pub fn record(&self, id: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.id == id)
    }
    pub fn ordered(&self, view_id: Option<&str>) -> Vec<&Record> {
        let mut records: Vec<_> = self.records.iter().collect();
        records.sort_by(|a, b| ordering::compare(a, b, view_id));
        records
    }
}

#[cfg(test)]
mod tests;

pub use view_sort::{ViewSort, ViewSortAttempt, ViewSortChoice, ViewSortPermit, ViewSortPhase};

mod board_choice;
pub use board_choice::{
    BoardChoiceAction, BoardChoiceAttempt, BoardChoiceChange, BoardChoicePermit, BoardChoicePhase,
};

mod board_group_delete;
pub use board_group_delete::{
    BoardGroupDelete, BoardGroupDeleteAttempt, BoardGroupDeleteCellsPermit, BoardGroupDeletePhase,
    BoardGroupDeleteSchemaPermit,
};

mod view_refinement_save;
pub use view_refinement_save::{
    ViewRefinementDestination, ViewRefinementSave, ViewRefinementSaveAttempt,
    ViewRefinementSavePermit, ViewRefinementSavePhase,
};

mod default_view_preference;
pub use default_view_preference::{
    ListDefaultViewPreference, ListDefaultViewPreferenceAttempt, ListDefaultViewPreferencePermit,
};

pub use view_refinement_save::ViewRefinementAcknowledgement;
