//! KanbanColumnOrderProvider useState(null), BoardView W/V and 5QWf.B_/v4.
//! One host visit owns this typed draft, including across selected-view changes.
//! It is not a server intent and cannot construct a request permit.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardLaneOrderDraft {
    cells: Vec<Value>,
}
pub(super) fn shared(view: &Value) -> Result<bool, ListError> {
    match view.pointer("/grouping/group_by_column_id") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::String(s)) => Ok(!s.is_empty()),
        _ => Err(unsupported()),
    }
}
impl BoardLaneOrderDraft {
    pub(super) fn from_refinement_cells(cells: Vec<Value>) -> Self {
        Self { cells }
    }
    pub fn uses_shared_order(doc: &Document, id: &str) -> Result<bool, ListError> {
        if Document::parse(doc.raw.clone())? != *doc {
            return Err(unsupported());
        }
        let (view, _) = draft_basis(doc, id)?;
        shared(view)
    }
    pub fn current_order(
        doc: &Document,
        id: &str,
        current: Option<&Self>,
    ) -> Result<Vec<String>, ListError> {
        if Document::parse(doc.raw.clone())? != *doc {
            return Err(unsupported());
        }
        let (view, col) = draft_basis(doc, id)?;
        let mut remaining = base_keys(col, view)?;
        let mut ordered = Vec::new();
        let raw = view.pointer("/grouping/order").filter(|v| !v.is_null());
        let values = if let Some(draft) = current {
            Some(draft.cells.as_slice())
        } else {
            raw.map(|v| {
                v.as_array()
                    .filter(|a| a.len() <= 1024)
                    .map(Vec::as_slice)
                    .ok_or_else(unsupported)
            })
            .transpose()?
        };
        if let Some(cells) = values {
            for cell in cells {
                let key = if current.is_some() {
                    // Source B_ converts under the CURRENT grouping type. A Select
                    // draft viewed under Checkbox (and conversely) means empty,
                    // then current choice order. Private typed construction keeps
                    // arbitrary raw/unknown objects out of this local state.
                    if col.kind == "checkbox" {
                        if cell.get("checkbox") == Some(&Value::Bool(true)) {
                            "true".into()
                        } else {
                            String::new()
                        }
                    } else {
                        cell.get("select")
                            .and_then(Value::as_array)
                            .and_then(|a| a.first())
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .into()
                    }
                } else {
                    cell_key(cell, &col.kind)?
                };
                if let Some(at) = remaining.iter().position(|k| *k == key) {
                    ordered.push(remaining.remove(at));
                }
            }
        }
        ordered.extend(remaining);
        Ok(ordered)
    }
    pub fn moved(
        doc: &Document,
        id: &str,
        current: Option<&Self>,
        source: &str,
        target: &str,
        placement: BoardLanePlacement,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        eligible(doc, allow, personal)?;
        let (view, col) = draft_basis(doc, id)?;
        if shared(view)? {
            return Err(ListError("This Board order requires a shared request."));
        }
        let before = Self::current_order(doc, id, current)?;
        let mut keys = before.clone();
        if source == target {
            return Err(ListError("Choose a different destination lane."));
        }
        let at = keys
            .iter()
            .position(|k| k == source)
            .ok_or_else(unsupported)?;
        let key = keys.remove(at);
        let at = keys
            .iter()
            .position(|k| k == target)
            .ok_or_else(unsupported)?;
        keys.insert(
            at + usize::from(placement == BoardLanePlacement::After),
            key,
        );
        if keys == before {
            return Err(ListError("This lane is already in that position."));
        }
        Ok(Self {
            cells: keys
                .iter()
                .map(|key| {
                    if col.kind == "checkbox" {
                        json!({"checkbox":key=="true"})
                    } else {
                        json!({"select":if key.is_empty(){vec![]}else{vec![key]}})
                    }
                })
                .collect(),
        })
    }
    /// Ephemeral read input, never authoritative state or write admission.
    /// Read support belongs to saved_views::project, not ordinary lane authoring.
    /// Ungrouped Boards retain their original document; other Board types consume
    /// the provider's typed cells under their current grouping type (5QWf.B_).
    pub fn projection_document(&self, doc: &Document, id: &str) -> Result<Document, ListError> {
        if Document::parse(doc.raw.clone())? != *doc {
            return Err(unsupported());
        }
        let mut matches = doc.views.iter().filter(|view| view["id"] == id);
        let view = matches.next().ok_or_else(unsupported)?;
        if matches.next().is_some() || view["type"] != "board" {
            return Err(unsupported());
        }
        // Do not manufacture grouping or hide malformed read metadata. The
        // existing projection decides whether an unchanged shape is readable.
        if !view
            .pointer("/grouping/group_by")
            .and_then(Value::as_str)
            .is_some_and(|key| !key.is_empty())
        {
            return Ok(doc.clone());
        }
        let mut raw = doc.raw.clone();
        raw["list_metadata"]["views"]
            .as_array_mut()
            .ok_or_else(unsupported)?
            .iter_mut()
            .find(|v| v["id"] == id)
            .ok_or_else(unsupported)?["grouping"]["order"] = Value::Array(self.cells.clone());
        Document::parse(raw)
    }
    pub fn typed_order(&self) -> &[Value] {
        &self.cells
    }
}
#[cfg(test)]
mod tests;
