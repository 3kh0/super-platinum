//! B uDYD7312962 writes user/channel ordered ID arrays; [] clears.
//! B convertEntity17991030 defaults to single_entity. Canonical IDs follow
//! S kflk4619765 (U/W, without paste aliases) and kdyP4619174 (C/G/D/Z).
use super::*;

fn schema(column: &Column) -> Result<(), ListError> {
    if !matches!(column.kind.as_str(), "user" | "channel")
        || column
            .raw
            .get("options")
            .is_some_and(|v| !v.is_null() && !v.is_object())
        || column.raw.pointer("/options/format").is_some_and(|v| {
            !v.is_null() && !matches!(v.as_str(), Some("single_entity" | "multi_entity"))
        })
    {
        return Err(ListError(
            "This column's entity selection format is unsupported.",
        ));
    }
    Ok(())
}

pub(super) fn validate<'a>(
    column: &Column,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), ListError> {
    schema(column)?;
    let mut seen = HashSet::new();
    let multi = column.is_multi_user() || column.is_multi_channel();
    let prefixes: &[u8] = match column.kind.as_str() {
        "user" => b"UW",
        "channel" => b"CGDZ",
        _ => unreachable!("validated entity schema"),
    };
    for id in values {
        let bytes = id.as_bytes();
        if bytes.len() < 9
            || !prefixes.contains(&bytes[0])
            || !bytes[1..]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            || !seen.insert(id)
            || (!multi && seen.len() > 1)
        {
            return Err(ListError(
                "Choose distinct valid references for this column.",
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_stored<'a>(
    column: &Column,
    value: Option<&'a Value>,
) -> Result<&'a [Value], ListError> {
    let values = match value {
        None => &[][..],
        Some(Value::Array(values)) => values,
        _ => return Err(ListError("This reference field is malformed.")),
    };
    // Validate before allocating a retained editor seed. No workspace cache
    // membership requirement: an existing unknown reference must remain removable.
    validate(
        column,
        values.iter().map(|v| v.as_str().unwrap_or_default()),
    )?;
    Ok(values)
}

pub(super) fn read(column: &Column, value: Option<&Value>) -> Result<Vec<String>, ListError> {
    Ok(validate_stored(column, value)?
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect())
}
