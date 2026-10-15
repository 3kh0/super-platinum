//! Direct-root subitems: one create, server identity, explicit positive readback.
//! Source contract: tmp/list-subitem-create-plan/source-excerpts.json.
use super::{CreationIntent, Document, ListError, PersonalTaskLists, Record, RecordsPage};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
mod readback;
pub use readback::SubitemReadback;
#[cfg(test)]
mod tests;

pub const SUBITEM_INPUT_LIMIT: usize = 2 * 1024 * 1024;
/// Count before cloning. Bounded output, even when input belongs to the caller.
pub fn subitem_input_bytes(document: &Document, page: &RecordsPage) -> Result<usize, ListError> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= SUBITEM_INPUT_LIMIT)
                .ok_or_else(|| std::io::Error::other("subitem input limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Counter(0);
    serde_json::to_writer(&mut count, &(&document.raw, &page.raw))
        .map_err(|_| ListError("This List is too large for a retained subitem operation."))?;
    Ok(count.0)
}
#[derive(Clone, Debug, PartialEq)]
pub struct SubitemCreationIntent {
    document: Arc<Document>,
    page: Arc<RecordsPage>,
    parent: String,
}
impl SubitemCreationIntent {
    pub fn capture(
        document: &Document,
        page: &RecordsPage,
        parent: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
        enabled: bool,
    ) -> Result<Self, ListError> {
        subitem_input_bytes(document, page)?;
        Self::eligible(document, page, parent, allow, personal, enabled)?;
        if Document::parse(document.raw.clone())? != *document
            || RecordsPage::parse(page.raw.clone())? != *page
        {
            return Err(ListError("List representations disagree."));
        }
        Ok(Self {
            document: Arc::new(document.clone()),
            page: Arc::new(page.clone()),
            parent: parent.into(),
        })
    }
    /// Retains an immutable, host-bounded snapshot only. This grants no dispatch
    /// authority: admit revalidates both typed/raw representations and all rules.
    /// Hosts can cheaply retain Arcs on the UI thread and validate on a worker.
    pub fn retain(document: Arc<Document>, page: Arc<RecordsPage>, parent: String) -> Self {
        Self {
            document,
            page,
            parent,
        }
    }
    fn validate_snapshot(&self) -> Result<(), ListError> {
        subitem_input_bytes(&self.document, &self.page)?;
        if Document::parse(self.document.raw.clone())? != *self.document
            || RecordsPage::parse(self.page.raw.clone())? != *self.page
        {
            return Err(ListError("List representations disagree."));
        }
        Ok(())
    }
    pub fn eligible(
        document: &Document,
        page: &RecordsPage,
        parent: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
        enabled: bool,
    ) -> Result<(), ListError> {
        if !enabled {
            return Err(ListError("Subitems are not enabled for this account."));
        }
        // Empty view is deliberate: no Completed or Board/filter seeds.
        CreationIntent::eligible(document, allow, personal, None)?;
        let row = page
            .record(parent)
            .ok_or(ListError("Open the existing parent again."))?;
        active(row, &document.id)?;
        if row.parent_id().is_some()
            || row
                .raw
                .get("parent_record_id")
                .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(ListError("Only a root item can have a new subitem."));
        }
        if page
            .hierarchy()
            .iter()
            .any(|r| matches!(r.relation, super::ParentRelation::Cycle(_)))
        {
            return Err(ListError("The loaded parent relations are cyclic."));
        }
        // Conservative union of all known links, including archived/unknown rows.
        if page
            .records
            .iter()
            .filter(|r| r.parent_id() == Some(parent))
            .count()
            >= 50
        {
            return Err(ListError(
                "The maximum of 50 known subitems has been reached.",
            ));
        }
        Ok(())
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn parent_id(&self) -> &str {
        &self.parent
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn population(&self) -> &RecordsPage {
        &self.page
    }
    pub fn revalidate(
        &self,
        document: &Document,
        parent_read: &SubitemReadback,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
        enabled: bool,
    ) -> Result<(), ListError> {
        self.validate_snapshot()?;
        Self::eligible(document, &self.page, &self.parent, allow, personal, enabled)?;
        if Document::parse(document.raw.clone())? != *document
            || !super::cell_edit::document_matches(&self.document.raw, &document.raw)
            || !super::cell_edit::document_matches(&document.raw, &parent_read.document.raw)
            || parent_read.parent
                != *self
                    .page
                    .record(&self.parent)
                    .ok_or(ListError("Parent missing."))?
        {
            return Err(ListError(
                "The parent or List changed. Refresh before adding a subitem.",
            ));
        }
        // New/changed positively returned children change the reviewed baseline.
        // Absence is never removal and does not lower the known-child count.
        if parent_read
            .children
            .iter()
            .any(|r| self.page.record(&r.id) != Some(r))
        {
            return Err(ListError(
                "Subitems changed. Refresh the parent before adding another.",
            ));
        }
        Ok(())
    }
    pub fn validate_response(&self, raw: &Value) -> Result<Record, ListError> {
        positive(raw)?;
        optional_owner(raw, "list_id", self.list_id())?;
        let record = Record::parse(
            raw.get("record")
                .cloned()
                .ok_or(ListError("No child identity was returned."))?,
        )?;
        active(&record, self.list_id())?;
        if self.page.record(&record.id).is_some() || record.parent_id() != Some(self.parent_id()) {
            return Err(ListError(
                "The new subitem's parent or identity was not confirmed.",
            ));
        }
        fields(&self.document, &record)?;
        Ok(record)
    }
}
/// Non-clone admission consumed by the exact builder. Cannot be rearmed.
pub struct SubitemCreatePermit {
    intent: SubitemCreationIntent,
}
impl SubitemCreatePermit {
    pub(crate) fn into_intent(self) -> SubitemCreationIntent {
        self.intent
    }
}
pub struct SubitemCreateAttempt {
    intent: Option<SubitemCreationIntent>,
}
impl SubitemCreateAttempt {
    pub fn new(intent: SubitemCreationIntent) -> Self {
        Self {
            intent: Some(intent),
        }
    }
    pub fn admit(
        &mut self,
        document: &Document,
        read: &SubitemReadback,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
        enabled: bool,
    ) -> Result<SubitemCreatePermit, ListError> {
        self.intent
            .as_ref()
            .ok_or(ListError("This subitem request was already admitted."))?
            .revalidate(document, read, allow, personal, enabled)?;
        Ok(SubitemCreatePermit {
            intent: self.intent.take().unwrap(),
        })
    }
}
fn positive(raw: &Value) -> Result<(), ListError> {
    if raw.get("ok") != Some(&Value::Bool(true)) || raw.get("error").is_some_and(|v| !v.is_null()) {
        Err(ListError("Slack has not confirmed this subitem response."))
    } else {
        Ok(())
    }
}
fn optional_owner(raw: &Value, key: &str, expected: &str) -> Result<(), ListError> {
    if raw.get(key).is_some_and(|v| v.as_str() != Some(expected)) {
        Err(ListError("Subitem ownership differs."))
    } else {
        Ok(())
    }
}
fn active(record: &Record, list: &str) -> Result<(), ListError> {
    optional_owner(&record.raw, "list_id", list)?;
    if record.id.trim() != record.id
        || record.id.is_empty()
        || ["archived", "is_deleted"].iter().any(|k| {
            record
                .raw
                .get(*k)
                .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
        })
    {
        return Err(ListError("The subitem or parent is unavailable."));
    }
    Ok(())
}
fn fields(document: &Document, row: &Record) -> Result<(), ListError> {
    for (key, raw) in &row.fields {
        let col = document
            .columns
            .iter()
            .find(|c| c.key == *key)
            .ok_or(ListError("Subitem schema differs."))?;
        optional_owner(raw, "column_id", &col.id)?;
        optional_owner(raw, "row_id", &row.id)?;
        if raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(ListError("Subitem field is unconfirmed."));
        }
    }
    Ok(())
}
fn same_identity(a: &Record, b: &Record) -> bool {
    a.id == b.id
        && ["date_created", "created_by"]
            .iter()
            .all(|k| a.raw.get(*k) == b.raw.get(*k))
}
