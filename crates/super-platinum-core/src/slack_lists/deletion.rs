//! Ordinary item deletion includes its associated subtasks. Loaded descendants
//! are a stale-intent guard, never proof of the complete cascade or its size.
use super::{Document, ListError, PersonalTaskLists, Record, RecordsPage};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

mod state;
pub use state::{AcceptedDeletionState, DeletionStateIntent};

#[cfg(test)]
mod tests;

/// Capture only after explicit confirmation of deleting the item and any
/// associated subtasks. The caller owns account, visit and pending-operation
/// guards; neither this intent nor the endpoint provides compare-and-swap.
#[derive(Debug)]
pub struct DeletionIntent {
    document: Document,
    record: Record,
    descendants: BTreeMap<String, Value>,
    complete: bool,
}

impl DeletionIntent {
    pub fn capture(
        document: &Document,
        page: &RecordsPage,
        record_id: &str,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        Self::eligible(document, page, record_id, allow_lists, personal)?;
        Ok(Self {
            document: document.clone(),
            record: page.record(record_id).unwrap().clone(),
            descendants: descendants(page, record_id)?,
            complete: page.complete(),
        })
    }

    pub fn eligible(
        document: &Document,
        page: &RecordsPage,
        record_id: &str,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if let Some(reason) = document.read_only_reason(allow_lists) {
            return Err(ListError(reason));
        }
        if personal.is_none_or(|policy| policy.contains(&document.id)) {
            return Err(ListError(
                "Ordinary List deletion preferences are not verified.",
            ));
        }
        let record = page
            .record(record_id)
            .ok_or(ListError("This List item is no longer available."))?;
        validate_record(record, &document.id)?;
        Ok(())
    }

    pub fn has_known_subtasks(&self) -> bool {
        !self.descendants.is_empty()
    }

    /// Captured Slack derives children from its decoded parent index. Native
    /// permits immediate deletion only with a complete page and no descendants;
    /// incomplete results retain confirmation. This does not establish absence
    /// of archived or server-only descendants outside that loaded index.
    pub fn can_skip_delete_confirmation(&self) -> bool {
        self.complete && !self.has_known_subtasks()
    }

    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn record_id(&self) -> &str {
        &self.record.id
    }

    pub fn revalidate(
        &self,
        document: &Document,
        page: &RecordsPage,
        allow_lists: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        Self::eligible(document, page, self.record_id(), allow_lists, personal)?;
        if document.id != self.document.id
            || !super::cell_edit::document_matches(&document.raw, &self.document.raw)
            || page.record(self.record_id()).map(|record| &record.raw) != Some(&self.record.raw)
            || page.complete() != self.complete
            || descendants(page, self.record_id())? != self.descendants
        {
            return Err(ListError(
                "This List item changed. Review it before deleting.",
            ));
        }
        Ok(())
    }

    /// The captured delete thunk consumes ordinary API success, no record body.
    /// This receipt confirms the request; canonical absence uses records.info.
    pub fn validate_response(&self, response: &Value) -> Result<AcceptedDeletion, ListError> {
        if response["ok"] != true
            || response.get("error").is_some_and(|value| !value.is_null())
            || conflicts(response, "list_id", self.list_id())
            || conflicts(response, "id", self.record_id())
        {
            return Err(ListError("Slack has not confirmed this List deletion."));
        }
        Ok(AcceptedDeletion {
            list_id: self.list_id().into(),
            record_id: self.record_id().into(),
        })
    }

