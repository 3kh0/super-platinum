//! Retained Q81I eM/eP/eV -> 5QWf.v4 -> BoardView V -> 0Kzd.v.
//! Shared lane ordering only. Local unsaved grouping, choice deletion and record
//! positions are separate operations. Unknown grouping members stay raw-shaped.
use super::{Column, Document, ListError, PersonalTaskLists, ViewRename};
use serde_json::{Value, json};
use std::collections::BTreeSet;
mod attempt;
mod draft;
mod refinement;
pub use attempt::{BoardLaneOrderAttempt, BoardLaneOrderPermit, BoardLaneOrderPhase};
pub use draft::BoardLaneOrderDraft;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardLanePlacement {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardLaneOrder {
    document: Document,
    view_id: String,
    grouping: Value,
    order: Vec<String>,
    expected_metadata: Value,
    refinement: Option<(super::ViewRefinementDraft, Option<BoardLaneOrderDraft>)>,
}
fn identity(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
}
fn unsupported() -> ListError {
    ListError("This Board lane order is not supported.")
}
fn eligible(
    doc: &Document,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if Document::parse(doc.raw.clone())? != *doc {
        return Err(ListError("List representations disagree."));
    }
    if let Some(reason) = doc.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&doc.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    Ok(())
}
fn basis<'a>(doc: &'a Document, id: &str) -> Result<(&'a Value, &'a Column), ListError> {
    let result = draft_basis(doc, id)?;
    if !ViewRename::supports(result.0) || !draft::shared(result.0)? {
        return Err(unsupported());
    }
    Ok(result)
}
fn draft_basis<'a>(doc: &'a Document, id: &str) -> Result<(&'a Value, &'a Column), ListError> {
    if !identity(id) {
        return Err(unsupported());
    }
    // Catalogue catches duplicate raw view identities; only a current custom Board
    // can create a shared lane-order permit.
    // 0Kzd and the switcher exclude Record/CanvasInline before catalogue work.
    // Still check full raw identities before filtering, and retain those raw
    // siblings in original/expected metadata and every current readback.
    let mut identities = BTreeSet::new();
    for view in &doc.views {
        let id = view["id"]
            .as_str()
            .filter(|id| identity(id))
            .ok_or_else(unsupported)?;
        if !identities.insert(id) {
            return Err(unsupported());
        }
    }
    let mut catalog_raw = doc.raw.clone();
    catalog_raw["list_metadata"]["views"] = Value::Array(
        doc.views
            .iter()
            .filter(|v| !matches!(v["type"].as_str(), Some("record" | "canvas_inline")))
            .cloned()
            .collect(),
    );
    let catalog_doc = Document::parse(catalog_raw)?;
    let catalog = super::saved_views::catalog(&catalog_doc).map_err(|_| unsupported())?;
    if !catalog.iter().any(|v| v.id == id && v.kind == "board") {
        return Err(unsupported());
    }
    let view = doc
        .views
        .iter()
        .find(|v| v["id"] == id)
        .ok_or_else(unsupported)?;
    let grouping = view
        .get("grouping")
        .and_then(Value::as_object)
        .ok_or_else(unsupported)?;
    let key = grouping
        .get("group_by")
        .and_then(Value::as_str)
        .filter(|s| identity(s))
        .ok_or_else(unsupported)?;
    let column_id = match grouping.get("group_by_column_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) if identity(s) => Some(s.as_str()),
        _ => return Err(unsupported()),
    };
    let mut columns = doc.columns.iter().filter(|c| c.key == key);
    let column = columns.next().ok_or_else(unsupported)?;
    if columns.next().is_some()
        || column_id.is_some_and(|id| column.id != id)
        || doc.columns.iter().filter(|c| c.id == column.id).count() != 1
        || ["deleted", "is_deleted", "suggested", "is_suggested"]
            .iter()
            .any(|k| column.raw.get(*k).is_some_and(|v| v != &Value::Bool(false)))
    {
        return Err(unsupported());
    }
    Ok((view, column))
}
fn base_keys(column: &Column, view: &Value) -> Result<Vec<String>, ListError> {
    let mut choices = Vec::new();
    match column.kind.as_str() {
        "checkbox" => choices.push(("true".to_owned(), String::new())),
        "select" => {
            if column
                .raw
                .pointer("/options/format")
                .is_some_and(|v| v != "single_select" && v != "multi_select")
            {
                return Err(unsupported());
            }
            let raw = column
                .raw
                .pointer("/options/choices")
                .and_then(Value::as_array)
                .ok_or_else(unsupported)?;
            if raw.len() > 1023 {
                return Err(unsupported());
            }
            for choice in raw {
                let key = choice["value"]
                    .as_str()
                    .filter(|s| identity(s))
                    .ok_or_else(unsupported)?;
                let label = choice["label"].as_str().ok_or_else(unsupported)?;
                // Opaque keys are not a pipe-delimited cell value in the Board.
                if key.contains('|') || choices.iter().any(|(k, _)| k == key) {
                    return Err(unsupported());
                }
                choices.push((key.into(), label.into()));
            }
            if let Some(sorts) = view.get("sorts").filter(|v| !v.is_null()) {
                let sorts = sorts.as_array().ok_or_else(unsupported)?;
                if let Some(sort) = sorts.iter().find(|s| s["key"] == column.key) {
                    let ascending = sort["ascending"].as_bool().ok_or_else(unsupported)?;
                    // JavaScript string comparison uses UTF-16 code units.
                    choices.sort_by(|a, b| {
                        let cmp = a.1.encode_utf16().cmp(b.1.encode_utf16());
                        if ascending { cmp } else { cmp.reverse() }
                    });
                }
            }
        }
        _ => return Err(unsupported()),
    }
    let mut keys = vec![String::new()];
    keys.extend(choices.into_iter().map(|(key, _)| key));
    Ok(keys)
}
fn cell_key(cell: &Value, kind: &str) -> Result<String, ListError> {
    let object = cell
        .as_object()
        .filter(|o| o.len() == 1)
        .ok_or_else(unsupported)?;
    match kind {
        "checkbox" => object
            .get("checkbox")
            .and_then(Value::as_bool)
            .map(|b| if b { "true" } else { "" }.into())
            .ok_or_else(unsupported),
        "select" => {
            let values = object
                .get("select")
                .and_then(Value::as_array)
                .filter(|a| a.len() <= 1)
                .ok_or_else(unsupported)?;
            match values.first() {
                None => Ok(String::new()),
                Some(v) => v
                    .as_str()
                    .filter(|s| identity(s))
                    .map(str::to_owned)
                    .ok_or_else(unsupported),
            }
        }
        _ => Err(unsupported()),
    }
}
impl BoardLaneOrder {
    /// Includes the empty lane even when it is not rendered. A host must resolve
    /// actual visible neighbors and capture their identities before invoking this.
    pub fn current_order(doc: &Document, id: &str) -> Result<Vec<String>, ListError> {
        if Document::parse(doc.raw.clone())? != *doc {
            return Err(unsupported());
        }
        let (view, column) = basis(doc, id)?;
        let mut remaining = base_keys(column, view)?;
        let mut ordered = Vec::new();
        if let Some(raw) = view.pointer("/grouping/order").filter(|v| !v.is_null()) {
            let raw = raw
                .as_array()
                .filter(|a| a.len() <= 1024)
                .ok_or_else(unsupported)?;
            for cell in raw {
                let key = cell_key(cell, &column.kind)?;
                // Actual B_ drops deleted/duplicate lane identities then appends
                // remaining choices. Unknown entry metadata is refused above.
                if let Some(index) = remaining.iter().position(|k| *k == key) {
                    ordered.push(remaining.remove(index));
                }
            }
        }
        ordered.extend(remaining);
        Ok(ordered)
    }
    pub fn capture(
        doc: &Document,
        id: &str,
        source: &str,
        target: &str,
        placement: BoardLanePlacement,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        Self::capture_with_draft(doc, id, source, target, placement, None, allow, personal)
    }
    /// The host owns the visit-local draft and must fence it across every await.
    /// Raw document admission/readback remain unchanged; the override supplies only
    /// the actual displayed starting order, never replacement canonical metadata.
    pub fn capture_with_draft(
        doc: &Document,
        id: &str,
        source: &str,
        target: &str,
        placement: BoardLanePlacement,
        draft: Option<&BoardLaneOrderDraft>,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        eligible(doc, allow, personal)?;
        let before = BoardLaneOrderDraft::current_order(doc, id, draft)?;
        let (view, column) = basis(doc, id)?;
        if source == target {
            return Err(ListError("Choose a different destination lane."));
        }
        let mut order = before.clone();
        let index = order
            .iter()
            .position(|key| key == source)
            .ok_or_else(unsupported)?;
        let key = order.remove(index);
        let index = order
            .iter()
            .position(|key| key == target)
            .ok_or_else(unsupported)?;
        order.insert(
            index + usize::from(placement == BoardLanePlacement::After),
            key,
        );
        if order == before {
            return Err(ListError("This lane is already in that position."));
        }
        let mut grouping = view["grouping"].clone();
        grouping["order"] = Value::Array(
            order
                .iter()
                .map(|key| {
                    if column.kind == "checkbox" {
                        json!({"checkbox":key == "true"})
                    } else {
                        json!({"select":if key.is_empty() {vec![]} else {vec![key]}})
                    }
                })
                .collect(),
        );
        // Bound the actual preserved grouping, not just the newly authored order.
        if grouping.to_string().len() > 8192 {
            return Err(unsupported());
        }
        let mut expected_metadata = doc.raw["list_metadata"].clone();
        let views = expected_metadata["views"]
            .as_array_mut()
            .ok_or_else(unsupported)?;
        views
            .iter_mut()
            .find(|v| v["id"] == id)
            .ok_or_else(unsupported)?["grouping"] = grouping.clone();
        Ok(Self {
            refinement: None,
            document: doc.clone(),
            view_id: id.into(),
            grouping,
            order,
            expected_metadata,
        })
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    pub fn grouping(&self) -> &Value {
        &self.grouping
    }
    pub fn desired_order(&self) -> &[String] {
        &self.order
    }
    pub fn revalidate(
        &self,
        fresh: &Document,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<(), ListError> {
        eligible(fresh, allow, personal)?;
        if !super::cell_edit::document_matches(&self.document.raw, &fresh.raw) {
            return Err(ListError(
                "This Board changed. Review the lane order again.",
            ));
        }
        if let Some((refinement, provider)) = &self.refinement {
            Self::refined_order(fresh, refinement, provider.as_ref())?;
        } else {
            Self::current_order(fresh, &self.view_id)?;
        }
        Ok(())
    }
    pub fn observes_desired(&self, fresh: &Document) -> bool {
        Document::parse(fresh.raw.clone()).is_ok_and(|parsed| parsed == *fresh)
            && fresh.id == self.document.id
            && ["created", "user"]
                .iter()
                .all(|key| fresh.raw.get(*key) == self.document.raw.get(*key))
            && fresh.raw.get("list_metadata") == Some(&self.expected_metadata)
    }
}
#[cfg(test)]
mod tests;
