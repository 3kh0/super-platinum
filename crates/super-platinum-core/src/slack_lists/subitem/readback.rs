use super::*;
#[derive(Clone, Debug)]
pub struct SubitemReadback {
    pub(super) document: Document,
    pub(super) parent: Record,
    pub(super) children: Vec<Record>,
}
impl SubitemReadback {
    pub fn from_response(list: &str, parent: &str, raw: &Value) -> Result<Self, ListError> {
        positive(raw)?;
        optional_owner(raw, "list_id", list)?;
        optional_owner(raw, "id", parent)?;
        let document = Document::parse(
            raw.get("list")
                .cloned()
                .ok_or(ListError("Parent List is missing."))?,
        )?;
        if document.id != list || super::super::unavailable(&document.raw) {
            return Err(ListError("Parent List differs."));
        }
        for key in ["is_deleted", "is_tombstoned"] {
            if document
                .raw
                .get(key)
                .is_some_and(|v| v != &Value::Bool(false))
            {
                return Err(ListError("Parent List is unavailable."));
            }
        }
        let row = Record::parse(
            raw.get("record")
                .cloned()
                .ok_or(ListError("Parent record is missing."))?,
        )?;
        active(&row, list)?;
        fields(&document, &row)?;
        if row.id != parent || row.parent_id().is_some() {
            return Err(ListError("Parent is no longer a root item."));
        }
        if row
            .raw
            .get("parent_record_id")
            .is_some_and(|v| !v.is_null() && !v.is_string())
        {
            return Err(ListError("Parent relation is malformed."));
        }
        let children = raw
            .get("subtasks")
            .and_then(Value::as_array)
            .ok_or(ListError("Explicit subitem evidence is missing."))?;
        let mut ids = HashSet::from([parent.to_owned()]);
        let children = children
            .iter()
            .map(|raw| {
                let row = Record::parse(raw.clone())?;
                active(&row, list)?;
                fields(&document, &row)?;
                if row.parent_id() != Some(parent) || !ids.insert(row.id.clone()) {
                    return Err(ListError("Subitem parent identities are ambiguous."));
                }
                Ok(row)
            })
            .collect::<Result<Vec<_>, ListError>>()?;
        Ok(Self {
            document,
            parent: row,
            children,
        })
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn metadata_matches(&self, document: &Document) -> bool {
        super::super::cell_edit::document_matches(&self.document.raw, &document.raw)
    }
    /// Positive full-object replacements; omitted rows/envelope/cursor retained.
    pub fn merge(
        &self,
        original: &Document,
        page: &RecordsPage,
    ) -> Result<(Document, RecordsPage), ListError> {
        if original.id != self.document.id
            || ["created", "user"]
                .iter()
                .any(|k| original.raw.get(*k) != self.document.raw.get(*k))
            || page
                .record(&self.parent.id)
                .is_none_or(|p| !same_identity(p, &self.parent) || p.parent_id().is_some())
        {
            return Err(ListError("The parent or List incarnation changed."));
        }
        let mut records = page.records.clone();
        for incoming in std::iter::once(&self.parent).chain(&self.children) {
            if let Some(old) = records.iter_mut().find(|r| r.id == incoming.id) {
                if !same_identity(old, incoming) || old.parent_id() != incoming.parent_id() {
                    return Err(ListError("Subitem identity or parent changed."));
                }
                *old = incoming.clone();
            } else {
                records.push(incoming.clone());
            }
        }
        let mut raw = page.raw.clone();
        raw["records"] = Value::Array(records.iter().map(|r| r.raw.clone()).collect());
        let page = RecordsPage::parse(raw)?;
        // A root plus direct children cannot add a cycle, but reject preexisting
        // malformed graph evidence involving a returned child as an ancestor.
        if page
            .hierarchy()
            .iter()
            .any(|r| matches!(r.relation, super::super::ParentRelation::Cycle(_)))
        {
            return Err(ListError("Subitem ancestry is cyclic."));
        }
        Ok((self.document.clone(), page))
    }
    pub fn confirmed<'a>(
        &'a self,
        intent: &SubitemCreationIntent,
        receipt: &Record,
    ) -> Option<&'a Record> {
        if self.parent.id != intent.parent_id()
            || receipt.parent_id() != Some(intent.parent_id())
            || intent.population().record(&receipt.id).is_some()
            || active(receipt, intent.list_id()).is_err()
            || fields(intent.original(), receipt).is_err()
        {
            return None;
        }
        self.children
            .iter()
            .find(|r| same_identity(r, receipt) && r.parent_id() == Some(intent.parent_id()))
    }
}
