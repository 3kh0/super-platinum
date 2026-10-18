//! Explicit Save refinements / Save as new. Separate from blank-view creation.
//! One metadata effect only: selected-view preference needs its own attempt.
use super::{
    BoardLaneOrderDraft, Document, ListError, PersonalTaskLists, ViewCreate, ViewLayout,
    ViewRefinementDraft, ViewRename,
};
use serde_json::{Value, json};
mod acknowledgement;
mod attempt;
pub use acknowledgement::ViewRefinementAcknowledgement;
mod payload;
pub use attempt::{ViewRefinementSaveAttempt, ViewRefinementSavePermit, ViewRefinementSavePhase};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewRefinementDestination {
    Existing,
    New { name: String },
}
#[derive(Clone, Debug, PartialEq)]
pub struct ViewRefinementSave {
    document: Document,
    draft: ViewRefinementDraft,
    provider: Option<BoardLaneOrderDraft>,
    destination: ViewRefinementDestination,
    fields: Vec<(String, String)>,
    expected_view: Value,
    expected_metadata: Option<Value>,
}
fn unsupported() -> ListError {
    ListError("These refinements cannot be saved in this view.")
}
fn eligible(
    doc: &Document,
    allow: Option<&str>,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    if Document::parse(doc.raw.clone())? != *doc {
        return Err(unsupported());
    }
    if let Some(reason) = doc.read_only_reason(allow) {
        return Err(ListError(reason));
    }
    if personal.is_none_or(|p| p.contains(&doc.id)) {
        return Err(ListError("Ordinary List preferences are not verified."));
    }
    Ok(())
}
impl ViewRefinementSave {
    pub fn capture(
        doc: &Document,
        draft: &ViewRefinementDraft,
        provider: Option<&BoardLaneOrderDraft>,
        destination: ViewRefinementDestination,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Self, ListError> {
        eligible(doc, allow, personal)?;
        if !draft.changes(doc)?.any() {
            return Err(ListError("Change a refinement before saving this view."));
        }
        let effective = draft.projection_document(doc)?;
        let current = doc
            .views
            .iter()
            .find(|v| v["id"] == draft.view_id())
            .ok_or_else(unsupported)?;
        let view = effective
            .views
            .iter()
            .find(|v| v["id"] == draft.view_id())
            .ok_or_else(unsupported)?;
        let layout = match view["type"].as_str() {
            Some("table") => ViewLayout::Table,
            Some("board") => ViewLayout::Board,
            _ => return Err(unsupported()),
        };
        match &destination {
            ViewRefinementDestination::Existing => {
                if !ViewRename::supports(current) {
                    return Err(unsupported());
                }
            }
            ViewRefinementDestination::New { name } => {
                ViewCreate::capture(doc, name, layout, allow, personal)?;
            }
        }
        // Even a layout-only Save resends current filters/sorts. Reject unsupported
        // semantics before constructing any request; no flattened grouped filter.
        let (fields, expected_view) = payload::build(
            doc,
            &effective,
            draft,
            current,
            view,
            provider,
            &destination,
        )?;
        let expected_metadata = if destination == ViewRefinementDestination::Existing {
            let mut metadata = doc.raw["list_metadata"].clone();
            *metadata["views"]
                .as_array_mut()
                .ok_or_else(unsupported)?
                .iter_mut()
                .find(|v| v["id"] == draft.view_id())
                .ok_or_else(unsupported)? = expected_view.clone();
            Some(metadata)
        } else {
            None
        };
        Ok(Self {
            document: doc.clone(),
            draft: draft.clone(),
            provider: provider.cloned(),
            destination,
            fields,
            expected_view,
            expected_metadata,
        })
    }
    pub fn original(&self) -> &Document {
        &self.document
    }
    pub fn draft(&self) -> &ViewRefinementDraft {
        &self.draft
    }
    pub fn provider(&self) -> Option<&BoardLaneOrderDraft> {
        self.provider.as_ref()
    }
    pub fn list_id(&self) -> &str {
        &self.document.id
    }
    pub fn view_id(&self) -> &str {
        self.draft.view_id()
    }
    pub fn destination(&self) -> &ViewRefinementDestination {
        &self.destination
    }
    pub fn method(&self) -> &'static str {
        match self.destination {
            ViewRefinementDestination::Existing => "lists.views.update",
            ViewRefinementDestination::New { .. } => "lists.views.create",
        }
    }
    /// Source-built fields, excluding the session token added by rest_form.
    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
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
                "This List changed. Review the refinements again.",
            ));
        }
        let captured = Self::capture(
            fresh,
            &self.draft,
            self.provider.as_ref(),
            self.destination.clone(),
            allow,
            personal,
        )?;
        if captured.fields != self.fields || captured.expected_view != self.expected_view {
            return Err(unsupported());
        }
        Ok(())
    }
    /// Exact current metadata, never causal attribution or preference confirmation.
    pub fn observed_view_id<'a>(&'a self, fresh: &'a Document) -> Option<&'a str> {
        if Document::parse(fresh.raw.clone()).ok().as_ref() != Some(fresh)
            || fresh.id != self.document.id
            || ["created", "user"]
                .iter()
                .any(|k| fresh.raw.get(*k) != self.document.raw.get(*k))
        {
            return None;
        }
        if let Some(expected) = &self.expected_metadata {
            return (fresh.raw.get("list_metadata") == Some(expected)).then_some(self.view_id());
        }
        let before = &self.document.views;
        if fresh.views.len() != before.len().checked_add(1)? {
            return None;
        }
        let mut new = fresh
            .views
            .iter()
            .filter(|v| !before.iter().any(|b| b["id"] == v["id"]));
        let added = new.next()?;
        if new.next().is_some() || !ViewRename::supports(added) {
            return None;
        }
        if added.get("filter_set").is_some_and(|v| !v.is_null())
            || added
                .get("info_column_filters")
                .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return None;
        }
        for key in ["filters", "sorts"] {
            if self.expected_view.get(key).is_none()
                && added
                    .get(key)
                    .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
            {
                return None;
            }
        }
        if self.expected_view.get("grouping").is_none()
            && added
                .get("grouping")
                .is_some_and(|v| !v.is_null() && v.as_object().is_none_or(|o| !o.is_empty()))
        {
            return None;
        }
        // Server supplies ID/position and may add creation metadata. Every requested
        // field must match exactly; siblings and all other List metadata stay exact.
        for (key, value) in self.expected_view.as_object()? {
            if key == "columns" {
                let requested = value.as_array()?;
                let actual = added.get(key)?.as_array()?;
                if actual.len() != requested.len() {
                    return None;
                }
                for (wanted, current) in requested.iter().zip(actual) {
                    for (key, value) in wanted.as_object()? {
                        if current.get(key) != Some(value) {
                            return None;
                        }
                    }
                    let id = wanted["id"].as_str()?;
                    let column = self.document.column(id)?;
                    if current
                        .get("key")
                        .is_some_and(|key| key.as_str() != Some(&column.key))
                    {
                        return None;
                    }
                }
            } else if added.get(key) != Some(value) {
                return None;
            }
        }
        let id = added["id"].as_str()?;
        let mut metadata = fresh.raw.get("list_metadata")?.clone();
        metadata["views"] = Value::Array(
            fresh
                .views
                .iter()
                .filter(|v| v["id"] != id)
                .cloned()
                .collect(),
        );
        (self.document.raw.get("list_metadata") == Some(&metadata)).then_some(id)
    }
}
#[cfg(test)]
mod tests;
