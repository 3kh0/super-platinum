//! Immutable Board drop plan. This changes one per-view record position and,
//! when crossing lanes, one typed grouping cell in that same request.
use super::saved_views::{self, Projection, ProjectionContext, ProjectionLayout};
use super::{CellEdit, CellEditValue, Document, ListError, PersonalTaskLists, Record, RecordsPage};
use serde_json::{Value, json};
mod attempt;
mod position;
pub use attempt::{BoardRecordMoveAttempt, BoardRecordMovePermit, BoardRecordMovePhase};

#[derive(Clone, Debug, PartialEq)]
pub struct BoardRecordMove {
    document: Document,
    refinement: Option<(
        super::ViewRefinementDraft,
        Option<super::BoardLaneOrderDraft>,
    )>,
    records: RecordsPage,
    view_id: String,
    row: String,
    source: String,
    destination: String,
    before: Option<String>,
    projection: Projection,
    position: Option<String>,
    cell: Option<CellEdit>,
    expected_page: Value,
}
#[cfg(test)]
mod tests;
fn invalid() -> ListError {
    ListError("This Board drop is not supported. Review the current Board.")
}
fn member(record: &Record, document: &Document) -> bool {
    record
        .raw
        .get("list_id")
        .is_none_or(|v| v.as_str() == Some(document.id.as_str()))
        && record
            .raw
            .get("parent_record_id")
            .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
        && ["archived", "is_deleted", "is_suggested", "suggested"]
            .iter()
            .all(|k| {
                record
                    .raw
                    .get(*k)
                    .is_none_or(|v| v.is_null() || v == &Value::Bool(false))
            })
}
fn projection(
    doc: &Document,
    page: &RecordsPage,
    id: &str,
    context: &ProjectionContext,
) -> Result<Projection, ListError> {
    if Document::parse(doc.raw.clone())? != *doc || RecordsPage::parse(page.raw.clone())? != *page {
        return Err(invalid());
    }
    let mut views = doc.views.iter().filter(|v| v["id"] == id);
    let raw = views.next().ok_or_else(invalid)?;
    if views.next().is_some() || raw["type"] != "board" || raw["is_locked"] == true {
        return Err(invalid());
    }
    let view = saved_views::SavedView::parse(raw.clone()).map_err(|_| invalid())?;
    let projection = saved_views::project(doc, page, &view, context).map_err(|_| invalid())?;
    if projection.layout != ProjectionLayout::Board {
        return Err(invalid());
    }
    Ok(projection)
}
impl BoardRecordMove {
    /// Host captures this after an actual rendered drop or keyboard move and
    /// supplies the same current projection context on fresh admission. Partial
    /// pages stay partial; only loaded projected destination neighbors are used.
    pub fn capture(
        doc: &Document,
        page: &RecordsPage,
        view: &str,
        context: &ProjectionContext,
        row: &str,
        source: &str,
        destination: &str,
        before: Option<&str>,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        Self::capture_read(
            doc,
            doc,
            page,
            view,
            context,
            row,
            source,
            destination,
            before,
            allow,
            personal,
        )
    }
    pub fn capture_refined(
        doc: &Document,
        page: &RecordsPage,
        local: &super::ViewRefinementDraft,
        provider: Option<&super::BoardLaneOrderDraft>,
        context: &ProjectionContext,
        row: &str,
        source: &str,
        destination: &str,
        before: Option<&str>,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        let read = super::BoardLaneOrder::refined_projection_document(doc, local, provider)?;
        let mut intent = Self::capture_read(
            doc,
            &read,
            page,
            local.view_id(),
            context,
            row,
            source,
            destination,
            before,
            allow,
            personal,
        )?;
        intent.refinement = Some((local.clone(), provider.cloned()));
        Ok(intent)
    }
    fn capture_read(
        doc: &Document,
        read: &Document,
        page: &RecordsPage,
        view: &str,
        context: &ProjectionContext,
        row: &str,
        source: &str,
        destination: &str,
        before: Option<&str>,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        if let Some(reason) = doc.read_only_reason(allow) {
            return Err(ListError(reason));
        }
        if personal.is_none_or(|p| p.contains(&doc.id)) {
            return Err(ListError("Ordinary List preferences are not verified."));
        }
        let projected = projection(read, page, view, context)?;
        let source_group = projected
            .groups
            .iter()
            .find(|g| g.key.as_deref() == Some(source))
            .ok_or_else(invalid)?;
        if source_group
            .rows
            .iter()
            .filter(|r| r.record_id == row)
            .count()
            != 1
        {
            return Err(invalid());
        }
        let target = projected
            .groups
            .iter()
            .find(|g| g.key.as_deref() == Some(destination))
            .ok_or_else(invalid)?;
        let original = page.record(row).ok_or_else(invalid)?;
        if original
            .raw
            .get("list_id")
            .is_some_and(|v| v.as_str() != Some(doc.id.as_str()))
            || original
                .raw
                .get("parent_record_id")
                .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
            || ["archived", "is_deleted", "is_suggested", "suggested"]
                .iter()
                .any(|k| {
                    original
                        .raw
                        .get(*k)
                        .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
                })
        {
            return Err(invalid());
        }
        if before == Some(row) {
            return Err(invalid());
        }
        // A multi-select source may already occupy the destination. The source
        // renderer then creates a duplicate drag row; this ambiguous shape is not
        // converted into an inferred remove-one-membership operation here.
        if source != destination && target.rows.iter().any(|r| r.record_id == row) {
            return Err(invalid());
        }
        let mut ids = target
            .rows
            .iter()
            .filter(|r| r.record_id != row)
            .map(|r| r.record_id.clone())
            .collect::<Vec<_>>();
        let index = match before {
            Some(id) => ids.iter().position(|r| r == id).ok_or_else(invalid)?,
            None => ids.len(),
        };
        ids.insert(index, row.into());
        if source == destination
            && ids
                == target
                    .rows
                    .iter()
                    .map(|r| r.record_id.clone())
                    .collect::<Vec<_>>()
        {
            return Err(ListError("This card is already in that position."));
        }
        let left = index.checked_sub(1).and_then(|i| page.record(&ids[i]));
        let right = ids.get(index + 1).and_then(|id| page.record(id));
        if left
            .into_iter()
            .chain(right)
            .any(|record| !member(record, doc))
        {
            return Err(invalid());
        }
        let position = position::between(left, right, view)?;
        let cell = if source == destination {
            None
        } else {
            let raw = read
                .views
                .iter()
                .find(|v| v["id"] == view)
                .ok_or_else(invalid)?;
            let key = raw
                .pointer("/grouping/group_by")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let mut columns = doc.columns.iter().filter(|c| c.key == key);
            let column = columns.next().ok_or_else(invalid)?;
            if columns.next().is_some()
                || doc.columns.iter().filter(|c| c.id == column.id).count() != 1
                || original.fields.iter().any(|(key, field)| {
                    key != &column.key
                        && field
                            .get("column_id")
                            .is_some_and(|id| id.as_str() == Some(column.id.as_str()))
                })
            {
                return Err(invalid());
            }
            let value = match column.kind.as_str() {
                "checkbox" if matches!(destination, "" | "true") => {
                    CellEditValue::Boolean(destination == "true")
                }
                "select" => CellEditValue::Select(if destination.is_empty() {
                    vec![]
                } else {
                    vec![destination.into()]
                }),
                _ => return Err(invalid()),
            };
            Some(CellEdit::capture(
                doc, page, row, &column.id, allow, personal, value,
            )?)
        };
        if position.is_none() && cell.is_none() {
            return Err(ListError(
                "The current items have no usable position bounds.",
            ));
        }
        let mut expected_page = page.raw.clone();
        let record = expected_page["records"]
            .as_array_mut()
            .ok_or_else(invalid)?
            .iter_mut()
            .find(|r| r["id"] == row)
            .ok_or_else(invalid)?;
        if let Some(position) = &position {
            if record.get("view_positions").is_none_or(Value::is_null) {
                record["view_positions"] = json!({});
            }
            let positions = record["view_positions"]
                .as_object_mut()
                .ok_or_else(invalid)?;
            let key = positions
                .keys()
                .find(|id| id.eq_ignore_ascii_case(view))
                .cloned()
                .unwrap_or_else(|| view.into());
            positions.insert(key, json!(position));
        }
        if let Some(cell) = &cell {
            let fields = record["fields"].as_array_mut().ok_or_else(invalid)?;
            let field = fields
                .iter_mut()
                .find(|f| f["key"] == cell.column_key())
                .ok_or_else(invalid)?;
            let cells = cell.cells();
            let wire = cells[0].as_object().ok_or_else(invalid)?;
            for (key, value) in wire
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), "row_id" | "column_id"))
            {
                field[key] = value.clone();
            }
        }
        Ok(Self {
            document: doc.clone(),
            refinement: None,
            records: page.clone(),
            view_id: view.into(),
            row: row.into(),
            source: source.into(),
            destination: destination.into(),
            before: before.map(str::to_owned),
            projection: projected,
            position,
            cell,
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
        &self.view_id
    }
    pub fn record_id(&self) -> &str {
        &self.row
    }
    pub fn desired_position(&self) -> Option<&str> {
        self.position.as_deref()
    }
    pub fn cells(&self) -> Option<Value> {
        self.cell.as_ref().map(CellEdit::cells)
    }
    pub fn is_cell_only(&self) -> bool {
        self.position.is_none()
    }
    pub fn revalidate(
        &self,
        doc: &Document,
        page: &RecordsPage,
        context: &ProjectionContext,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        if !super::cell_edit::document_matches(&self.document.raw, &doc.raw)
            || page != &self.records
        {
            return Err(ListError("This Board changed. Review the drop again."));
        }
        let current = if let Some((local, provider)) = &self.refinement {
            Self::capture_refined(
                doc,
                page,
                local,
                provider.as_ref(),
                context,
                &self.row,
                &self.source,
                &self.destination,
                self.before.as_deref(),
                allow,
                personal,
            )?
        } else {
            Self::capture(
                doc,
                page,
                &self.view_id,
                context,
                &self.row,
                &self.source,
                &self.destination,
                self.before.as_deref(),
                allow,
                personal,
            )?
        };
        if current.projection != self.projection
            || current.position != self.position
            || current.cells() != self.cells()
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Strict raw observation. Server-normalized timestamps/fields/row arrays
    /// remain Unconfirmed pending an explicitly supported readback contract.
    pub fn observes_desired(&self, doc: &Document, page: &RecordsPage) -> bool {
        Document::parse(doc.raw.clone()).is_ok_and(|d| d == *doc)
            && RecordsPage::parse(page.raw.clone()).is_ok_and(|p| p == *page)
            && super::cell_edit::document_matches(&self.document.raw, &doc.raw)
            && page.raw == self.expected_page
    }
}
