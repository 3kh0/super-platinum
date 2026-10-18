//! BoardView reads local grouping but V spreads the canonical saved grouping.
//! Q81I/5QWf order is based on displayed choices; no local document is published.
use super::*;
use crate::slack_lists::ViewRefinementDraft;
fn original<'a>(doc: &'a Document, id: &str) -> Result<&'a Value, ListError> {
    if Document::parse(doc.raw.clone())? != *doc {
        return Err(unsupported());
    }
    let mut views = doc.views.iter().filter(|v| v["id"] == id);
    let view = views.next().ok_or_else(unsupported)?;
    if views.next().is_some() {
        return Err(unsupported());
    }
    Ok(view)
}
fn shared_target(view: &Value) -> Result<bool, ListError> {
    let grouping = match view.get("grouping") {
        None | Some(Value::Null) => return Ok(false),
        Some(Value::Object(o)) => o,
        _ => return Err(unsupported()),
    };
    let text = |key: &str| match grouping.get(key) {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(s)) => Ok(s.as_str()),
        _ => Err(unsupported()),
    };
    Ok(!text("group_by")?.is_empty() && !text("group_by_column_id")?.is_empty())
}
/// The read adapter retains the saved order fallback independently of the local
/// grouping map. Missing/null order remains missing/null; provider overrides it.
pub(super) fn document(doc: &Document, local: &ViewRefinementDraft) -> Result<Document, ListError> {
    let saved = original(doc, local.view_id())?;
    let mut raw = local.projection_document(doc)?.raw;
    let view = raw["list_metadata"]["views"]
        .as_array_mut()
        .ok_or_else(unsupported)?
        .iter_mut()
        .find(|v| v["id"] == local.view_id())
        .ok_or_else(unsupported)?;
    if view["type"] != "board" {
        return Err(unsupported());
    }
    if view
        .pointer("/grouping/group_by")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        if let Some(order) = saved.pointer("/grouping/order") {
            view["grouping"]["order"] = order.clone();
        }
    }
    Document::parse(raw)
}
fn provider(
    doc: &Document,
    id: &str,
    current: Option<&BoardLaneOrderDraft>,
) -> Result<Option<BoardLaneOrderDraft>, ListError> {
    if let Some(current) = current {
        return Ok(Some(current.clone()));
    }
    let view = original(doc, id)?;
    let Some(raw) = view.pointer("/grouping/order").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let cells = raw
        .as_array()
        .filter(|a| a.len() <= 1024)
        .ok_or_else(unsupported)?;
    for cell in cells {
        let object = cell
            .as_object()
            .filter(|o| o.len() == 1)
            .ok_or_else(unsupported)?;
        if object.contains_key("checkbox") {
            cell_key(cell, "checkbox")?;
        } else {
            cell_key(cell, "select")?;
        }
    }
    // Same source B_ conversion for saved order and provider order.
    Ok(Some(BoardLaneOrderDraft::from_refinement_cells(
        cells.clone(),
    )))
}
impl BoardLaneOrder {
    pub fn refined_uses_shared(
        doc: &Document,
        local: &ViewRefinementDraft,
    ) -> Result<bool, ListError> {
        let saved = original(doc, local.view_id())?;
        document(doc, local)?;
        shared_target(saved)
    }
    pub fn refined_projection_document(
        doc: &Document,
        local: &ViewRefinementDraft,
        current: Option<&BoardLaneOrderDraft>,
    ) -> Result<Document, ListError> {
        let read = document(doc, local)?;
        match provider(doc, local.view_id(), current)? {
            Some(p) => p.projection_document(&read, local.view_id()),
            None => Ok(read),
        }
    }
    pub fn refined_order(
        doc: &Document,
        local: &ViewRefinementDraft,
        current: Option<&BoardLaneOrderDraft>,
    ) -> Result<Vec<String>, ListError> {
        let read = document(doc, local)?;
        let provider = provider(doc, local.view_id(), current)?;
        BoardLaneOrderDraft::current_order(&read, local.view_id(), provider.as_ref())
    }
    pub fn capture_refined(
        doc: &Document,
        local: &ViewRefinementDraft,
        current: Option<&BoardLaneOrderDraft>,
        source: &str,
        target: &str,
        placement: BoardLanePlacement,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        eligible(doc, allow, personal)?;
        let saved = original(doc, local.view_id())?;
        if !ViewRename::supports(saved) || !shared_target(saved)? {
            return Err(unsupported());
        }
        let key = saved
            .pointer("/grouping/group_by")
            .and_then(Value::as_str)
            .ok_or_else(unsupported)?;
        let id = saved
            .pointer("/grouping/group_by_column_id")
            .and_then(Value::as_str)
            .ok_or_else(unsupported)?;
        let mut columns = doc.columns.iter().filter(|c| c.key == key);
        let original_column = columns.next().ok_or_else(unsupported)?;
        if columns.next().is_some()
            || original_column.id != id
            || !matches!(original_column.kind.as_str(), "select" | "checkbox")
        {
            return Err(unsupported());
        }

        let read = document(doc, local)?;
        let (_, column) = draft_basis(&read, local.view_id())?;
        let provider = provider(doc, local.view_id(), current)?;
        let mut raw = read.raw.clone();
        let view = raw["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == local.view_id())
            .unwrap();
        // Validator-only shared marker. Canonical grouping is restored below.
        view["grouping"]["group_by_column_id"] = json!(column.id);
        let check = Document::parse(raw)?;
        let moved = Self::capture_with_draft(
            &check,
            local.view_id(),
            source,
            target,
            placement,
            provider.as_ref(),
            allow,
            personal,
        )?;
        let mut grouping = saved["grouping"].clone();
        grouping["order"] = moved.grouping["order"].clone();
        if grouping.to_string().len() > 8192 {
            return Err(unsupported());
        }
        let mut expected = doc.raw["list_metadata"].clone();
        expected["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == local.view_id())
            .unwrap()["grouping"] = grouping.clone();
        Ok(Self {
            document: doc.clone(),
            view_id: local.view_id().into(),
            grouping,
            order: moved.order,
            expected_metadata: expected,
            refinement: Some((local.clone(), current.cloned())),
        })
    }
}
impl BoardLaneOrderDraft {
    pub fn moved_refined(
        doc: &Document,
        local: &ViewRefinementDraft,
        current: Option<&Self>,
        source: &str,
        target: &str,
        placement: BoardLanePlacement,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        eligible(doc, allow, personal)?;
        if shared_target(original(doc, local.view_id())?)? {
            return Err(ListError("This Board order requires a shared request."));
        }
        let read = document(doc, local)?;
        let provider = provider(doc, local.view_id(), current)?;
        let mut raw = read.raw.clone();
        raw["list_metadata"]["views"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == local.view_id())
            .unwrap()["grouping"]
            .as_object_mut()
            .ok_or_else(unsupported)?
            .remove("group_by_column_id");
        Self::moved(
            &Document::parse(raw)?,
            local.view_id(),
            provider.as_ref(),
            source,
            target,
            placement,
            allow,
            personal,
        )
    }
}

#[cfg(test)]
mod tests;
