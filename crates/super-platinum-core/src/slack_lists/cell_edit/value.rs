use super::*;
use std::collections::HashSet;

mod date;
mod rich;

pub(super) fn identity(column: &Column, record: &Record) -> Result<(), ListError> {
    if ["archived", "is_deleted"].iter().any(|key| {
        record
            .raw
            .get(key)
            .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
    }) {
        return Err(ListError("This List item is archived or unavailable."));
    }
    if let Some(field) = record.fields.get(&column.key) {
        if !field.is_object()
            || field
                .get("column_id")
                .is_some_and(|id| id.as_str() != Some(&column.id))
            || field
                .get("row_id")
                .is_some_and(|id| id.as_str() != Some(&record.id))
        {
            return Err(ListError("This List field has an ambiguous identity."));
        }
    }
    Ok(())
}

pub(super) fn eligible(
    column: &Column,
    record: &Record,
    list_id: &str,
    personal: Option<&PersonalTaskLists>,
) -> Result<(), ListError> {
    identity(column, record)?;
    if matches!(
        column.kind.as_str(),
        "checkbox" | "completed" | "todo_completed"
    ) {
        return if column.editable_checkbox_with_policy(record, list_id, personal) {
            Ok(())
        } else {
            Err(ListError("This checkbox is not editable."))
        };
    }
    // Personal To-dos have specialized handlers. This slice only implements the
    // ordinary List update path, even for otherwise familiar schema types.
    if personal.is_none_or(|policy| policy.contains(list_id)) {
        return Err(ListError(
            "Ordinary List editing preferences are not verified.",
        ));
    }
    validate_read(column, record)
}

