use super::*;
use std::collections::BTreeSet;

fn array(value: Option<&Value>) -> Result<Vec<Value>, ListError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) if a.len() <= 1024 => Ok(a.clone()),
        _ => Err(unsupported()),
    }
}
fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn column<'a>(doc: &'a Document, key: &str) -> Result<&'a super::super::Column, ListError> {
    let mut matches = doc.columns.iter().filter(|c| c.key == key);
    let c = matches.next().ok_or_else(unsupported)?;
    if matches.next().is_some() || !identity(&c.id) || !identity(&c.key) {
        return Err(unsupported());
    }
    Ok(c)
}
/// Validate through established ordinary filter/sort semantics using a temporary
/// Table read adapter. This never grants its edited document write authority.
fn rules(doc: &Document, view: &Value) -> Result<(Value, Value), ListError> {
    if view.get("filter_set").is_some_and(|v| !v.is_null())
        || !array(view.get("info_column_filters"))?.is_empty()
    {
        return Err(unsupported());
    }
    let filters = array(view.get("filters"))?;
    let sorts = array(view.get("sorts"))?;
    let mut filter_wire = Vec::new();
    let mut filter_check = Vec::new();
    for row in filters {
        let object = row.as_object().ok_or_else(unsupported)?;
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "key" | "operator" | "values" | "typed_values" | "column_id"
            )
        }) || !array(row.get("typed_values"))?.is_empty()
        {
            return Err(unsupported());
        }
        let key = row["key"].as_str().ok_or_else(unsupported)?;
        let c = column(doc, key)?;
        if row
            .get("column_id")
            .is_some_and(|id| id.as_str() != Some(&c.id))
        {
            return Err(unsupported());
        }
        // RO preserves missing/null values rather than materializing []. This
        // ordinary authoring cohort requires the explicit array emitted by our
        // existing controls; comparison defaults are not wire normalization.
        let values = row
            .get("values")
            .and_then(Value::as_array)
            .ok_or_else(unsupported)?
            .clone();
        if !values.iter().all(Value::is_string) || !row["operator"].is_string() {
            return Err(unsupported());
        }
        let check = json!({"key":key,"operator":row["operator"],"values":values});
        filter_check.push(check.clone());
        // Actual +E6Z.RO projects only these members; optional column_id remains
        // absent if the source rule had none, rather than inventing one.
        let mut wire = check;
        if let Some(id) = row.get("column_id") {
            wire["column_id"] = id.clone();
        }
        filter_wire.push(wire);
    }
    let mut sort_check = Vec::new();
    for row in &sorts {
        let object = row.as_object().ok_or_else(unsupported)?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "key" | "ascending" | "column_id"))
        {
            return Err(unsupported());
        }
        let key = row["key"].as_str().ok_or_else(unsupported)?;
        let c = column(doc, key)?;
        if row
            .get("column_id")
            .is_some_and(|id| id.as_str() != Some(&c.id))
        {
            return Err(unsupported());
        }
        let ascending = match row.get("ascending") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            _ => return Err(unsupported()),
        };
        sort_check.push(json!({"key":key,"ascending":ascending}));
    }
    let mut raw = doc.raw.clone();
    let check = raw["list_metadata"]["views"]
        .as_array_mut()
        .ok_or_else(unsupported)?
        .iter_mut()
        .find(|v| v["id"] == view["id"])
        .ok_or_else(unsupported)?;
    check["type"] = json!("table");
    // Built-in source view may Save-as-new. This scratch copy only invokes pure
    // ordinary rule validation, never eligibility or a request constructor.
    for flag in ["default_view_key", "is_all_items_view", "is_locked"] {
        check.as_object_mut().ok_or_else(unsupported)?.remove(flag);
    }
    check["filters"] = json!(filter_check);
    check["sorts"] = json!(sort_check);
    check["info_column_filters"] = json!([]);
    let parsed = Document::parse(raw)?;
    let id = view["id"].as_str().ok_or_else(unsupported)?;
    if super::super::ViewFilter::current(&parsed, id).is_none()
        || super::super::ViewSort::current(&parsed, id).is_none()
    {
        return Err(unsupported());
    }
    Ok((json!(filter_wire), json!(sorts)))
}
pub(super) fn build(
    doc: &Document,
    effective: &Document,
    draft: &ViewRefinementDraft,
    current: &Value,
    view: &Value,
    provider: Option<&BoardLaneOrderDraft>,
    destination: &ViewRefinementDestination,
) -> Result<(Vec<(String, String)>, Value), ListError> {
    let (filters, sorts) = rules(effective, view)?;
    let completed = match view.get("show_completed_items") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        _ => return Err(unsupported()),
    };
    let mut grouping = match view.get("grouping") {
        None | Some(Value::Null) => None,
        Some(Value::Object(_)) => Some(view["grouping"].clone()),
        _ => return Err(unsupported()),
    };
    if let Some(grouping) = &grouping {
        if let Some(key) = grouping.get("group_by") {
            let key = key.as_str().ok_or_else(unsupported)?;
            if !key.is_empty() {
                let c = column(effective, key)?;
                if !super::super::ViewCreate::grouping_columns(effective).any(|g| g.id == c.id)
                    || grouping
                        .get("group_by_column_id")
                        .is_some_and(|id| !id.is_null() && id != "" && id.as_str() != Some(&c.id))
                {
                    return Err(unsupported());
                }
            }
        }
    }
    if let Some(provider) = provider {
        // Exact source merge applies also to Table or ungrouped refinements.
        // The provider has private typed construction and is retained verbatim.
        let grouping = grouping.get_or_insert_with(|| json!({}));
        grouping["order"] = json!(provider.typed_order());
    }
    let mut expected = if *destination == ViewRefinementDestination::Existing {
        current.clone()
    } else {
        json!({})
    };
    let mut fields: Vec<(String, String)> = vec![("list_id".into(), doc.id.clone())];
    match destination {
        ViewRefinementDestination::Existing => fields.push(("id".into(), draft.view_id().into())),
        ViewRefinementDestination::New { name } => {
            fields.push(("name".into(), name.clone()));
            expected["name"] = json!(name);
        }
    }
    let kind = view["type"].as_str().ok_or_else(unsupported)?;
    fields.push(("type".into(), kind.into()));
    expected["type"] = json!(kind);
    let existing = *destination == ViewRefinementDestination::Existing;
    if !existing {
        if let Some(value) = &grouping {
            fields.push(("grouping".into(), value.to_string()));
            expected["grouping"] = value.clone();
        }
    }
    if existing || !filters.as_array().ok_or_else(unsupported)?.is_empty() {
        fields.push(("filters".into(), filters.to_string()));
        expected["filters"] = filters;
    }
    if existing {
        fields.push(("info_column_filters".into(), "[]".into()));
        expected["info_column_filters"] = json!([]);
    }
    if existing || !sorts.as_array().ok_or_else(unsupported)?.is_empty() {
        fields.push(("sorts".into(), sorts.to_string()));
        expected["sorts"] = sorts;
    }
    // Source q contains local visibility overrides, including equal overrides.
    // Derive changed columns from the immutable draft, not from differences alone.
    let visibility = draft.visibility_overrides();
    if !existing || !visibility.is_empty() {
        let columns = view["columns"].as_array().ok_or_else(unsupported)?;
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut wire = Vec::new();
        for value in columns {
            let key = value["key"].as_str().ok_or_else(unsupported)?;
            let c = column(effective, key)?;
            if !ids.insert(&c.id)
                || !keys.insert(&c.key)
                || value.get("id").is_some_and(|id| id.as_str() != Some(&c.id))
            {
                return Err(unsupported());
            }
            if existing {
                if let Some(visible) = visibility.get(key) {
                    wire.push(json!({"key":key,"visible":visible,"id":c.id}));
                }
            } else {
                if value.get("id").and_then(Value::as_str) != Some(&c.id) {
                    return Err(unsupported());
                }
                let visible = match value.get("visible") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(b)) => *b,
                    _ => return Err(unsupported()),
                };
                let mut out = json!({"id":c.id,"visible":visible});
                for member in ["width", "should_wrap_text", "position"] {
                    if let Some(v) = value.get(member) {
                        let valid = match member {
                            "width" => v.as_f64().is_some_and(|n| n.is_finite() && n > 0.0),
                            "should_wrap_text" => v.is_boolean(),
                            _ => v.is_string() || v.is_number(),
                        };
                        if !valid {
                            return Err(unsupported());
                        }
                        out[member] = v.clone();
                    }
                }
                wire.push(out);
            }
        }
        if existing && wire.len() != visibility.len() {
            return Err(unsupported());
        }
        if !wire.is_empty() {
            fields.push(("columns".into(), json!(wire).to_string()));
        }
        if existing {
            expected["columns"] = view["columns"].clone();
        } else {
            expected["columns"] = json!(wire);
        }
    }
    if existing {
        if let Some(grouping) = grouping {
            fields.push(("grouping".into(), grouping.to_string()));
            expected["grouping"] = grouping;
        }
    }
    fields.push(("show_completed_items".into(), completed.to_string()));
    expected["show_completed_items"] = json!(completed);
    fields.push((
        "_x_reason".into(),
        if existing {
            "update-view"
        } else {
            "create-view"
        }
        .into(),
    ));
    if fields.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() > 64 * 1024 {
        return Err(unsupported());
    }
    Ok((fields, expected))
}
