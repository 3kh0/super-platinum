use super::*;
use model::{Filter, Sort};
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub(super) enum Atom {
    Empty,
    Boolean(bool),
    Number(f64),
    Text(String),
    RichText(String),
    Values(Vec<String>),
}
impl Atom {
    fn empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Boolean(v) => !*v,
            Self::Number(_) => false,
            Self::Text(s) => s.is_empty() || s == "false",
            Self::RichText(_) => false,
            Self::Values(v) => v.is_empty() || v.join("|").is_empty() || v.join("|") == "false",
        }
    }
    pub(super) fn text(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Boolean(v) => v.to_string(),
            Self::Number(v) => v.to_string(),
            Self::Text(v) | Self::RichText(v) => v.clone(),
            Self::Values(v) => v.join("|"),
        }
    }
}
pub(super) fn atom(
    column: &Column,
    record: &Record,
    context: &ProjectionContext,
    sorting: bool,
) -> Result<Atom> {
    let Some(field) = record.fields.get(&column.key) else {
        return Ok(Atom::Empty);
    };
    if field
        .get("column_id")
        .is_some_and(|id| id.as_str() != Some(&column.id))
        || field
            .get("row_id")
            .is_some_and(|id| id.as_str() != Some(&record.id))
    {
        return Err(unsupported(
            "Cell identity does not match the saved-view column.",
        ));
    }
    let member = match column.kind.as_str() {
        "text" | "rich_text" => "rich_text",
        "number" | "currency" | "rating" => "number",
        "checkbox" | "completed" | "todo_completed" => "checkbox",
        "select" | "multi_select" => "select",
        "user" | "assignee" | "todo_assignee" => "user",
        "channel" => "channel",
        "email" => "email",
        "phone" => "phone",
        _ => {
            return Err(unsupported(format!(
                "Saved-view rules for {} fields are not supported yet.",
                column.kind
            )));
        }
    };
    let member = if column.kind == "rating" {
        "rating"
    } else {
        member
    };
    let raw = field.get(member).or_else(|| {
        if member == "rich_text" {
            field.get("richText")
        } else {
            None
        }
    });
    if member == "rich_text" {
        if !sorting && let Some(text) = record.fallback(&column.key).filter(|s| !s.is_empty()) {
            return Ok(Atom::Text(text.into()));
        }
        return match raw {
            None | Some(Value::Null) => Ok(Atom::Empty),
            Some(Value::Array(blocks)) => {
                if blocks.is_empty() {
                    return Ok(Atom::Empty);
                }
                validate_rich(blocks, 0, &mut 2048, &mut 65536)?;
                let mut remaining = 2048;
                Ok(Atom::RichText(first_text(
                    blocks,
                    context,
                    sorting,
                    0,
                    &mut remaining,
                )?))
            }
            _ => Err(unsupported(
                "Rich text for saved-view projection is malformed.",
            )),
        };
    }
    let Some(raw) = raw.filter(|v| !v.is_null()) else {
        return Ok(Atom::Empty);
    };
    if member == "checkbox" {
        return raw
            .as_bool()
            .map(Atom::Boolean)
            .ok_or_else(|| unsupported("Checkbox value is malformed."));
    }
    let values = raw
        .as_array()
        .ok_or_else(|| unsupported("Saved-view field array is malformed."))?;
    if values.is_empty() {
        return Ok(Atom::Empty);
    }
    if matches!(member, "number" | "rating") {
        if values.len() != 1 {
            return Err(unsupported("Numeric field has ambiguous values."));
        }
        return values[0]
            .as_f64()
            .filter(|n| n.is_finite())
            .map(Atom::Number)
            .ok_or_else(|| unsupported("Numeric field is malformed."));
    }
    values
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| unsupported("Saved-view field values are malformed."))
        })
        .collect::<Result<Vec<_>>>()
        .map(Atom::Values)
}
fn validate_rich(
    nodes: &[Value],
    depth: usize,
    nodes_left: &mut usize,
    bytes_left: &mut usize,
) -> Result<()> {
    if depth > 32 || nodes.len() > *nodes_left {
        return Err(unsupported(
            "Rich text exceeds saved-view projection limits.",
        ));
    }
    *nodes_left -= nodes.len();
    for node in nodes {
        let Some(object) = node.as_object() else {
            return Err(unsupported("Rich text node is malformed."));
        };
        if !matches!(
            node["type"].as_str(),
            Some(
                "rich_text"
                    | "rich_text_section"
                    | "rich_text_list"
                    | "rich_text_quote"
                    | "rich_text_preformatted"
                    | "text"
                    | "link"
                    | "emoji"
                    | "user"
                    | "channel"
                    | "usergroup"
                    | "broadcast"
                    | "date"
                    | "color"
            )
        ) {
            return Err(unsupported(
                "Opaque rich text cannot be projected into saved-view rules.",
            ));
        }
        for text in object.values().filter_map(Value::as_str) {
            if text.len() > *bytes_left {
                return Err(unsupported(
                    "Rich text exceeds saved-view projection limits.",
                ));
            }
            *bytes_left -= text.len();
        }
        if let Some(children) = node.get("elements") {
            let children = children
                .as_array()
                .ok_or_else(|| unsupported("Rich text children are malformed."))?;
            validate_rich(children, depth + 1, nodes_left, bytes_left)?;
        }
    }
    Ok(())
}
fn first_text(
    nodes: &[Value],
    context: &ProjectionContext,
    labels: bool,
    depth: usize,
    remaining: &mut usize,
) -> Result<String> {
    if depth > 32 || nodes.len() > *remaining {
        return Err(unsupported(
            "Rich text exceeds saved-view projection limits.",
        ));
    }
    *remaining -= nodes.len();
    for node in nodes {
        let text = match node["type"].as_str() {
            Some("text") => node["text"].as_str(),
            Some("channel") if labels => node["channel_id"]
                .as_str()
                .and_then(|id| context.channel_names.get(id).map(String::as_str)),
            Some("user") if labels => node["user_id"]
                .as_str()
                .and_then(|id| context.user_display_names.get(id).map(String::as_str)),
            _ => None,
        };
        if let Some(text) = text.filter(|s| !s.is_empty()) {
            return Ok(text.into());
        }
        if let Some(children) = node["elements"].as_array() {
            let text = first_text(children, context, labels, depth + 1, remaining)?;
            if !text.is_empty() {
                return Ok(text);
            }
        }
    }
    Ok(String::new())
}
fn ignored(column: &Column, context: &ProjectionContext) -> bool {
    !context.todo_mode
        && matches!(
            column.kind.as_str(),
            "todo_completed" | "todo_assignee" | "todo_due_date"
        )
}
pub(super) fn validate_filter(
    document: &Document,
    filter: &Filter,
    context: &ProjectionContext,
) -> Result<()> {
    let c = model::column(document, &filter.key, filter.column_id.as_ref())?;
    if ignored(c, context) {
        return Ok(());
    }
    if dates::kind(&c.kind) {
        return dates::validate(filter, context);
    }
    let op = filter.operator.as_str();
    let valid = match c.kind.as_str() {
        "checkbox" | "completed" | "todo_completed" => matches!(op, "is_empty" | "is_not_empty"),
        "text" | "email" | "phone" => matches!(
            op,
            "contains" | "does_not_contain" | "is_empty" | "is_not_empty"
        ),
        "number" | "currency" => matches!(
            op,
            "equal"
                | "not_equal"
                | "greater_than"
                | "less_than"
                | "greater_than_or_equal"
                | "less_than_or_equal"
                | "is_empty"
                | "is_not_empty"
        ),
        "rating" => matches!(
            op,
            "contains" | "does_not_contain" | "is_empty" | "is_not_empty"
        ),
        "select" => matches!(
            op,
            "equal" | "not_equal" | "contains" | "does_not_contain" | "is_empty" | "is_not_empty"
        ),
        "user" | "assignee" | "todo_assignee" | "channel" => matches!(
            op,
            "equal"
                | "not_equal"
                | "contains"
                | "does_not_contain"
                | "is_empty"
                | "is_not_empty"
                | "is_me"
                | "is_not_me"
        ),
        _ => false,
    };
    if !valid {
        return Err(unsupported(format!(
            "Filter {} on {} is not supported yet.",
            op, c.kind
        )));
    }
    if matches!(c.kind.as_str(), "number" | "currency")
        && !matches!(op, "is_empty" | "is_not_empty")
        && let Some(value) = filter.values.first()
    {
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        if !text.trim().is_empty() && !text.parse::<f64>().is_ok_and(|n| n.is_finite()) {
            return Err(unsupported("Numeric filter value is malformed."));
        }
    }
    if c.kind == "select" {
        validate_select(c)?;
    }
    if filter
        .values
        .iter()
        .any(|v| !v.is_string() && !v.is_number())
    {
        return Err(unsupported(
            "Filter values are not supported scalar values.",
        ));
    }
    if (matches!(op, "is_me" | "is_not_me") || filter.values.iter().any(|v| v == "viewer"))
        && context.viewer_id.as_deref().is_none_or(str::is_empty)
    {
        return Err(unsupported(
            "This saved filter requires the authenticated viewer.",
        ));
    }
    Ok(())
}
pub(super) fn filter(
    document: &Document,
    record: &Record,
    filter: &Filter,
    context: &ProjectionContext,
) -> Result<bool> {
    let c = model::column(document, &filter.key, filter.column_id.as_ref())?;
    if ignored(c, context) {
        return Ok(true);
    }
    if dates::kind(&c.kind) {
        return dates::filter(c, record, filter, context);
    }
    let op = filter.operator.as_str();
    let value = atom(
        c,
        record,
        context,
        matches!(op, "is_empty" | "is_not_empty"),
    )?;
    if op == "is_empty" {
        return Ok(value.empty());
    }
    if op == "is_not_empty" {
        return Ok(!value.empty());
    }
    if filter.values.is_empty()
        && !matches!(op, "is_me" | "is_not_me")
        && !matches!(c.kind.as_str(), "text" | "email" | "phone")
    {
        return Ok(true);
    }
    let values = filter
        .values
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .collect::<Vec<_>>();
    Ok(match c.kind.as_str() {
        "text" | "email" | "phone" => {
            let found = value
                .text()
                .to_lowercase()
                .contains(&values.first().cloned().unwrap_or_default().to_lowercase());
            if op == "does_not_contain" {
                !found
            } else {
                found
            }
        }
        "number" | "currency" => {
            let target = values.first().map(String::as_str).unwrap_or("").trim();
            let target = if target.is_empty() {
                0.0
            } else {
                target
                    .parse::<f64>()
                    .map_err(|_| unsupported("Numeric filter value is malformed."))?
            };
            match value {
                Atom::Number(n) => match op {
                    "equal" => n == target,
                    "not_equal" => n != target,
                    "greater_than" => n > target,
                    "less_than" => n < target,
                    "greater_than_or_equal" => n >= target,
                    "less_than_or_equal" => n <= target,
                    _ => false,
                },
                _ => false,
            }
        }
        "rating" => {
            let found = values.contains(&value.text());
            if op == "does_not_contain" {
                !found
            } else {
                found
            }
        }
        _ => {
            let mut actual = value
                .text()
                .split('|')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let multi = if c.kind == "select" {
                c.is_multi_select()
            } else {
                c.raw.pointer("/options/format").and_then(Value::as_str) == Some("multi_entity")
            };
            if matches!(op, "is_me" | "is_not_me") {
                // User IDs are opaque identities: U1 must never match U10.
                let found = actual
                    .iter()
                    .any(|id| Some(id.as_str()) == context.viewer_id.as_deref());
                if op == "is_not_me" { !found } else { found }
            } else {
                if !multi {
                    actual.truncate(1)
                }
                let found = values.iter().any(|v| {
                    actual.contains(&if v == "viewer" {
                        context.viewer_id.clone().unwrap_or_default()
                    } else {
                        v.clone()
                    })
                });
                if matches!(op, "not_equal" | "does_not_contain") {
                    !found
                } else {
                    found
                }
            }
        }
    })
}
#[derive(Clone, Debug)]
enum Key {
    Missing,
    Number(f64),
    Text(String),
}
fn compare(a: &Key, b: &Key, ascending: bool) -> Ordering {
    match (a, b) {
        (Key::Missing, Key::Missing) => Ordering::Equal,
        (Key::Missing, _) => Ordering::Greater,
        (_, Key::Missing) => Ordering::Less,
        _ => {
            let ord = match (a, b) {
                (Key::Number(a), Key::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
                (Key::Text(a), Key::Text(b)) => a.cmp(b),
                _ => Ordering::Equal,
            };
            if ascending { ord } else { ord.reverse() }
        }
    }
}
pub(super) fn choices(c: &Column) -> Result<Vec<(String, String)>> {
    validate_select(c)?;
    let input = match c.raw.pointer("/options/choices") {
        None => &[][..],
        Some(Value::Array(a)) => a.as_slice(),
        _ => return Err(unsupported("Select choices are malformed.")),
    };
    let mut seen = BTreeSet::new();
    input
        .iter()
        .map(|v| {
            let id = v["value"]
                .as_str()
                .filter(|v| !v.is_empty() && !v.contains('|'))
                .ok_or_else(|| unsupported("Select choice identity is malformed."))?;
            if !seen.insert(id) {
                return Err(unsupported("Select choice identities are ambiguous."));
            }
            let label = v["label"]
                .as_str()
                .ok_or_else(|| unsupported("Select choice label is malformed."))?;
            Ok((id.into(), label.into()))
        })
        .collect()
}
fn validate_select(c: &Column) -> Result<()> {
    if c.raw
        .pointer("/options/format")
        .is_some_and(|v| !matches!(v.as_str(), Some("single_select" | "multi_select")))
    {
        return Err(unsupported("Select format is not supported."));
    }
    Ok(())
}
pub(super) fn validate_sort(
    document: &Document,
    sort: &Sort,
    context: &ProjectionContext,
) -> Result<()> {
    let c = model::column(document, &sort.key, sort.column_id.as_ref())?;
    if ignored(c, context) {
        return Ok(());
    }
    if dates::kind(&c.kind) {
        return Ok(());
    }
    if !matches!(
        c.kind.as_str(),
        "text"
            | "number"
            | "currency"
            | "rating"
            | "select"
            | "checkbox"
            | "completed"
            | "todo_completed"
            | "user"
            | "todo_assignee"
            | "channel"
            | "email"
            | "phone"
    ) {
        return Err(unsupported(format!(
            "Sorting {} fields is not supported yet.",
            c.kind
        )));
    }
    if c.kind == "select" {
        choices(c)?;
    }
    Ok(())
}
pub(super) fn sort(
    document: &Document,
    rows: &mut Vec<&Record>,
    sorts: &[Sort],
    context: &ProjectionContext,
) -> Result<()> {
    let plans = sorts
        .iter()
        .map(|sort| {
            let column = model::column(document, &sort.key, sort.column_id.as_ref())?;
            let ranks = if column.kind == "select" {
                choices(column)?
                    .into_iter()
                    .enumerate()
                    .map(|(index, (id, _))| (id, index))
                    .collect::<BTreeMap<_, _>>()
            } else {
                BTreeMap::new()
            };
            Ok((column, ranks))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut decorated = Vec::with_capacity(rows.len());
    for row in rows.iter() {
        let mut keys = Vec::with_capacity(sorts.len());
        for (sort, (c, ranks)) in sorts.iter().zip(&plans) {
            if ignored(c, context) {
                keys.push(Key::Missing);
                continue;
            }
            if dates::kind(&c.kind) {
                keys.push(Key::Number(dates::sort_key(
                    c,
                    row,
                    context,
                    sort.ascending,
                )?));
                continue;
            }
            let value = atom(c, row, context, true)?;
            let key = match c.kind.as_str() {
                "select" => value
                    .text()
                    .split('|')
                    .filter_map(|id| ranks.get(id))
                    .min()
                    .copied()
                    .map(|n| Key::Number(n as f64))
                    .unwrap_or(Key::Missing),
                "number" | "currency" | "rating" => match value {
                    Atom::Number(n) => Key::Number(n),
                    _ => Key::Missing,
                },
                "checkbox" | "completed" | "todo_completed" => {
                    Key::Number(if matches!(value, Atom::Boolean(true)) {
                        1.0
                    } else {
                        0.0
                    })
                }
                "user" | "todo_assignee" => Key::Text(
                    context
                        .user_real_names
                        .get(&value.text())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_lowercase())
                        .unwrap_or_else(|| {
                            if sort.ascending {
                                "zzzzzzzz".into()
                            } else {
                                String::new()
                            }
                        }),
                ),
                "channel" => {
                    let text = value.text();
                    Key::Text(if text.is_empty() {
                        if sort.ascending {
                            "zzzzzzzz".into()
                        } else {
                            String::new()
                        }
                    } else {
                        context
                            .channel_names
                            .get(text.split('|').next().unwrap_or(""))
                            .map(|s| s.to_lowercase())
                            .unwrap_or_default()
                    })
                }
                "text" => {
                    let text = value.text();
                    Key::Text(if matches!(value, Atom::Empty) {
                        if sort.ascending {
                            "zzzzzzzz".into()
                        } else {
                            String::new()
                        }
                    } else {
                        text.to_lowercase()
                    })
                }
                _ => Key::Text(value.text()),
            };
            keys.push(key);
        }
        decorated.push((*row, keys));
    }
    decorated.sort_by(|(_, a), (_, b)| {
        a.iter()
            .zip(b)
            .zip(sorts)
            .map(|((a, b), sort)| compare(a, b, sort.ascending))
            .find(|o| !o.is_eq())
            .unwrap_or(Ordering::Equal)
    });
    *rows = decorated.into_iter().map(|(row, _)| row).collect();
    Ok(())
}
pub(super) fn validate_completion_policy(context: &ProjectionContext) -> Result<()> {
    if context.todo_mode && context.apply_todo_completion_rules.is_none() {
        return Err(unsupported(
            "This task view requires a verified completion-visibility policy.",
        ));
    }
    Ok(())
}
pub(super) fn completion(
    document: &Document,
    row: &Record,
    view: &SavedView,
    context: &ProjectionContext,
) -> Result<bool> {
    validate_completion_policy(context)?;
    if !context.todo_mode || context.apply_todo_completion_rules == Some(false) {
        return Ok(true);
    }
    let Some(c) = document.columns.iter().find(|c| c.kind == "todo_completed") else {
        return Ok(true);
    };
    let done = matches!(atom(c, row, context, false)?, Atom::Boolean(true));
    let show = match context.effective_show_completed {
        Some(value) => value,
        None => match view.raw.get("show_completed_items") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => return Err(unsupported("Completed-item visibility is malformed.")),
        },
    };
    Ok((show || !done) && (view.default_view_key.as_deref() != Some("completed_items") || done))
}