fn member(column: &Column) -> Result<&'static str, ListError> {
    match column.kind.as_str() {
        "text" | "rich_text" => Ok("rich_text"),
        "number" | "currency" => Ok("number"),
        "select" | "multi_select" => Ok("select"),
        "date" | "due_date" | "todo_due_date" => Ok("date"),
        "checkbox" | "completed" | "todo_completed" => Ok("checkbox"),
        _ => Err(ListError("Editing this field type is not supported yet.")),
    }
}
fn raw<'a>(column: &Column, record: &'a Record) -> Result<Option<&'a Value>, ListError> {
    let member = member(column)?;
    let Some(field) = record.fields.get(&column.key) else {
        return Ok(None);
    };
    if member == "rich_text"
        && field.get("rich_text").is_some()
        && field.get("richText").is_some()
        && field.get("rich_text") != field.get("richText")
    {
        return Err(ListError(
            "This rich-text field has conflicting representations.",
        ));
    }
    let result = field.get(member).or_else(|| {
        if member == "rich_text" {
            field.get("richText")
        } else {
            None
        }
    });
    if result.is_none_or(Value::is_null)
        && field.as_object().is_some_and(|object| {
            object.iter().any(|(key, value)| {
                !matches!(
                    key.as_str(),
                    "key" | "row_id" | "column_id" | "text" | "rich_text" | "richText"
                ) && !value.is_null()
            })
        })
    {
        return Err(ListError(
            "This List field has an unsupported stored value.",
        ));
    }
    if member == "rich_text"
        && result.is_none_or(|value| value.is_null() || value.as_array().is_some_and(Vec::is_empty))
        && field
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
    {
        return Err(ListError(
            "The original rich text is unavailable; editing would lose content.",
        ));
    }
    Ok(result.filter(|value| !value.is_null()))
}
fn validate_read(column: &Column, record: &Record) -> Result<(), ListError> {
    if member(column)? == "date" {
        return date::read(record.fields.get(&column.key)).map(|_| ());
    }
    let value = raw(column, record)?;
    match member(column)? {
        "rich_text" => match value {
            None => Ok(()),
            Some(Value::Array(blocks)) => rich::validate(blocks, blocks),
            _ => Err(ListError("This rich-text field is malformed.")),
        },
        "number" => match value {
            None => Ok(()),
            Some(Value::Array(values)) if values.is_empty() => Ok(()),
            Some(Value::Array(values)) if values.len() == 1 => values[0]
                .as_number()
                .filter(|n| valid_number(n))
                .map(|_| ())
                .ok_or(ListError(
                    "This numeric field is malformed or outside the supported range.",
                )),
            _ => Err(ListError("This numeric field is malformed.")),
        },
        "select" => {
            let options = choices(column)?;
            match value {
                None => Ok(()),
                Some(Value::Array(values)) => {
                    let mut seen = HashSet::new();
                    if (!column.is_multi_select() && values.len() > 1)
                        || values.iter().any(|v| {
                            v.as_str()
                                .is_none_or(|id| !options.contains(id) || !seen.insert(id))
                        })
                    {
                        Err(ListError(
                            "This selection contains unavailable or ambiguous options.",
                        ))
                    } else {
                        Ok(())
                    }
                }
                _ => Err(ListError("This selection is malformed.")),
            }
        }
        "checkbox" => value
            .and_then(Value::as_bool)
            .map(|_| ())
            .ok_or(ListError("This checkbox is missing or malformed.")),
        _ => unreachable!(),
    }
}
pub(super) fn read(column: &Column, record: &Record) -> Result<CellEditValue, ListError> {
    if member(column)? == "date" {
        return date::read(record.fields.get(&column.key))
            .map(|value| CellEditValue::Date(value.map(str::to_owned)));
    }
    validate_read(column, record)?;
    let value = raw(column, record)?;
    Ok(match member(column)? {
        "rich_text" => {
            CellEditValue::RichText(value.and_then(Value::as_array).cloned().unwrap_or_default())
        }
        "number" => CellEditValue::Number(
            value
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_number)
                .cloned(),
        ),
        "select" => CellEditValue::Select(
            value
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
        ),
        "checkbox" => CellEditValue::Boolean(value.and_then(Value::as_bool).unwrap()),
        _ => unreachable!(),
    })
}
fn choices(column: &Column) -> Result<HashSet<&str>, ListError> {
    if column.raw.pointer("/options/format").is_some_and(|value| {
        !value.is_null() && !matches!(value.as_str(), Some("single_select" | "multi_select"))
    }) {
        return Err(ListError("This column's selection format is unsupported."));
    }
    if column
        .raw
        .get("options")
        .is_some_and(|value| !value.is_object() && !value.is_null())
    {
        return Err(ListError("This column's options are malformed."));
    }
    let values = match column.raw.pointer("/options/choices") {
        None => return Ok(HashSet::new()),
        Some(Value::Array(values)) => values,
        _ => return Err(ListError("This column's options are malformed.")),
    };
    let mut result = HashSet::new();
    for option in values {
        let id = option
            .get("value")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && !id.contains('|'))
            .ok_or(ListError("This column's option identity is malformed."))?;
        if option.get("label").and_then(Value::as_str).is_none() || !result.insert(id) {
            return Err(ListError(
                "This column's options are ambiguous or malformed.",
            ));
        }
    }
    Ok(result)
}
fn valid_number(number: &Number) -> bool {
    // The captured client edits JavaScript numbers, not arbitrary-precision integers.
    if let Some(value) = number.as_u64() {
        return value <= 9_007_199_254_740_991;
    }
    if let Some(value) = number.as_i64() {
        return value >= -9_007_199_254_740_991;
    }
    number.as_f64().is_some_and(f64::is_finite)
}
pub(super) fn validate(
    column: &Column,
    desired: &CellEditValue,
    initial: &CellEditValue,
) -> Result<(), ListError> {
    match (column.kind.as_str(), desired, initial) {
        (
            "text" | "rich_text",
            CellEditValue::RichText(blocks),
            CellEditValue::RichText(original),
        ) => rich::validate(blocks, original),
        ("number" | "currency", CellEditValue::Number(value), CellEditValue::Number(_)) => {
            if value.as_ref().is_none_or(valid_number) {
                Ok(())
            } else {
                Err(ListError("Enter a finite number in the supported range."))
            }
        }
        (
            "date" | "due_date" | "todo_due_date",
            CellEditValue::Date(value),
            CellEditValue::Date(_),
        ) => {
            if value.as_deref().is_none_or(date::valid) {
                Ok(())
            } else {
                Err(ListError(
                    "Enter a real calendar date in YYYY-MM-DD format.",
                ))
            }
        }
        ("select" | "multi_select", CellEditValue::Select(values), CellEditValue::Select(_)) => {
            let options = choices(column)?;
            let mut seen = HashSet::new();
            if (!column.is_multi_select() && values.len() > 1)
                || values
                    .iter()
                    .any(|id| !options.contains(id.as_str()) || !seen.insert(id))
            {
                Err(ListError("Choose current options from this column."))
            } else {
                Ok(())
            }
        }
        _ => Err(ListError("The edit does not match this field's type.")),
    }
}
pub(super) fn wire(value: &CellEditValue) -> (&'static str, Value) {
    match value {
        CellEditValue::Boolean(value) => ("checkbox", json!(value)),
        CellEditValue::RichText(value) => ("rich_text", Value::Array(value.clone())),
        CellEditValue::Number(value) => (
            "number",
            Value::Array(value.iter().cloned().map(Value::Number).collect()),
        ),
        CellEditValue::Select(value) => ("select", json!(value)),
        CellEditValue::Date(value) => (
            "date",
            Value::Array(value.iter().cloned().map(Value::String).collect()),
        ),
    }
}

// A matching date member cannot acknowledge an incompatible timestamp sibling.
// Other typed values retain their established receipt policy.
pub(super) fn receipt_valid(desired: &CellEditValue, cell: &Value) -> bool {
    match desired {
        CellEditValue::Date(desired) => {
            date::read(Some(cell)).is_ok_and(|actual| actual == desired.as_deref())
        }
        _ => true,
    }
}
