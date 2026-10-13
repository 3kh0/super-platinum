use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedView {
    pub reason: String,
}
impl std::fmt::Display for UnsupportedView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}
impl std::error::Error for UnsupportedView {}
#[derive(Clone, Debug, PartialEq)]
pub struct SavedView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub position: Option<Value>,
    pub default_view_key: Option<String>,
    pub is_all_items_view: bool,
    pub is_template_initial_view: bool,
    pub raw: Value,
}
impl SavedView {
    pub fn parse(raw: Value) -> Result<Self> {
        let string = |key| {
            raw.get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| unsupported(format!("Saved view {key} is missing or malformed.")))
        };
        let position = raw.get("position").filter(|v| !v.is_null()).cloned();
        if position
            .as_ref()
            .is_some_and(|v| !v.is_string() && !v.is_number())
        {
            return Err(unsupported("Saved view position is malformed."));
        }
        Ok(Self {
            id: string("id")?,
            name: string("name")?,
            kind: string("type")?,
            position,
            default_view_key: raw
                .get("default_view_key")
                .and_then(Value::as_str)
                .map(str::to_owned),
            is_all_items_view: raw["is_all_items_view"] == true,
            is_template_initial_view: raw["is_template_initial_view"] == true,
            raw,
        })
    }
}
pub fn catalog(document: &Document) -> Result<Vec<SavedView>> {
    let mut views = document
        .views
        .iter()
        .cloned()
        .map(SavedView::parse)
        .collect::<Result<Vec<_>>>()?;
    let mut identities = BTreeSet::new();
    if views
        .iter()
        .any(|view| !identities.insert(view.id.as_str()))
    {
        return Err(unsupported("Saved view identities are ambiguous."));
    }
    let string_positions = views
        .iter()
        .any(|v| v.position.as_ref().is_some_and(Value::is_string));
    let number_positions = views
        .iter()
        .any(|v| v.position.as_ref().is_some_and(Value::is_number));
    if string_positions && number_positions {
        return Err(unsupported(
            "Saved view positions mix string and numeric representations.",
        ));
    }
    if views
        .iter()
        .filter_map(|v| v.position.as_ref())
        .any(|v| v.is_number() && !v.as_f64().is_some_and(f64::is_finite))
    {
        return Err(unsupported(
            "Saved view position is outside the supported numeric range.",
        ));
    }
    views.sort_by(|a, b| match (&a.position, &b.position) {
        (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(b),
        (Some(a), Some(b)) => a.as_f64().unwrap().total_cmp(&b.as_f64().unwrap()),
        (None, None) => std::cmp::Ordering::Equal,
        (None, _) => std::cmp::Ordering::Greater,
        (_, None) => std::cmp::Ordering::Less,
    });
    Ok(views)
}
pub(super) fn column<'a>(
    document: &'a Document,
    key: &str,
    id: Option<&Value>,
) -> Result<&'a Column> {
    let column = document
        .columns
        .iter()
        .find(|c| c.key == key)
        .ok_or_else(|| unsupported(format!("Saved view references missing column {key}.")))?;
    if id.is_some_and(|id| !id.is_null() && id.as_str() != Some(&column.id)) {
        return Err(unsupported("Saved view column identity changed."));
    }
    Ok(column)
}
pub(super) fn group_column<'a>(document: &'a Document, raw: &Value) -> Result<Option<&'a Column>> {
    let Some(group) = raw.get("grouping").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let group = group
        .as_object()
        .ok_or_else(|| unsupported("Saved grouping is malformed."))?;
    let Some(key) = group
        .get("group_by")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        if group.is_empty() || group.get("group_by").is_some_and(|v| v == "") {
            return Ok(None);
        }
        return Err(unsupported(
            "Saved grouping column is missing or malformed.",
        ));
    };
    let column = column(document, key, group.get("group_by_column_id"))?;
    if !matches!(
        column.kind.as_str(),
        "select" | "checkbox" | "completed" | "todo_completed"
    ) {
        return Err(unsupported(
            "Grouping this field type is not supported yet.",
        ));
    }
    Ok(Some(column))
}
pub(super) fn columns(
    document: &Document,
    raw: &Value,
    group: Option<&Column>,
) -> Result<Vec<String>> {
    let input = raw
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| unsupported("Saved view columns are missing or malformed."))?;
    let mut keys = BTreeSet::new();
    let mut out = Vec::new();
    for entry in input {
        let key = entry
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| unsupported("Saved column key is malformed."))?;
        if !keys.insert(key) {
            return Err(unsupported("Saved view has duplicate column keys."));
        }
        // Captured visibility skips deleted schema keys; a replaced same-key ID
        // is ambiguous and must not acquire the old saved column's identity.
        let Some(column) = document.columns.iter().find(|c| c.key == key) else {
            continue;
        };
        if entry
            .get("id")
            .is_some_and(|id| !id.is_null() && id.as_str() != Some(&column.id))
        {
            return Err(unsupported("Saved column identity changed."));
        }
        if entry.get("visible").is_some_and(|v| !v.is_boolean()) {
            return Err(unsupported("Saved column visibility is malformed."));
        }
        if entry["visible"] == true || group.is_some_and(|g| g.key == key) {
            out.push(column.id.clone());
        }
    }
    Ok(out)
}
#[derive(Debug)]
pub(super) struct Filter {
    pub key: String,
    pub column_id: Option<Value>,
    pub operator: String,
    pub values: Vec<Value>,
}
#[derive(Debug)]
pub(super) struct Sort {
    pub key: String,
    pub column_id: Option<Value>,
    pub ascending: bool,
}
fn array<'a>(raw: &'a Value, key: &str) -> Result<&'a [Value]> {
    match raw.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        _ => Err(unsupported(format!("Saved {key} is malformed."))),
    }
}
pub(super) fn filters(raw: &Value) -> Result<Vec<Filter>> {
    array(raw, "filters")?
        .iter()
        .chain(array(raw, "info_column_filters")?)
        .map(|f| {
            Ok(Filter {
                key: f["key"]
                    .as_str()
                    .ok_or_else(|| unsupported("Filter key is malformed."))?
                    .into(),
                column_id: f.get("column_id").cloned(),
                operator: f["operator"]
                    .as_str()
                    .ok_or_else(|| unsupported("Filter operator is malformed."))?
                    .into(),
                values: f["values"]
                    .as_array()
                    .ok_or_else(|| unsupported("Filter values are malformed."))?
                    .clone(),
            })
        })
        .collect()
}
pub(super) fn sorts(raw: &Value) -> Result<Vec<Sort>> {
    array(raw, "sorts")?
        .iter()
        .map(|s| {
            Ok(Sort {
                key: s["key"]
                    .as_str()
                    .ok_or_else(|| unsupported("Sort key is malformed."))?
                    .into(),
                column_id: s.get("column_id").cloned(),
                ascending: s["ascending"]
                    .as_bool()
                    .ok_or_else(|| unsupported("Sort direction is malformed."))?,
            })
        })
        .collect()
}
