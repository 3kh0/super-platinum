//! Retained c8rH / iLfW / A/54: personal completion updates its own row and
//! optionally a referenced List row. These are distinct, non-atomic writes.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskCompletionReference {
    pub list_id: String,
    pub record_id: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TaskCompletion {
    primary: TaskCompletionCell,
    reference: Option<TaskCompletionReference>,
    reference_observation: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TaskCompletionCell {
    edit: CellEdit,
    personal: PersonalTaskLists,
    initial: bool,
}
fn id(value: Option<&Value>) -> Result<String, ListError> {
    value
        .and_then(Value::as_str)
        .filter(|v| {
            !v.is_empty() && v.len() <= 1024 && v.trim() == *v && !v.chars().any(char::is_control)
        })
        .map(str::to_owned)
        .ok_or(ListError("The referenced To-do identity is malformed."))
}
/// Source reads the first Reference column, and the first reference[] value.
/// Other reference kinds do not invoke a second List-cell mutation.
fn reference(
    document: &Document,
    record: &Record,
) -> Result<Option<TaskCompletionReference>, ListError> {
    let Some(column) = document.columns.iter().find(|c| c.kind == "reference") else {
        return Ok(None);
    };
    value::identity(column, record)?;
    let Some(field) = record.fields.get(&column.key) else {
        return Ok(None);
    };
    let Some(raw) = field.get("reference") else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let values = raw
        .as_array()
        .ok_or(ListError("This To-do reference has an unsupported shape."))?;
    let Some(first) = values.first() else {
        return Ok(None);
    };
    let object = first
        .as_object()
        .ok_or(ListError("This To-do reference is malformed."))?;
    let Some(target) = object.get("list_record") else {
        return Ok(None);
    };
    if object.len() != 1 || !target.is_object() {
        return Err(ListError("This To-do has an ambiguous List reference."));
    }
    Ok(Some(TaskCompletionReference {
        list_id: id(target.get("list_id"))?,
        record_id: id(target.get("row_id"))?,
    }))
}
fn reference_observation(document: &Document, record: &Record) -> Value {
    document
        .columns
        .iter()
        .find(|c| c.kind == "reference")
        .map(|column| json!({"column": column.raw, "field": record.fields.get(&column.key)}))
        .unwrap_or(Value::Null)
}
pub(super) fn value(column: &Column, record: &Record) -> Result<bool, ListError> {
    if column.kind != "todo_completed" || column.key != "todo_completed" {
        return Err(ListError(
            "This field is not the personal To-do completion field.",
        ));
    }
    value::identity(column, record)?;
    let Some(field) = record.fields.get(&column.key) else {
        return Ok(false);
    };
    match field.get("checkbox") {
        None if field.as_object().is_some_and(|o| {
            o.keys()
                .all(|k| matches!(k.as_str(), "key" | "column_id" | "row_id" | "text"))
        }) =>
        {
            Ok(false)
        }
        Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(ListError("This To-do completion value is malformed.")),
    }
}
impl Column {
    pub fn personal_completion_value(
        &self,
        record: &Record,
        list_id: &str,
        personal: &PersonalTaskLists,
    ) -> Result<bool, ListError> {
        if !personal.contains(list_id) {
            return Err(ListError(
                "This is not an authenticated personal To-do List.",
            ));
        }
        value(self, record)
    }
}
impl TaskCompletionCell {
    fn capture(
        document: &Document,
        page: &RecordsPage,
        row: &str,
        allow: Option<&str>,
        personal: &PersonalTaskLists,
        checked: bool,
    ) -> Result<Self, ListError> {
        if let Some(reason) = document.read_only_reason(allow) {
            return Err(ListError(reason));
        }
        let mut columns = document
            .columns
            .iter()
            .filter(|c| c.key == "todo_completed");
        let column = columns
            .next()
            .ok_or(ListError("This List has no To-do completion field."))?;
        if columns.next().is_some() {
            return Err(ListError("This completion field is ambiguous."));
        }
        let record = page
            .record(row)
            .ok_or(ListError("This To-do item is unavailable."))?;
        if record
            .raw
            .get("list_id")
            .is_some_and(|v| v.as_str() != Some(&document.id))
        {
            return Err(ListError("This To-do item belongs to another List."));
        }
        let initial = value(column, record)?;
        Ok(Self {
            initial,
            personal: personal.clone(),
            edit: CellEdit {
                inner: Edit::Typed {
                    list_id: document.id.clone(),
                    record_id: record.id.clone(),
                    column_id: column.id.clone(),
                    column_key: column.key.clone(),
                    document: document.raw.clone(),
                    record: record.raw.clone(),
                    column: column.raw.clone(),
                    value: CellEditValue::Boolean(checked),
                    task_policy: Some(personal.clone()),
                },
            },
        })
    }
    pub fn edit(&self) -> &CellEdit {
        &self.edit
    }
    pub fn needs_write(&self) -> bool {
        self.edit.value() != CellEditValue::Boolean(self.initial)
    }
    pub fn revalidate(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow: Option<&str>,
        personal: &PersonalTaskLists,
    ) -> Result<(), ListError> {
        if &self.personal != personal {
            return Err(ListError("Personal To-do preferences changed."));
        }
        self.edit.revalidate(document, page, allow, Some(personal))
    }
    pub fn readback_value(&self, document: &Document, page: &RecordsPage) -> Option<bool> {
        let Edit::Typed {
            list_id,
            record_id,
            column_id,
            column: old_column,
            record: old_record,
            ..
        } = &self.edit.inner
        else {
            return None;
        };
        if &document.id != list_id || super::super::unavailable(&document.raw) {
            return None;
        }
        let column = document
            .column(column_id)
            .filter(|c| c.raw == *old_column)?;
        let record = page.record(record_id)?;
        if record
            .raw
            .get("list_id")
            .is_some_and(|id| id.as_str() != Some(list_id.as_str()))
        {
            return None;
        }
        if ["date_created", "created_by"]
            .iter()
            .any(|key| record.raw.get(key) != old_record.get(key))
        {
            return None;
        }
        value(column, record).ok()
    }
    pub fn readback_matches(&self, document: &Document, page: &RecordsPage) -> bool {
        self.readback_value(document, page)
            .map(CellEditValue::Boolean)
            == Some(self.edit.value())
    }
}
impl TaskCompletion {
    pub fn capture(
        document: &Document,
        page: &RecordsPage,
        row: &str,
        column_id: &str,
        allow: Option<&str>,
        personal: &PersonalTaskLists,
        checked: bool,
    ) -> Result<Self, ListError> {
        if !personal.contains(&document.id) {
            return Err(ListError(
                "Personal To-do preferences do not identify this List.",
            ));
        }
        let primary = TaskCompletionCell::capture(document, page, row, allow, personal, checked)?;
        if primary.edit.column_id() != column_id {
            return Err(ListError(
                "The completion control does not identify this field.",
            ));
        }
        let record = page.record(row).unwrap();
        if value(document.column(column_id).unwrap(), record)? == checked {
            return Err(ListError("This To-do is already set to that state."));
        }
        let reference =
            reference(document, record)?.filter(|r| r.list_id != document.id || r.record_id != row);
        Ok(Self {
            primary,
            reference,
            reference_observation: reference_observation(document, record),
        })
    }
    pub fn primary(&self) -> &TaskCompletionCell {
        &self.primary
    }
    pub fn reference(&self) -> Option<&TaskCompletionReference> {
        self.reference.as_ref()
    }
    /// After observing the primary write, a host may continue only toward the
    /// originally captured reference. This permits the primary completion and
    /// server update timestamp to change, but never rebases a replaced source.
    pub fn reference_still_current(&self, document: &Document, page: &RecordsPage) -> bool {
        let Edit::Typed {
            document: original,
            record_id,
            ..
        } = &self.primary.edit.inner
        else {
            return false;
        };
        document.raw == *original
            && self.primary.readback_matches(document, page)
            && page.record(record_id).is_some_and(|record| {
                reference_observation(document, record) == self.reference_observation
            })
    }
    /// Capture the referenced write from its own complete fresh document/page.
    /// It is never inferred from the personal row or authorized by that List.
    pub fn capture_reference(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow: Option<&str>,
        personal: &PersonalTaskLists,
    ) -> Result<TaskCompletionCell, ListError> {
        let reference = self
            .reference
            .as_ref()
            .ok_or(ListError("This To-do has no referenced List write."))?;
        if document.id != reference.list_id || &self.primary.personal != personal {
            return Err(ListError(
                "The referenced List or To-do preferences changed.",
            ));
        }
        let CellEditValue::Boolean(checked) = self.primary.edit.value() else {
            unreachable!()
        };
        TaskCompletionCell::capture(
            document,
            page,
            &reference.record_id,
            allow,
            personal,
            checked,
        )
    }
}
#[cfg(test)]
mod tests;
