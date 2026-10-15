//! Known-parent hydration, never ordinary List continuation.
//! Retained client-list-record-details.4fd506d6b6a13818 SHA256
//! ded0d242c564b6cf461d414afada0361ab39c4ff7bdf9bcbd052a5b1386d2f60,
//! offsets 32200..34900: incomplete population -> info -> parent + subtasks.
use super::*;

#[derive(Clone, Debug)]
pub struct ListRecordHydration {
    document: Document,
    parent: Record,
    subtasks: Vec<Record>,
}

impl ListRecordHydration {
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn parent_id(&self) -> &str {
        &self.parent.id
    }
    pub fn from_response(list: &str, parent: &str, raw: &Value) -> Result<Self, ListError> {
        if list.is_empty()
            || parent.is_empty()
            || raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || conflicts(raw, "list_id", list)
            || conflicts(raw, "id", parent)
        {
            return Err(ListError("List item hydration was not confirmed."));
        }
        let document = Document::parse(raw["list"].clone())?;
        if document.id != list {
            return Err(ListError("List item hydration owner did not match."));
        }
        let parent_record = active_record(raw["record"].clone(), list)?;
        if parent_record.id != parent {
            return Err(ListError("List item hydration parent did not match."));
        }
        let rows = match raw.get("subtasks") {
            None | Some(Value::Null) => &[][..],
            Some(Value::Array(rows)) => rows.as_slice(),
            _ => return Err(ListError("List subtasks are malformed.")),
        };
        let mut ids = HashSet::from([parent.to_owned()]);
        let mut subtasks = Vec::with_capacity(rows.len());
        for raw in rows {
            let record = active_record(raw.clone(), list)?;
            // This bounded slice admits direct children only. It does not guess
            // a parent for a partial row or recursively discover descendants.
            if record.parent_id() != Some(parent) || !ids.insert(record.id.clone()) {
                return Err(ListError("List subtasks have ambiguous parent identities."));
            }
            subtasks.push(record);
        }
        Ok(Self {
            document,
            parent: parent_record,
            subtasks,
        })
    }

    /// Merge positively returned records only; absence never means deletion.
    /// Caller must also hold its exact account/visit/read/revision baseline.
    pub fn merge(
        &self,
        document: &Document,
        page: &RecordsPage,
    ) -> Result<(Document, RecordsPage), ListError> {
        if document.id != self.document.id || page.complete() {
            return Err(ListError(
                "List hydration no longer matches an incomplete population.",
            ));
        }
        let before = page
            .record(&self.parent.id)
            .ok_or(ListError("Open this existing List item again."))?;
        active_record(before.raw.clone(), &document.id)?;
        if !same_identity(before, &self.parent) {
            return Err(ListError("List item identity changed during hydration."));
        }
        let mut ancestors = HashSet::from([before.id.as_str()]);
        let mut ancestor = before.parent_id();
        while let Some(id) = ancestor {
            if !ancestors.insert(id) {
                return Err(ListError("List item ancestry is cyclic."));
            }
            ancestor = page.record(id).and_then(Record::parent_id);
        }
        if self
            .subtasks
            .iter()
            .any(|row| ancestors.contains(row.id.as_str()))
        {
            return Err(ListError("List hydration would introduce cyclic ancestry."));
        }
        let mut records = page.records.clone();
        for incoming in std::iter::once(&self.parent).chain(self.subtasks.iter()) {
            if let Some(existing) = records.iter_mut().find(|r| r.id == incoming.id) {
                active_record(existing.raw.clone(), &document.id)?;
                if !same_identity(existing, incoming) {
                    return Err(ListError("List item identity changed during hydration."));
                }
                *existing = incoming.clone();
            } else {
                records.push(incoming.clone());
            }
        }
        let mut raw = page.raw.clone();
        raw["records"] = Value::Array(records.iter().map(|r| r.raw.clone()).collect());
        let merged = RecordsPage::parse(raw)?;
        if merged.next_cursor != page.next_cursor {
            return Err(ListError("List hydration cannot change completeness."));
        }
        Ok((self.document.clone(), merged))
    }
}

fn conflicts(raw: &Value, key: &str, expected: &str) -> bool {
    raw.get(key)
        .is_some_and(|value| value.as_str() != Some(expected))
}
fn active_record(raw: Value, list: &str) -> Result<Record, ListError> {
    if conflicts(&raw, "list_id", list)
        || ["archived", "is_deleted"].iter().any(|key| {
            raw.get(key)
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
        })
        || raw
            .get("parent_record_id")
            .is_some_and(|v| !v.is_null() && !v.is_string())
    {
        return Err(ListError(
            "List hydration contains unavailable or foreign records.",
        ));
    }
    Record::parse(raw)
}
fn same_identity(before: &Record, after: &Record) -> bool {
    ["date_created", "created_by", "parent_record_id"]
        .iter()
        .all(|key| before.raw.get(key) == after.raw.get(key))
}

#[cfg(test)]
mod tests;
