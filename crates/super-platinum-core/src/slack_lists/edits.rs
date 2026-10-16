use super::{CellValue, Document, ListError, PersonalTaskLists, RecordsPage};
use serde_json::{Value, json};

/// A captured existing ordinary checkbox, never a row/column creation request.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckboxEdit {
    list_id: String,
    record_id: String,
    column_id: String,
    column_key: String,
    checked: bool,
    document: Value,
    record: Value,
    column: Value,
}
impl CheckboxEdit {
    pub fn capture(
        document: &Document,
        page: &RecordsPage,
        record_id: &str,
        column_id: &str,
        allow_lists: Option<&str>,
        checked: bool,
    ) -> Result<Self, ListError> {
        Self::capture_with_policy(
            document,
            page,
            record_id,
            column_id,
            allow_lists,
            None,
            checked,
        )
    }
    pub fn capture_with_policy(
        document: &Document,
        page: &RecordsPage,
        record_id: &str,
        column_id: &str,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        checked: bool,
    ) -> Result<Self, ListError> {
        if let Some(reason) = document.read_only_reason(allow_lists) {
            return Err(ListError(reason));
        }
        let column = document
            .column(column_id)
            .ok_or(ListError("This List column is no longer available."))?;
        let record = page
            .record(record_id)
            .ok_or(ListError("This List item is no longer available."))?;
        // Existing completion semantics remain; the only new default is a
        // genuinely missing ordinary no-option Checkbox's first explicit true.
        let initial = column
            .checkbox_input_value_with_policy(record, &document.id, personal_tasks)
            .ok_or(ListError("This field is not an editable checkbox."))?;
        if initial == checked {
            return Err(ListError("This checkbox is already set to that value."));
        }
        Ok(Self {
            list_id: document.id.clone(),
            record_id: record.id.clone(),
            column_id: column.id.clone(),
            column_key: column.key.clone(),
            checked,
            document: document.raw.clone(),
            record: record.raw.clone(),
            column: column.raw.clone(),
        })
    }
    pub fn list_id(&self) -> &str {
        &self.list_id
    }
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    pub fn column_id(&self) -> &str {
        &self.column_id
    }
    pub fn column_key(&self) -> &str {
        &self.column_key
    }
    pub fn checked(&self) -> bool {
        self.checked
    }
    pub fn cells(&self) -> Value {
        json!([{"row_id":self.record_id,"column_id":self.column_id,"checkbox":self.checked}])
    }
    pub fn revalidate(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow_lists: Option<&str>,
    ) -> Result<(), ListError> {
        self.revalidate_with_policy(document, page, allow_lists, None)
    }
    pub fn revalidate_with_policy(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if let Some(reason) = document.read_only_reason(allow_lists) {
            return Err(ListError(reason));
        }
        if document.id != self.list_id
            || !super::cell_edit::document_matches(&document.raw, &self.document)
            || page.record(&self.record_id).map(|record| &record.raw) != Some(&self.record)
            || document.column(&self.column_id).map(|column| &column.raw) != Some(&self.column)
        {
            return Err(ListError(
                "This List changed. Review it before editing again.",
            ));
        }
        if !document.column(&self.column_id).is_some_and(|column| {
            super::policy::ordinary_boolean_kind(&column.kind, &document.id, personal_tasks)
        }) {
            return Err(ListError(
                "Ordinary List completion permissions are not verified.",
            ));
        }
        Ok(())
    }
    /// `ok:true` alone is not proof the requested checkbox was changed.
    /// A malformed/partial response must enter the caller's uncertain/readback path.
    pub fn validate_response(&self, response: &Value) -> Result<(), ListError> {
        let valid = response["ok"] == true
            && response
                .get("cells")
                .and_then(Value::as_array)
                .is_some_and(|cells| {
                    cells.len() == 1
                        && cells[0]["row_id"] == self.record_id
                        && cells[0]["column_id"] == self.column_id
                        && cells[0]["checkbox"] == self.checked
                        && cells[0]
                            .get("key")
                            .is_none_or(|key| key.as_str() == Some(&self.column_key))
                        && cells[0].get("error").is_none_or(Value::is_null)
                });
        if valid {
            Ok(())
        } else {
            Err(ListError(
                "Slack has not confirmed this checkbox change. Refresh the List before retrying.",
            ))
        }
    }
    /// Readback may advance the record timestamp; schema identity must still match.
    pub fn readback_matches(&self, document: &Document, page: &RecordsPage) -> bool {
        self.readback_value(document, page) == Some(self.checked)
    }
    /// A concrete different readback can be shown for a new explicit user edit;
    /// it must never trigger an automatic resend of the captured operation.
    pub fn readback_value(&self, document: &Document, page: &RecordsPage) -> Option<bool> {
        if document.id != self.list_id || super::unavailable(&document.raw) {
            return None;
        }
        let column = document
            .column(&self.column_id)
            .filter(|c| c.key == self.column_key && c.raw == self.column)?;
        let record = page.record(&self.record_id)?;
        let field = record.fields.get(&self.column_key)?;
        if ["date_created", "created_by"]
            .iter()
            .any(|key| record.raw.get(key) != self.record.get(key))
            || record
                .raw
                .get("archived")
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
            || record
                .raw
                .get("is_deleted")
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
            || field
                .get("column_id")
                .is_some_and(|id| id.as_str() != Some(&self.column_id))
            || field
                .get("row_id")
                .is_some_and(|id| id.as_str() != Some(&self.record_id))
        {
            return None;
        }
        match column.cell(record) {
            CellValue::Boolean(value) => Some(value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
