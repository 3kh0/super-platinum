//! Existing-row typed cell mutations; no row, schema, or select-option creation.
use super::{CheckboxEdit, Column, Document, ListError, PersonalTaskLists, Record, RecordsPage};
use serde_json::{Number, Value, json};

#[cfg(test)]
mod channel_tests;
mod comparison;
mod link;
pub use link::LinkValue;
#[cfg(test)]
mod date_tests;
#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod user_tests;
mod value;

#[derive(Clone, Debug, PartialEq)]
pub enum CellEditValue {
    /// Ordinary Link cell content; storage does not authorize opening its URL.
    Link(Option<LinkValue>),
    Boolean(bool),
    RichText(Vec<Value>),
    Number(Option<Number>),
    Select(Vec<String>),
    /// Ordinary User-column identities; personal To-do assignees use another protocol.
    Users(Vec<String>),
    /// Existing channel references; never joins, creates, or shares a channel.
    Channels(Vec<String>),
    /// An ordinary calendar date, never a Unix timestamp or the Later sentinel.
    Date(Option<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellEdit {
    inner: Edit,
}
#[derive(Clone, Debug, PartialEq)]
enum Edit {
    Checkbox(CheckboxEdit),
    Typed {
        list_id: String,
        record_id: String,
        column_id: String,
        column_key: String,
        document: Value,
        record: Value,
        column: Value,
        value: CellEditValue,
    },
}
impl From<CheckboxEdit> for CellEdit {
    fn from(edit: CheckboxEdit) -> Self {
        Self {
            inner: Edit::Checkbox(edit),
        }
    }
}

impl Column {
    /// Current Select columns store cardinality in options.format; older
    /// schemas may use the dedicated multi_select type (captured jHPH/FieldRenderer).
    pub fn is_multi_select(&self) -> bool {
        self.kind == "multi_select"
            || (self.kind == "select"
                && self.raw.pointer("/options/format").and_then(Value::as_str)
                    == Some("multi_select"))
    }
    /// Captured entity columns default to single_entity; only multi_entity
    /// enables multiple users. Eligibility separately rejects unknown formats.
    pub fn is_multi_user(&self) -> bool {
        self.kind == "user"
            && self.raw.pointer("/options/format").and_then(Value::as_str) == Some("multi_entity")
    }
    /// Channel reference columns use the same source-backed entity cardinality.
    pub fn is_multi_channel(&self) -> bool {
        self.kind == "channel"
            && self.raw.pointer("/options/format").and_then(Value::as_str) == Some("multi_entity")
    }
    /// Projection check without cloning the rich-text tree. Permissions on the
    /// document are separate; dispatch still captures and revalidates everything.
    pub fn editable_value(
        &self,
        record: &Record,
        list_id: &str,
        personal_tasks: Option<&PersonalTaskLists>,
    ) -> bool {
        value::eligible(self, record, list_id, personal_tasks).is_ok()
    }
    /// Exact editor seed. Opaque rich nodes remain present, never plain-text flattened.
    pub fn edit_value(
        &self,
        record: &Record,
        list_id: &str,
        personal_tasks: Option<&PersonalTaskLists>,
    ) -> Result<CellEditValue, ListError> {
        value::eligible(self, record, list_id, personal_tasks)?;
        value::read(self, record)
    }
}

impl CellEdit {
    pub fn capture(
        document: &Document,
        page: &RecordsPage,
        record_id: &str,
        column_id: &str,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        desired: CellEditValue,
    ) -> Result<Self, ListError> {
        if let CellEditValue::Boolean(checked) = desired {
            // The generic editor still requires a stored boolean. Only the
            // dedicated checkbox input admits the first toggle from absence.
            if !document
                .column(column_id)
                .zip(page.record(record_id))
                .is_some_and(|(column, record)| {
                    column.editable_checkbox_with_policy(record, &document.id, personal_tasks)
                })
            {
                return Err(ListError("This checkbox is not editable."));
            }
            return CheckboxEdit::capture_with_policy(
                document,
                page,
                record_id,
                column_id,
                allow_lists,
                personal_tasks,
                checked,
            )
            .map(Self::from);
        }
        if let Some(reason) = document.read_only_reason(allow_lists) {
            return Err(ListError(reason));
        }
        let column = document
            .column(column_id)
            .ok_or(ListError("This List column is no longer available."))?;
        let record = page
            .record(record_id)
            .ok_or(ListError("This List item is no longer available."))?;
        let initial = column.edit_value(record, &document.id, personal_tasks)?;
        value::validate(column, &desired, &initial)?;
        if desired == initial {
            return Err(ListError("This List field is already set to that value."));
        }
        Ok(Self {
            inner: Edit::Typed {
                list_id: document.id.clone(),
                record_id: record.id.clone(),
                column_id: column.id.clone(),
                column_key: column.key.clone(),
                document: document.raw.clone(),
                record: record.raw.clone(),
                column: column.raw.clone(),
                value: desired,
            },
        })
    }
    pub fn list_id(&self) -> &str {
        match &self.inner {
            Edit::Checkbox(e) => e.list_id(),
            Edit::Typed { list_id, .. } => list_id,
        }
    }
    pub fn record_id(&self) -> &str {
        match &self.inner {
            Edit::Checkbox(e) => e.record_id(),
            Edit::Typed { record_id, .. } => record_id,
        }
    }
    pub fn column_id(&self) -> &str {
        match &self.inner {
            Edit::Checkbox(e) => e.column_id(),
            Edit::Typed { column_id, .. } => column_id,
        }
    }
    pub fn column_key(&self) -> &str {
        match &self.inner {
            Edit::Checkbox(e) => e.column_key(),
            Edit::Typed { column_key, .. } => column_key,
        }
    }
    pub fn value(&self) -> CellEditValue {
        match &self.inner {
            Edit::Checkbox(e) => CellEditValue::Boolean(e.checked()),
            Edit::Typed { value, .. } => value.clone(),
        }
    }
    pub fn cells(&self) -> Value {
        match &self.inner {
            Edit::Checkbox(e) => e.cells(),
            Edit::Typed {
                record_id,
                column_id,
                value,
                ..
            } => {
                let (key, value) = value::wire(value);
                let mut cell = json!({"row_id":record_id,"column_id":column_id});
                cell[key] = value;
                Value::Array(vec![cell])
            }
        }
    }
    pub fn revalidate(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        match &self.inner {
            Edit::Checkbox(e) => {
                e.revalidate_with_policy(document, page, allow_lists, personal_tasks)
            }
            Edit::Typed {
                list_id,
                record_id,
                column_id,
                document: old_doc,
                record: old_record,
                column: old_column,
                ..
            } => {
                if let Some(reason) = document.read_only_reason(allow_lists) {
                    return Err(ListError(reason));
                }
                let column = document
                    .column(column_id)
                    .ok_or(ListError("This List column is no longer available."))?;
                let record = page
                    .record(record_id)
                    .ok_or(ListError("This List item is no longer available."))?;
                if document.id != *list_id
                    || !document_matches(&document.raw, old_doc)
                    || record.raw != *old_record
                    || column.raw != *old_column
                {
                    return Err(ListError(
                        "This List changed. Review it before editing again.",
                    ));
                }
                value::eligible(column, record, &document.id, personal_tasks)
            }
        }
    }
    /// A bare ok or partial cell receipt cannot prove the intended mutation.
    pub fn validate_response(&self, response: &Value) -> Result<(), ListError> {
        if let Edit::Checkbox(e) = &self.inner {
            return e.validate_response(response);
        }
        let cells = response.get("cells").and_then(Value::as_array);
        let valid = response.get("ok") == Some(&Value::Bool(true))
            && cells.is_some_and(|cells| {
                if cells.len() != 1 {
                    return false;
                }
                let cell = &cells[0];
                let requested = self.value();
                let (member, desired) = value::wire(&requested);
                cell.get("row_id").and_then(Value::as_str) == Some(self.record_id())
                    && cell.get("column_id").and_then(Value::as_str) == Some(self.column_id())
                    && cell
                        .get("key")
                        .is_none_or(|v| v.as_str() == Some(self.column_key()))
                    && cell.get("error").is_none_or(Value::is_null)
                    && value::receipt_valid(&requested, cell)
                    && cell
                        .get(member)
                        .is_some_and(|actual| comparison::wire_matches(member, &desired, actual))
            });
        if valid {
            Ok(())
        } else {
            Err(ListError(
                "Slack has not confirmed this field change. Refresh the List before retrying.",
            ))
        }
    }
    pub fn readback_matches(&self, document: &Document, page: &RecordsPage) -> bool {
        self.readback_value(document, page)
            .is_some_and(|value| comparison::value_matches(&value, &self.value()))
    }
    /// Concrete differing values unlock only a new explicit edit, never a replay.
    pub fn readback_value(&self, document: &Document, page: &RecordsPage) -> Option<CellEditValue> {
        match &self.inner {
            Edit::Checkbox(e) => e.readback_value(document, page).map(CellEditValue::Boolean),
            Edit::Typed {
                list_id,
                record_id,
                column_id,
                column_key,
                column: old_column,
                record: old_record,
                ..
            } => {
                if document.id != *list_id || super::unavailable(&document.raw) {
                    return None;
                }
                let column = document
                    .column(column_id)
                    .filter(|c| c.key == *column_key && c.raw == *old_column)?;
                let record = page.record(record_id)?;
                if ["date_created", "created_by"]
                    .iter()
                    .any(|key| record.raw.get(key) != old_record.get(key))
                {
                    return None;
                }
                value::identity(column, record).ok()?;
                value::read(column, record).ok()
            }
        }
    }
}

// Live file-viewing preflight on 2026-10-13 changed only top-level last_read.
// Reading a List does not change edit identity. Preserve exact comparison of
// every other attribute, including schema, permissions and unknown metadata.
pub(super) fn document_matches(left: &Value, right: &Value) -> bool {
    let (Some(left), Some(right)) = (left.as_object(), right.as_object()) else {
        return left == right;
    };
    let count = |object: &serde_json::Map<String, Value>| {
        object.len() - usize::from(object.contains_key("last_read"))
    };
    count(left) == count(right)
        && left
            .iter()
            .filter(|(key, _)| *key != "last_read")
            .all(|(key, value)| right.get(key) == Some(value))
}
