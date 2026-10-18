//! Visit-local view refinement data, separate from the Kanban order provider.
//! Source: 8A13 current/dirty selectors, uPpd.Mu/U3 and AZAI local reducers.
//! This type is read state, never a server intent or dispatch permit.
use super::{Document, ListError, ViewCreate, ViewLayout};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewRefinementChanges {
    pub filters: bool,
    pub sorts: bool,
    pub visibility: bool,
    pub layout: bool,
    pub grouping: bool,
    pub completed: bool,
}
impl ViewRefinementChanges {
    /// The separate Kanban provider is deliberately not a Save visibility input.
    pub fn any(&self) -> bool {
        self.filters
            || self.sorts
            || self.visibility
            || self.layout
            || self.grouping
            || self.completed
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewRefinementDraft {
    list_id: String,
    view_id: String,
    filters: Option<Vec<Value>>,
    sorts: Option<Vec<Value>>,
    visibility: BTreeMap<String, bool>,
    layout: Option<ViewLayout>,
    grouping: Option<Value>,
    completed: Option<bool>,
}
fn unsupported() -> ListError {
    ListError("These local view refinements are not supported.")
}
fn view<'a>(doc: &'a Document, id: &str) -> Result<&'a Value, ListError> {
    if Document::parse(doc.raw.clone())? != *doc {
        return Err(ListError("List representations disagree."));
    }
    let mut matching = doc.views.iter().filter(|v| v["id"] == id);
    let v = matching.next().ok_or_else(unsupported)?;
    if matching.next().is_some() || !matches!(v["type"].as_str(), Some("table" | "board")) {
        return Err(unsupported());
    }
    Ok(v)
}
fn array(v: Option<&Value>) -> Result<Vec<Value>, ListError> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) if a.len() <= 1024 => Ok(a.clone()),
        _ => Err(unsupported()),
    }
}
fn normalized(rows: &[Value], filters: bool) -> Result<Value, ListError> {
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let mut row = row.as_object().cloned().ok_or_else(unsupported)?;
        row.remove("column_id");
        if filters {
            for field in ["values", "typed_values"] {
                let values = array(row.get(field))?;
                // Current ordinary filter controls author string values. Arbitrary
                // nested objects/numbers need lossless JS serialization evidence;
                // serde_json cannot recover their original property order or
                // JS number formatting. Preserve those raw read shapes, refusing
                // this local comparison/edit rather than manufacturing equality.
                if !values.iter().all(Value::is_string) {
                    return Err(unsupported());
                }
                // Actual U3 compares the stringified arrays, keeping order.
                row.insert(
                    field.into(),
                    Value::String(Value::Array(values).to_string()),
                );
            }
        } else {
            let ascending = match row.get("ascending") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                _ => return Err(unsupported()),
            };
            row.insert("ascending".into(), json!(ascending));
        }
        result.push(Value::Object(row));
    }
    Ok(Value::Array(result))
}
// afel.A compares Number values after JSON decoding in the web client. Rust
// serde_json distinguishes integer/float representations; retain raw bytes but
// compare finite JS Number values here, including the normal >2^53 rounding.
fn js_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a
            .as_f64()
            .zip(b.as_f64())
            .is_some_and(|(a, b)| a.is_finite() && b.is_finite() && a == b),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| js_equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| js_equal(a, b)))
        }
        _ => a == b,
    }
}
fn grouping_key(v: Option<&Value>) -> Result<Option<&str>, ListError> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => match o.get("group_by") {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            _ => Err(unsupported()),
        },
        _ => Err(unsupported()),
    }
}
impl ViewRefinementDraft {
    pub fn capture(doc: &Document, id: &str) -> Result<Self, ListError> {
        view(doc, id)?;
        Ok(Self {
            list_id: doc.id.clone(),
            view_id: id.into(),
            filters: None,
            sorts: None,
            visibility: BTreeMap::new(),
            layout: None,
            grouping: None,
            completed: None,
        })
    }
    pub fn list_id(&self) -> &str {
        &self.list_id
    }
    pub(crate) fn visibility_overrides(&self) -> &BTreeMap<String, bool> {
        &self.visibility
    }
    pub fn view_id(&self) -> &str {
        &self.view_id
    }
    fn current<'a>(&self, doc: &'a Document) -> Result<&'a Value, ListError> {
        if doc.id != self.list_id {
            return Err(unsupported());
        }
        view(doc, &self.view_id)
    }
    pub fn with_layout(&self, doc: &Document, layout: ViewLayout) -> Result<Self, ListError> {
        self.current(doc)?;
        let mut result = self.clone();
        result.layout = Some(layout);
        Ok(result)
    }
    /// Actual local grouping reducer stores only group_by, dropping no canonical
    /// bytes: this replacement exists solely in the ephemeral read projection.
    pub fn with_grouping(
        &self,
        doc: &Document,
        column_id: Option<&str>,
    ) -> Result<Self, ListError> {
        self.current(doc)?;
        let key = match column_id {
            None => "",
            Some(id) => {
                &ViewCreate::grouping_columns(doc)
                    .find(|c| c.id == id)
                    .ok_or_else(unsupported)?
                    .key
            }
        };
        let mut result = self.clone();
        result.grouping = Some(json!({"group_by":key}));
        Ok(result)
    }
    /// Bounded local read data. Write admission must separately validate each
    /// supported rule and source wire projection; this method grants no permit.
    /// Same validated ordinary choices as the existing shared editor, applied
    /// only to a local read draft; read-only Lists do not gain write authority.
    pub fn with_filter_choice(
        &self,
        doc: &Document,
        choice: &super::ViewFilterChoice,
    ) -> Result<Self, ListError> {
        self.current(doc)?;
        let rows = super::ViewFilter::local_values(doc, choice)?;
        self.with_filters(doc, rows.as_array().ok_or_else(unsupported)?.clone())
    }
    pub fn with_sort_choice(
        &self,
        doc: &Document,
        choice: &super::ViewSortChoice,
    ) -> Result<Self, ListError> {
        self.current(doc)?;
        let rows = super::ViewSort::wire(doc, choice).ok_or_else(unsupported)?;
        self.with_sorts(doc, rows.as_array().ok_or_else(unsupported)?.clone())
    }
    pub fn with_filters(&self, doc: &Document, filters: Vec<Value>) -> Result<Self, ListError> {
        self.current(doc)?;
        Self::rules(doc, &filters, true)?;
        let mut result = self.clone();
        result.filters = Some(filters);
        Ok(result)
    }
    pub fn with_sorts(&self, doc: &Document, sorts: Vec<Value>) -> Result<Self, ListError> {
        self.current(doc)?;
        Self::rules(doc, &sorts, false)?;
        let mut result = self.clone();
        result.sorts = Some(sorts);
        Ok(result)
    }
    fn rules(doc: &Document, rows: &[Value], filters: bool) -> Result<(), ListError> {
        if rows.len() > 1024
            || serde_json::to_vec(rows).map_err(|_| unsupported())?.len() > 64 * 1024
        {
            return Err(unsupported());
        }
        normalized(rows, filters)?;
        for row in rows {
            let key = row["key"].as_str().ok_or_else(unsupported)?;
            if !doc.columns.iter().any(|c| c.key == key) {
                return Err(unsupported());
            }
            if filters && !row.get("operator").is_some_and(Value::is_string) {
                return Err(unsupported());
            }
        }
        Ok(())
    }
    pub fn with_visibility(
        &self,
        doc: &Document,
        column_id: &str,
        visible: bool,
    ) -> Result<Self, ListError> {
        let raw = self.current(doc)?;
        let column = doc.column(column_id).ok_or_else(unsupported)?;
        let columns = raw["columns"].as_array().ok_or_else(unsupported)?;
        if columns.iter().filter(|v| v["key"] == column.key).count() != 1 {
            return Err(unsupported());
        }
        let mut result = self.clone();
        result.visibility.insert(column.key.clone(), visible);
        Ok(result)
    }
    pub fn with_completed(&self, doc: &Document, show: bool) -> Result<Self, ListError> {
        self.current(doc)?;
        let mut result = self.clone();
        result.completed = Some(show);
        Ok(result)
    }
    pub fn changes(&self, doc: &Document) -> Result<ViewRefinementChanges, ListError> {
        let raw = self.current(doc)?;
        let mut changes = ViewRefinementChanges::default();
        if let Some(local) = &self.filters {
            let mut saved = array(raw.get("filters"))?;
            saved.extend(array(raw.get("info_column_filters"))?);
            changes.filters = !js_equal(&normalized(local, true)?, &normalized(&saved, true)?);
        }
        if let Some(local) = &self.sorts {
            changes.sorts = !js_equal(
                &normalized(local, false)?,
                &normalized(&array(raw.get("sorts"))?, false)?,
            );
        }
        changes.layout = self
            .layout
            .is_some_and(|layout| raw["type"] != layout.as_str());
        if let Some(local) = &self.grouping {
            let a = grouping_key(Some(local))?;
            let b = grouping_key(raw.get("grouping"))?;
            changes.grouping =
                (a.is_some_and(|s| !s.is_empty()) || b.is_some_and(|s| !s.is_empty())) && a != b;
        }
        if let Some(local) = self.completed {
            let saved = match raw.get("show_completed_items") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                _ => return Err(unsupported()),
            };
            changes.completed = local != saved;
        }
        if !self.visibility.is_empty() {
            let columns = raw["columns"].as_array().ok_or_else(unsupported)?;
            for (key, visible) in &self.visibility {
                let mut matches = columns.iter().filter(|v| v["key"] == *key);
                let column = matches.next().ok_or_else(unsupported)?;
                if matches.next().is_some() {
                    return Err(unsupported());
                }
                changes.visibility |= column.get("visible") != Some(&json!(visible));
            }
        }
        Ok(changes)
    }
    /// Never replaces canonical Loaded.document. The host must run its actual
    /// projector/context and reject unsupported draft projections before display.
    pub fn projection_document(&self, doc: &Document) -> Result<Document, ListError> {
        self.current(doc)?;
        let mut raw = doc.raw.clone();
        let target = raw["list_metadata"]["views"]
            .as_array_mut()
            .ok_or_else(unsupported)?
            .iter_mut()
            .find(|v| v["id"] == self.view_id)
            .ok_or_else(unsupported)?;
        if let Some(layout) = self.layout {
            target["type"] = json!(layout.as_str());
        }
        if let Some(grouping) = &self.grouping {
            target["grouping"] = grouping.clone();
        }
        if let Some(filters) = &self.filters {
            target["filters"] = json!(filters);
            target["info_column_filters"] = json!([]);
        }
        if let Some(sorts) = &self.sorts {
            target["sorts"] = json!(sorts);
        }
        if let Some(completed) = self.completed {
            target["show_completed_items"] = json!(completed);
        }
        if !self.visibility.is_empty() {
            for column in target["columns"].as_array_mut().ok_or_else(unsupported)? {
                if let Some(visible) = column["key"]
                    .as_str()
                    .and_then(|key| self.visibility.get(key))
                {
                    column["visible"] = json!(visible);
                }
            }
        }
        Document::parse(raw)
    }
}

#[cfg(test)]
mod tests;