    pub fn confirms_deleted(&self, readback: &ListRecordReadback) -> bool {
        readback.list_id == self.list_id()
            && readback.record_id == self.record_id()
            && readback.is_deleted()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedDeletion {
    list_id: String,
    record_id: String,
}
impl AcceptedDeletion {
    pub fn list_id(&self) -> &str {
        &self.list_id
    }
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    pub fn confirms_deleted(&self, readback: &ListRecordReadback) -> bool {
        readback.list_id == self.list_id
            && readback.record_id == self.record_id
            && readback.is_deleted()
    }
}

/// Request-associated records.info evidence. No constructor from a missing row,
/// filtered projection, generic API error, or absent list-page entry exists.
#[derive(Clone, Debug, PartialEq)]
pub struct ListRecordReadback {
    list_id: String,
    record_id: String,
    record: Option<Record>,
}
impl ListRecordReadback {
    pub fn from_response(
        list_id: &str,
        record_id: &str,
        response: &Value,
    ) -> Result<Self, ListError> {
        if !valid_id(list_id)
            || !valid_id(record_id)
            || conflicts(response, "list_id", list_id)
            || conflicts(response, "id", record_id)
            || response
                .get("list")
                .is_some_and(|list| list.get("id").and_then(Value::as_str) != Some(list_id))
        {
            return Err(ListError("List item readback identity did not match."));
        }
        let record = if response["ok"] == false && response["error"] == "record_deleted" {
            if response
                .get("record")
                .is_some_and(|record| !record.is_null())
            {
                return Err(ListError("List item deletion evidence is contradictory."));
            }
            None
        } else if response["ok"] == true && response.get("error").is_none_or(Value::is_null) {
            let record = Record::parse(
                response
                    .get("record")
                    .cloned()
                    .ok_or(ListError("List item readback is missing."))?,
            )?;
            if record.id != record_id || conflicts(&record.raw, "list_id", list_id) {
                return Err(ListError("List item readback identity did not match."));
            }
            // A successful response, even with is_deleted metadata, is not the
            // source-backed record_deleted error contract used for this path.
            Some(record)
        } else {
            return Err(ListError("List item deletion has not been confirmed."));
        };
        Ok(Self {
            list_id: list_id.into(),
            record_id: record_id.into(),
            record,
        })
    }
    pub fn list_id(&self) -> &str {
        &self.list_id
    }
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    pub fn is_deleted(&self) -> bool {
        self.record.is_none()
    }
    pub fn record(&self) -> Option<&Record> {
        self.record.as_ref()
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn conflicts(raw: &Value, key: &str, expected: &str) -> bool {
    raw.get(key)
        .is_some_and(|value| value.as_str() != Some(expected))
}
fn validate_record(record: &Record, list_id: &str) -> Result<(), ListError> {
    if !valid_id(list_id)
        || !valid_id(&record.id)
        || record.raw.get("id").and_then(Value::as_str) != Some(&record.id)
        || conflicts(&record.raw, "list_id", list_id)
        || ["archived", "is_deleted"].iter().any(|key| {
            record
                .raw
                .get(key)
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
        })
        || record.raw.get("parent_record_id").is_some_and(|p| {
            !p.is_null() && p.as_str().is_none_or(|id| !id.is_empty() && !valid_id(id))
        })
    {
        return Err(ListError(
            "This List item is archived or has an invalid identity.",
        ));
    }
    Ok(())
}
fn descendants(page: &RecordsPage, target: &str) -> Result<BTreeMap<String, Value>, ListError> {
    let mut children: HashMap<&str, Vec<&Record>> = HashMap::new();
    let mut ids = HashSet::new();
    for record in &page.records {
        if !ids.insert(record.id.as_str()) {
            return Err(ListError("This List has ambiguous item identities."));
        }
        if let Some(parent) = record.parent_id() {
            children.entry(parent).or_default().push(record);
        }
    }
    let mut result = BTreeMap::new();
    let mut pending = vec![target];
    let mut seen = HashSet::from([target]);
    while let Some(parent) = pending.pop() {
        for record in children.get(parent).into_iter().flatten() {
            if !seen.insert(&record.id) {
                return Err(ListError("This List item has cyclic subtasks."));
            }
            result.insert(record.id.clone(), record.raw.clone());
            pending.push(&record.id);
        }
    }
    Ok(result)
}
