//! Ordinary List item creation and duplication. The server owns the new identity.
use super::{Document, ListError, PersonalTaskLists, Record, RecordsPage};
use serde_json::{Value, json};

mod clock;
mod duplication;
pub use clock::{ListClock, ListTimestamp};
#[cfg(test)]
mod tests;

/// One explicit request to create or duplicate an item, with the captured completed
/// checkbox seed only when selected from the Completed view. Never replay this
/// intent after dispatch: a missing response cannot identify the created row.
#[derive(Clone, Debug, PartialEq)]
pub struct CreationIntent {
    document: Document,
    selected_view: Option<String>,
    initial_fields: Option<Value>,
    duplicate: Option<duplication::DuplicateSource>,
}

impl CreationIntent {
    pub fn capture(
        document: &Document,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: Option<&str>,
    ) -> Result<Self, ListError> {
        Self::eligible(document, allow_lists, personal_tasks, selected_view)?;
        let initial_fields = selected_view
            .and_then(|id| document.views.iter().find(|view| view["id"] == id))
            .filter(|view| view["default_view_key"] == "completed_items")
            .and_then(|_| {
                document
                    .columns
                    .iter()
                    .find(|column| column.kind == "todo_completed")
            })
            .map(|column| json!([{"column_id":column.id,"checkbox":true}]));
        Ok(Self {
            document: document.clone(),
            selected_view: selected_view.map(str::to_owned),
            initial_fields,
            duplicate: None,
        })
    }

    pub fn eligible(
        document: &Document,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: Option<&str>,
    ) -> Result<(), ListError> {
        if let Some(reason) = document.read_only_reason(allow_lists) {
            return Err(ListError(reason));
        }
        if personal_tasks.is_none_or(|policy| policy.contains(&document.id)) {
            return Err(ListError(
                "Ordinary List creation preferences are not verified.",
            ));
        }
        // Slack asks for explicit permission to archive old items at this limit.
        // This ordinary creation path never grants that unrelated permission.
        match document.raw.pointer("/list_limits/over_row_maximum") {
            Some(Value::Bool(false)) => {}
            Some(Value::Bool(true)) => {
                return Err(ListError("This List has reached its item limit."));
            }
            _ => return Err(ListError("This List's item limit has not been verified.")),
        }
        if document.columns.is_empty() {
            return Err(ListError("This List's schema is unavailable."));
        }
        if let Some(id) = selected_view {
            let view = document
                .views
                .iter()
                .find(|view| view["id"] == id)
                .ok_or(ListError("The selected List view is unavailable."))?;
            if view
                .get("default_view_key")
                .is_some_and(|key| !key.is_null() && !key.is_string())
            {
                return Err(ListError("The selected List view is malformed."));
            }
        }
        Ok(())
    }

    pub fn initial_fields(&self) -> Option<&Value> {
        self.initial_fields.as_ref()
    }

    pub fn list_id(&self) -> &str {
        &self.document.id
    }

    pub fn position(&self) -> &str {
        self.duplicate
            .as_ref()
            .map_or("", |source| source.position.as_str())
    }

    pub fn source_record_id(&self) -> Option<&str> {
        self.duplicate
            .as_ref()
            .map(|source| source.record.id.as_str())
    }

    pub fn revalidate(
        &self,
        document: &Document,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: Option<&str>,
    ) -> Result<(), ListError> {
        if self.duplicate.is_some() {
            return Err(ListError(
                "Refresh the List items before duplicating this item.",
            ));
        }
        self.revalidate_document(document, allow_lists, personal_tasks, selected_view)
    }

    fn revalidate_document(
        &self,
        document: &Document,
        allow_lists: Option<&str>,
        personal_tasks: Option<&PersonalTaskLists>,
        selected_view: Option<&str>,
    ) -> Result<(), ListError> {
        Self::eligible(document, allow_lists, personal_tasks, selected_view)?;
        if selected_view != self.selected_view.as_deref()
            || document.id != self.document.id
            || !super::cell_edit::document_matches(&self.document.raw, &document.raw)
        {
            return Err(ListError(
                "This List changed. Review it before creating an item.",
            ));
        }
        Ok(())
    }

