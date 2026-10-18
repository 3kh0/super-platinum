//! HcpI / WUPY / e6np: reminders belong to saved.list_record, not a date cell.
//! Missing record.saved is an observed absence in a complete individual record;
//! failing to find an item in a partial saved.list page is never that evidence.
use super::{Document, ListError, Record, RecordsPage};
use crate::later::SavedItem;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListReminderChange {
    Set(i64),
    Clear,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ListReminderEdit {
    document: Document,
    record: Record,
    method: &'static str,
    due: Option<i64>,
    mark: Option<&'static str>,
    change: ListReminderChange,
    saved_observation: Option<Value>,
}
fn available(document: &Document, page: &RecordsPage, row: &str) -> Result<Record, ListError> {
    if super::unavailable(&document.raw) {
        return Err(ListError("This List is unavailable."));
    }
    let record = page
        .record(row)
        .ok_or(ListError("This List item is unavailable."))?;
    if ["archived", "is_deleted"].iter().any(|key| {
        record
            .raw
            .get(key)
            .is_some_and(|v| !v.is_null() && *v != Value::Bool(false))
    }) || record
        .raw
        .get("list_id")
        .is_some_and(|v| v.as_str() != Some(&document.id))
    {
        return Err(ListError("This List item is unavailable."));
    }
    Ok(record.clone())
}
fn date(raw: &Value, key: &str) -> Result<i64, ListError> {
    match raw.get(key) {
        None => Ok(0),
        Some(v) => v
            .as_i64()
            .filter(|v| *v >= 0)
            .ok_or(ListError("The saved reminder is malformed.")),
    }
}
fn saved(raw: &Value) -> Result<Option<&Value>, ListError> {
    let Some(value) = raw.get("saved") else {
        return Ok(None);
    };
    if !value.is_object()
        || value.get("is_archived").and_then(Value::as_bool).is_none()
        || !matches!(
            value.get("state").and_then(Value::as_str),
            Some("in_progress" | "completed" | "archived")
        )
        || value
            .get("todo_state")
            .is_some_and(|v| !matches!(v.as_str(), Some("to_do" | "saved" | "completed")))
    {
        return Err(ListError("The saved reminder is not a known shape."));
    }
    date(value, "date_due")?;
    date(value, "date_completed")?;
    Ok(Some(value))
}
impl ListReminderEdit {
    /// `enabled` is the current authenticated lists_in_later assignment. A
    /// record reminder does not require permission to edit the shared schema.
    /// A present record marker must have a matching full saved item: date_created
    /// for the To-do sentinel is absent from the compact record marker.
    pub fn capture(
        document: &Document,
        page: &RecordsPage,
        row: &str,
        enabled: bool,
        item: Option<&SavedItem>,
        change: ListReminderChange,
    ) -> Result<Self, ListError> {
        if !enabled {
            return Err(ListError(
                "List reminders are not enabled for this account.",
            ));
        }
        let record = available(document, page, row)?;
        let compact = saved(&record.raw)?;
        match (compact, item) {
            (None, None) => {}
            (Some(raw), Some(item)) => {
                if item.item_type != "list_record"
                    || item.item_id != document.id
                    || item.ts != row
                    || item.is_archived != raw["is_archived"].as_bool().unwrap()
                    || item.state != raw["state"].as_str().unwrap()
                    || item.date_due != date(raw, "date_due")?
                    || item.date_completed != date(raw, "date_completed")?
                    || raw
                        .get("todo_state")
                        .is_some_and(|v| v.as_str() != Some(&item.todo_state))
                {
                    return Err(ListError(
                        "The saved item and List record disagree. Refresh first.",
                    ));
                }
            }
            _ => {
                return Err(ListError(
                    "The saved item has not been authoritatively observed.",
                ));
            }
        }
        let (method, due, mark) = match change {
            ListReminderChange::Set(due) if due > 0 && due <= 253_402_300_799 => {
                let mark = item.and_then(|item| {
                    if item.date_completed > 0 {
                        Some("uncompleted")
                    } else if item.is_archived {
                        Some("unarchived")
                    } else {
                        None
                    }
                });
                if item.is_some_and(|item| item.date_due == due) && mark.is_none() {
                    return Err(ListError("This reminder already has that time."));
                }
                (
                    if item.is_some() {
                        "saved.update"
                    } else {
                        "saved.add"
                    },
                    Some(due),
                    mark,
                )
            }
            ListReminderChange::Set(_) => return Err(ListError("Choose a valid reminder time.")),
            ListReminderChange::Clear => {
                let item = item.ok_or(ListError("This item has no reminder to clear."))?;
                if item.todo_state == "to_do" {
                    if item.date_created <= 0 || item.date_due == item.date_created {
                        return Err(ListError("This To-do has no confirmed reminder to clear."));
                    }
                    ("saved.update", Some(item.date_created), None)
                } else {
                    ("saved.delete", None, None)
                }
            }
        };
        Ok(Self {
            document: document.clone(),
            record,
            method,
            due,
            mark,
            change,
            saved_observation: item
                .map(serde_json::to_value)
                .transpose()
                .map_err(|_| ListError("The saved item could not be captured."))?,
        })
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn record_id(&self) -> &str {
        &self.record.id
    }
    pub fn method(&self) -> &'static str {
        self.method
    }
    pub fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("item_type", "list_record".into()),
            ("item_id", self.list_id().into()),
            ("ts", self.record_id().into()),
        ];
        if let Some(due) = self.due {
            fields.push(("date_due", due.to_string()));
        }
        if let Some(mark) = self.mark {
            fields.push(("mark", mark.into()));
        }
        fields.push((
            "_x_reason",
            match self.method {
                "saved.add" => "saved-api/saveListRecord",
                "saved.update" => "saved-api/updateSavedListRecord",
                _ => "saved-api/removeSavedListRecord",
            }
            .into(),
        ));
        fields
    }
    pub fn revalidate(
        &self,
        document: &Document,
        page: &RecordsPage,
        enabled: bool,
        item: Option<&SavedItem>,
    ) -> Result<(), ListError> {
        let current = Self::capture(document, page, self.record_id(), enabled, item, self.change)?;
        if !super::cell_edit::document_matches(&self.document.raw, &current.document.raw)
            || self.record.raw != current.record.raw
            || self.method != current.method
            || self.due != current.due
            || self.mark != current.mark
            || self.saved_observation != current.saved_observation
        {
            return Err(ListError(
                "This reminder or List item changed. Reopen it before saving.",
            ));
        }
        Ok(())
    }
    /// No optimistic replacement of rows, fields, saved views, or cursors.
    /// A bare mutation acknowledgement does not confirm the saved-item result.
    pub fn readback_matches(&self, document: &Document, page: &RecordsPage) -> bool {
        if document.id != self.document.id {
            return false;
        }
        let Ok(record) = available(document, page, self.record_id()) else {
            return false;
        };
        if ["date_created", "created_by"]
            .iter()
            .any(|key| record.raw.get(key) != self.record.raw.get(key))
        {
            return false;
        }
        let Ok(compact) = saved(&record.raw) else {
            return false;
        };
        if self.method == "saved.delete" {
            return compact.is_none();
        }
        compact.is_some_and(|value| {
            date(value, "date_due").ok() == self.due
                && (self.mark != Some("unarchived") || value["is_archived"] == false)
                && (self.mark != Some("uncompleted")
                    || date(value, "date_completed").ok() == Some(0))
        })
    }
}
#[cfg(test)]
mod tests;
