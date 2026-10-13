use super::*;
use evaluate::{Atom, atom, choices};
pub(super) fn group(
    key: Option<String>,
    label: String,
    records: Vec<&Record>,
    context: &ProjectionContext,
) -> Group {
    let collapsed = key
        .as_ref()
        .is_some_and(|key| context.collapsed_groups.contains(key));
    Group {
        count: records.len(),
        rows: if collapsed {
            Vec::new()
        } else {
            records
                .iter()
                .map(|r| row_instance(key.as_deref(), r))
                .collect()
        },
        key,
        label,
        collapsed,
    }
}
pub(super) fn project(
    column: Option<&Column>,
    records: Vec<&Record>,
    filters: &[model::Filter],
    sorts: &[model::Sort],
    context: &ProjectionContext,
) -> Result<Vec<Group>> {
    let Some(column) = column else {
        return Ok(vec![group(None, String::new(), records, context)]);
    };
    let mut groups: Vec<(String, String, Vec<&Record>)> = if column.kind == "select" {
        choices(column)?
            .into_iter()
            .map(|(id, label)| (id, label, Vec::new()))
            .collect()
    } else {
        Vec::new()
    };
    if column.kind == "select"
        && let Some(sort) = sorts.iter().find(|s| s.key == column.key)
    {
        groups.sort_by(|a, b| {
            if sort.ascending {
                a.1.cmp(&b.1)
            } else {
                b.1.cmp(&a.1)
            }
        });
    }
    let mut ungrouped = Vec::new();
    for record in records {
        let value = atom(column, record, context, false)?;
        if column.kind == "select" {
            let value = value.text();
            let mut ids = value
                .split('|')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if !column.is_multi_select() {
                ids.truncate(1)
            }
            let mut inserted = false;
            for (id, _, rows) in &mut groups {
                if ids.contains(&id.as_str()) {
                    rows.push(record);
                    inserted = true
                }
            }
            if !inserted {
                ungrouped.push(record)
            }
        } else if matches!(value, Atom::Boolean(true)) {
            // Captured grid grouping treats false/missing as Ungrouped, not a
            // second named false group. Do not apply Board grouping.order here.
            if groups.is_empty() {
                groups.push(("true".into(), "Checked".into(), Vec::new()));
            }
            groups[0].2.push(record);
        } else {
            ungrouped.push(record)
        }
    }
    // B useGetRowItems: writable grids show empty groups unless active filters
    // omit that group. Read-only grids omit them. +E6Z.vx uses the first filter
    // for the grouping key, not arbitrary filters on other fields.
    let mut output = groups
        .into_iter()
        .filter(|(key, _, rows)| {
            !rows.is_empty() || (context.editable && show_empty_group(filters, column, key))
        })
        .map(|(key, label, rows)| group(Some(key), label, rows, context))
        .collect::<Vec<_>>();
    if !ungrouped.is_empty() {
        output.push(group(
            Some(String::new()),
            "Ungrouped".into(),
            ungrouped,
            context,
        ));
    }
    Ok(output)
}

pub(super) fn show_empty_group(filters: &[model::Filter], column: &Column, key: &str) -> bool {
    if filters.is_empty() {
        return true;
    }
    let Some(filter) = filters.iter().find(|filter| filter.key == column.key) else {
        return false;
    };
    let includes = filter
        .values
        .iter()
        .any(|value| value.as_str() == Some(key));
    if includes {
        matches!(filter.operator.as_str(), "equal" | "contains" | "is_me")
    } else {
        matches!(
            filter.operator.as_str(),
            "not_equal"
                | "does_not_contain"
                | "greater_than"
                | "less_than"
                | "greater_than_or_equal"
                | "less_than_or_equal"
                | "is_not_me"
        )
    }
}
