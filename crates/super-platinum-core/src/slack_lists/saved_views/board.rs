//! Captured BoardView B17959600, Q81I B18314600, 5QWf.B_ B2839299.
//! Board lane ordering/visibility is distinct from the table grouping policy.
use super::*;
use evaluate::{Atom, atom, choices};

pub(super) fn columns(
    document: &Document,
    view: &SavedView,
    grouping: Option<&Column>,
    context: &ProjectionContext,
) -> Result<(Vec<String>, Option<String>)> {
    let mut columns = model::columns(document, &view.raw, None)?;
    // czID promotes an already-visible primary; it does not append hidden fields.
    if let Some(index) = columns.iter().position(|id| {
        document
            .column(id)
            .is_some_and(|c| c.raw["is_primary_column"] == true)
    }) {
        let id = columns.remove(index);
        columns.insert(0, id);
    }
    if let Some(group) = grouping {
        columns.retain(|id| id != &group.id);
    }
    let primary_visible = columns.iter().any(|id| {
        document
            .column(id)
            .is_some_and(|c| c.raw["is_primary_column"] == true)
    });
    let completion = if context.todo_mode && context.apply_todo_completion_rules == Some(true) {
        let completion = primary_visible
            .then(|| {
                document
                    .columns
                    .iter()
                    .find(|c| c.key == "todo_completed" && c.kind == "todo_completed")
            })
            .flatten()
            .map(|c| c.id.clone());
        // BoardView's todo renderer suppresses the ordinary completion field;
        // its checkbox is rendered alongside the visible primary field instead.
        columns.retain(|id| {
            document
                .column(id)
                .is_none_or(|c| c.kind != "todo_completed")
        });
        completion
    } else {
        None
    };
    Ok((columns, completion))
}
fn lane(key: String, label: String, rows: Vec<&Record>) -> Group {
    Group {
        count: rows.len(),
        rows: rows
            .into_iter()
            .map(|row| row_instance(Some(&key), row))
            .collect(),
        key: Some(key),
        label,
        collapsed: false,
    }
}
fn ordered_keys(view: &SavedView, column: &Column) -> Result<Vec<String>> {
    let Some(raw) = view.raw.pointer("/grouping/order").filter(|v| !v.is_null()) else {
        return Ok(Vec::new());
    };
    let order = raw
        .as_array()
        .ok_or_else(|| unsupported("Board lane order is malformed."))?;
    order
        .iter()
        .map(|cell| {
            if !cell.is_object() {
                return Err(unsupported("Board lane order is not typed cell data."));
            }
            if column.kind == "select" {
                match cell.get("select") {
                    None | Some(Value::Null) => Ok(String::new()),
                    Some(Value::Array(values)) => match values.first() {
                        None | Some(Value::Null) => Ok(String::new()),
                        Some(Value::String(id)) => Ok(id.clone()),
                        _ => Err(unsupported("Board select lane identity is malformed.")),
                    },
                    _ => Err(unsupported("Board select lane identity is ambiguous.")),
                }
            } else {
                match cell.get("checkbox") {
                    None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(String::new()),
                    Some(Value::Bool(true)) => Ok("true".into()),
                    _ => Err(unsupported("Board checkbox lane identity is malformed.")),
                }
            }
        })
        .collect()
}
pub(super) fn project(
    column: Option<&Column>,
    view: &SavedView,
    records: Vec<&Record>,
    filters: &[model::Filter],
    sorts: &[model::Sort],
    context: &ProjectionContext,
) -> Result<Vec<Group>> {
    let Some(column) = column else {
        return Ok(if records.is_empty() {
            Vec::new()
        } else {
            vec![lane(String::new(), "Ungrouped".into(), records)]
        });
    };
    if !matches!(
        column.kind.as_str(),
        "select" | "checkbox" | "todo_completed"
    ) {
        return Err(unsupported(
            "This Board grouping field is not supported yet.",
        ));
    }
    let todo = column.kind == "todo_completed";
    let mut lanes: Vec<(String, String, Vec<&Record>)> = if column.kind == "select" {
        choices(column)?
            .into_iter()
            .map(|(id, label)| (id, label, Vec::new()))
            .collect()
    } else {
        vec![(
            "true".into(),
            if todo { "Completed" } else { "Checked" }.into(),
            Vec::new(),
        )]
    };
    if column.kind == "select"
        && let Some(sort) = sorts.iter().find(|s| s.key == column.key)
    {
        lanes.sort_by(|a, b| {
            if sort.ascending {
                a.1.encode_utf16().cmp(b.1.encode_utf16())
            } else {
                b.1.encode_utf16().cmp(a.1.encode_utf16())
            }
        });
    }
    lanes.insert(
        0,
        (
            String::new(),
            if todo {
                "Open"
            } else if column.kind == "checkbox" {
                "Unchecked"
            } else {
                "Ungrouped"
            }
            .into(),
            Vec::new(),
        ),
    );
    let order = ordered_keys(view, column)?;
    let mut ordered = Vec::with_capacity(lanes.len());
    for key in order {
        if let Some(index) = lanes.iter().position(|lane| lane.0 == key) {
            ordered.push(lanes.remove(index));
        }
    }
    ordered.extend(lanes);
    let indices = ordered
        .iter()
        .enumerate()
        .map(|(i, lane)| (lane.0.clone(), i))
        .collect::<BTreeMap<_, _>>();
    let empty = indices[""];
    for record in records {
        let value = atom(column, record, context, false)?;
        let keys = if column.kind == "select" {
            let value = value.text();
            // Hs only splits multi-select fields. A canonical single selection
            // is one opaque value, never an inferred first item from a split.
            if column.is_multi_select() {
                value
                    .split('|')
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
                    .collect::<BTreeSet<_>>()
            } else if value.is_empty() {
                BTreeSet::new()
            } else {
                BTreeSet::from([value])
            }
        } else if matches!(value, Atom::Boolean(true)) {
            BTreeSet::from(["true".into()])
        } else {
            BTreeSet::new()
        };
        let mut inserted = false;
        for key in keys {
            if !key.is_empty()
                && let Some(index) = indices.get(&key)
            {
                ordered[*index].2.push(record);
                inserted = true;
            }
        }
        if !inserted {
            ordered[empty].2.push(record);
        }
    }
    Ok(ordered
        .into_iter()
        .filter(|(key, _, rows)| {
            if key.is_empty() {
                return !rows.is_empty();
            }
            // Q81I hides a lane rejected by a filter for this grouping field even
            // when it contains a card also assigned to another matching lane.
            !filters.iter().any(|f| f.key == column.key)
                || grouping::show_empty_group(filters, column, key)
        })
        .map(|(key, label, rows)| lane(key, label, rows))
        .collect())
}
#[cfg(test)]
mod tests;
