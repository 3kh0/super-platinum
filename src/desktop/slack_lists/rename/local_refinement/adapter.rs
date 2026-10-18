//! Pure control-read projection of source-supported optional rule members.
//! Canonical raw rules remain in Action; this document never authorizes a write.
use super::*;
pub(super) fn rules(doc: &Document, view: &mut Value, kind: Kind) -> Option<()> {
    let key = match kind {
        Kind::Filter => "filters",
        Kind::Sort => "sorts",
        _ => return Some(()),
    };
    let rows = match view.get(key) {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(rows)) => rows.clone(),
        _ => return None,
    };
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let o = row.as_object()?;
        if o.keys().any(|k| {
            if matches!(kind, Kind::Filter) {
                !matches!(
                    k.as_str(),
                    "key" | "operator" | "values" | "typed_values" | "column_id"
                )
            } else {
                !matches!(k.as_str(), "key" | "ascending" | "column_id")
            }
        }) {
            return None;
        }
        let key = row["key"].as_str()?;
        let mut columns = doc.columns.iter().filter(|c| c.key == key);
        let column = columns.next()?;
        if columns.next().is_some()
            || row
                .get("column_id")
                .is_some_and(|id| id.as_str() != Some(&column.id))
        {
            return None;
        }
        if matches!(kind, Kind::Filter) {
            if row
                .get("typed_values")
                .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
            {
                return None;
            }
            let values = match row.get("values") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(a)) if a.iter().all(Value::is_string) => a.clone(),
                _ => return None,
            };
            out.push(json!({"key":key,"operator":row["operator"].as_str()?,"values":values}));
        } else {
            let ascending = match row.get("ascending") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                _ => return None,
            };
            out.push(json!({"key":key,"ascending":ascending}));
        }
    }
    view[key] = json!(out);
    Some(())
}
