//! Board group deletion has independent schema and projected-cell effects.
//! Source Q81I schedules the schema callback and invokes the cell callback without
//! awaiting it. This plan does not turn those effects into an atomic operation.
use super::saved_views::{self, Projection, ProjectionContext, ProjectionLayout};
use super::{CellEdit, CellEditValue, Document, ListError, PersonalTaskLists, RecordsPage};
use serde_json::{Value, json};
use std::collections::BTreeSet;
mod attempt;
pub use attempt::{
    BoardGroupDeleteAttempt, BoardGroupDeleteCellsPermit, BoardGroupDeletePhase,
    BoardGroupDeleteSchemaPermit,
};
#[cfg(test)]
mod tests;
#[derive(Clone, Debug, PartialEq)]
pub struct BoardGroupDelete {
    document: Document,
    refinement: Option<super::ViewRefinementDraft>,
    records: RecordsPage,
    view: String,
    column_id: String,
    column_key: String,
    value: String,
    projection: Projection,
    options: Value,
    cells: Vec<Value>,
    expected_metadata: Value,
    expected_page: Value,
}
fn invalid() -> ListError {
    ListError(
        "This Board group deletion is not supported. Review the current groups and loaded items.",
    )
}
impl BoardGroupDelete {
    pub fn capture(
        doc: &Document,
        page: &RecordsPage,
        view: &str,
        value: &str,
        context: &ProjectionContext,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        Self::capture_read(doc, doc, page, view, value, context, allow, personal)
    }
    pub fn capture_refined(
        doc: &Document,
        page: &RecordsPage,
        local: &super::ViewRefinementDraft,
        value: &str,
        context: &ProjectionContext,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        let read = super::BoardLaneOrder::refined_projection_document(doc, local, None)?;
        let mut intent = Self::capture_read(
            doc,
            &read,
            page,
            local.view_id(),
            value,
            context,
            allow,
            personal,
        )?;
        intent.refinement = Some(local.clone());
        Ok(intent)
    }
    fn capture_read(
        doc: &Document,
        read: &Document,
        page: &RecordsPage,
        view: &str,
        value: &str,
        context: &ProjectionContext,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        let column = super::board_choice::basis(read, view, allow, personal)?;
        if RecordsPage::parse(page.raw.clone())? != *page || value.is_empty() || value.contains('|')
        {
            return Err(invalid());
        }
        let raw = read
            .views
            .iter()
            .find(|v| v["id"] == view)
            .ok_or_else(invalid)?;
        let selected = saved_views::SavedView::parse(raw.clone()).map_err(|_| invalid())?;
        let projected =
            saved_views::project(read, page, &selected, context).map_err(|_| invalid())?;
        if projected.layout != ProjectionLayout::Board {
            return Err(invalid());
        }
        let group = projected
            .groups
            .iter()
            .find(|g| g.key.as_deref() == Some(value))
            .ok_or_else(invalid)?;
        let mut options = column.raw["options"].clone();
        let choices = options["choices"].as_array_mut().ok_or_else(invalid)?;
        let at = choices
            .iter()
            .position(|c| c["value"] == value)
            .ok_or_else(invalid)?;
        choices.remove(at);
        if !super::board_choice::supported_options(&options) {
            return Err(invalid());
        }
        let mut cells = Vec::new();
        let mut seen = BTreeSet::new();
        let mut expected_page = page.raw.clone();
        for instance in &group.rows {
            if !seen.insert(instance.record_id.clone()) {
                continue;
            }
            let record = page.record(&instance.record_id).ok_or_else(invalid)?;
            let values = match column.edit_value(record, &doc.id, personal)? {
                CellEditValue::Select(values) => values,
                _ => return Err(invalid()),
            };
            // BoardView G compares the projected scalar exactly. A multi-select
            // card may occupy this lane yet "A|B" must not be cleared by delete A.
            if values.join("|") != value {
                continue;
            }
            // Projection is a read model, not record mutation authority. Missing
            // list_id uses the captured page/host provenance; a present identity
            // must match. Only ordinary, unambiguous target cells are writable.
            if record
                .raw
                .get("list_id")
                .is_some_and(|v| v.as_str() != Some(doc.id.as_str()))
                || record
                    .raw
                    .get("parent_record_id")
                    .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
                || ["archived", "is_deleted", "is_suggested", "suggested"]
                    .iter()
                    .any(|k| {
                        record
                            .raw
                            .get(*k)
                            .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
                    })
                || record.fields.iter().any(|(key, field)| {
                    key != &column.key
                        && field
                            .get("column_id")
                            .is_some_and(|id| id.as_str() == Some(column.id.as_str()))
                })
            {
                return Err(invalid());
            }
            let edit = CellEdit::capture(
                doc,
                page,
                &record.id,
                &column.id,
                allow,
                personal,
                CellEditValue::Select(Vec::new()),
            )?;
            let wire = edit.cells();
            let a = wire
                .as_array()
                .filter(|a| a.len() == 1)
                .ok_or_else(invalid)?;
            cells.push(a[0].clone());
            let row = expected_page["records"]
                .as_array_mut()
                .ok_or_else(invalid)?
                .iter_mut()
                .find(|r| r["id"] == record.id)
                .ok_or_else(invalid)?;
            let field = row["fields"]
                .as_array_mut()
                .ok_or_else(invalid)?
                .iter_mut()
                .find(|f| f["key"] == column.key)
                .ok_or_else(invalid)?;
            field["select"] = json!([]);
        }
        // Explicit local request bound, not a Slack capability claim.
        if cells.len() > 1000 || serde_json::to_vec(&cells).map_or(true, |b| b.len() > 512 * 1024) {
            return Err(invalid());
        }
        let mut expected_metadata = doc.raw["list_metadata"].clone();
        expected_metadata["schema"]
            .as_array_mut()
            .ok_or_else(invalid)?
            .iter_mut()
            .find(|c| c["id"] == column.id)
            .ok_or_else(invalid)?["options"] = options.clone();
        Ok(Self {
            document: doc.clone(),
            refinement: None,
            records: page.clone(),
            view: view.into(),
            column_id: column.id.clone(),
            column_key: column.key.clone(),
            value: value.into(),
            projection: projected,
            options,
            cells,
            expected_metadata,
            expected_page,
        })
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn original_records(&self) -> &RecordsPage {
        &self.records
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        &self.view
    }
    pub fn column_id(&self) -> &str {
        &self.column_id
    }
    pub fn column_key(&self) -> &str {
        &self.column_key
    }
    pub fn value(&self) -> &str {
        &self.value
    }
    pub fn options(&self) -> &Value {
        &self.options
    }
    pub fn cells(&self) -> &[Value] {
        &self.cells
    }
    pub fn revalidate(
        &self,
        doc: &Document,
        page: &RecordsPage,
        context: &ProjectionContext,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if doc != &self.document || page != &self.records {
            return Err(invalid());
        }
        let current = if let Some(local) = &self.refinement {
            Self::capture_refined(doc, page, local, &self.value, context, allow, personal)?
        } else {
            Self::capture(doc, page, &self.view, &self.value, context, allow, personal)?
        };
        if current != *self {
            return Err(invalid());
        }
        Ok(())
    }
    fn incarnation(&self, doc: &Document) -> bool {
        Document::parse(doc.raw.clone()).is_ok_and(|d| d == *doc)
            && doc.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|k| doc.raw.get(*k) == self.document.raw.get(*k))
    }
    /// Independent metadata observation. Backfill can affect records separately.
    pub fn observes_schema(&self, doc: &Document) -> bool {
        self.incarnation(doc) && doc.raw.get("list_metadata") == Some(&self.expected_metadata)
    }
    /// Conservative exact page observation. Normalization/backfill elsewhere in
    /// this page remains unconfirmed; no promise about unloaded pages is made.
    pub fn observes_cells(&self, doc: &Document, page: &RecordsPage) -> bool {
        self.incarnation(doc)
            && !self.cells.is_empty()
            && RecordsPage::parse(page.raw.clone()).is_ok_and(|p| p == *page)
            && page.raw == self.expected_page
    }
}