    /// Captured t0Mq reads response.record.id then decodes response.record.
    /// A bare `ok` is deliberately insufficient. No guessed local row ID is used.
    pub fn validate_response(&self, response: &Value) -> Result<Record, ListError> {
        if response.get("ok") != Some(&Value::Bool(true))
            || response
                .get("list_id")
                .is_some_and(|id| id.as_str() != Some(self.list_id()))
            || response.get("error").is_some_and(|error| !error.is_null())
        {
            return Err(ListError("Slack has not confirmed the new List item."));
        }
        let record = Record::parse(
            response
                .get("record")
                .cloned()
                .ok_or(ListError("Slack did not return the new List item."))?,
        )?;
        self.validate_record(&self.document, &record)?;
        self.validate_initial_fields(&record)?;
        Ok(record)
    }

    fn validate_record(&self, document: &Document, record: &Record) -> Result<(), ListError> {
        if self
            .duplicate
            .as_ref()
            .is_some_and(|source| source.existing_ids.contains(&record.id))
            || record.id.trim() != record.id
            || record
                .raw
                .get("list_id")
                .is_some_and(|id| id.as_str() != Some(self.list_id()))
            || ["archived", "is_deleted"].iter().any(|key| {
                record
                    .raw
                    .get(key)
                    .is_some_and(|value| !value.is_null() && value != &Value::Bool(false))
            })
            || record.raw.get("parent_record_id").is_some_and(|parent| {
                !parent.is_null()
                    && match parent.as_str() {
                        Some("") => false,
                        Some(id) => self.duplicate.is_none() || id.trim() != id || id == record.id,
                        None => true,
                    }
            })
        {
            return Err(ListError("The new List item's identity was not confirmed."));
        }
        for (key, field) in &record.fields {
            let column = document
                .columns
                .iter()
                .find(|column| column.key == *key)
                .ok_or(ListError(
                    "The new List item's fields did not match its schema.",
                ))?;
            if field
                .get("column_id")
                .is_some_and(|id| id.as_str() != Some(&column.id))
                || field
                    .get("row_id")
                    .is_some_and(|id| id.as_str() != Some(&record.id))
                || field.get("error").is_some_and(|error| !error.is_null())
            {
                return Err(ListError(
                    "The new List item's field identity was not confirmed.",
                ));
            }
        }
        Ok(())
    }

    fn validate_initial_fields(&self, record: &Record) -> Result<(), ListError> {
        if let Some(initial_fields) = self.initial_fields.as_ref().and_then(Value::as_array) {
            for initial in initial_fields {
                let column = self
                    .document
                    .columns
                    .iter()
                    .find(|column| initial["column_id"] == column.id)
                    .ok_or(ListError(
                        "The new List item's initial field is unavailable.",
                    ))?;
                if record
                    .fields
                    .get(&column.key)
                    .and_then(|field| field.get("checkbox"))
                    != initial.get("checkbox")
                {
                    return Err(ListError(
                        "Slack has not confirmed the new item's completed state.",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Readback uses only the server receipt ID, never a matching title or new-row
    /// diff. Concurrent users may independently create indistinguishable blanks,
    /// or change cells/schema after the receipt. The receipt uses its original
    /// schema and initial values; current fields use the current verified schema.
    pub fn readback_record(
        &self,
        document: &Document,
        page: &RecordsPage,
        receipt: &Record,
    ) -> Option<Record> {
        if document.id != self.document.id || super::unavailable(&document.raw) {
            return None;
        }
        self.validate_record(&self.document, receipt).ok()?;
        self.validate_initial_fields(receipt).ok()?;
        let record = page.record(&receipt.id)?;
        self.validate_record(document, record).ok()?;
        if ["date_created", "created_by"].iter().any(|key| {
            receipt
                .raw
                .get(key)
                .is_some_and(|expected| record.raw.get(key) != Some(expected))
        }) {
            return None;
        }
        Some(record.clone())
    }
}
