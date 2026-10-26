//! The selected JSON value in aBXg's template interpolation. No tone range gate.
use super::{Unavailable, Value};

pub(super) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

pub(super) fn string(value: &Value) -> Result<String, Unavailable> {
    // Same work envelope as the Profile rich-content boundary. This prevents a
    // directly supplied nested tone array from creating an unbounded traversal.
    let mut output = String::new();
    append(value, &mut output, 0, &mut 10_000)?;
    Ok(output)
}

fn append(
    value: &Value,
    output: &mut String,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), Unavailable> {
    if depth > 32 || *nodes == 0 {
        return Err(Unavailable::UnsupportedTitle);
    }
    *nodes -= 1;
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(&number(
            value.as_f64().ok_or(Unavailable::UnsupportedTitle)?,
        )?),
        Value::String(value) => output.push_str(value),
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if *nodes == 0 {
                    return Err(Unavailable::UnsupportedTitle);
                }
                if index != 0 {
                    output.push(',');
                }
                // Array.prototype.join renders nullish entries as empty text.
                if value.is_null() {
                    *nodes -= 1;
                } else {
                    append(value, output, depth + 1, nodes)?;
                }
            }
        }
        Value::Object(fields) => {
            // A JSON own toString cannot be callable. It shadows the inherited
            // method; valueOf cannot produce a primitive, so JS throws here.
            if fields.contains_key("toString") {
                return Err(Unavailable::UnsupportedTitle);
            }
            output.push_str("[object Object]");
        }
    }
    if output.len() > 1_048_576 {
        return Err(Unavailable::UnsupportedTitle);
    }
    Ok(())
}

fn number(value: f64) -> Result<String, Unavailable> {
    if !value.is_finite() {
        return Err(Unavailable::UnsupportedTitle);
    }
    if value == 0.0 {
        return Ok("0".into());
    }
    // ECMAScript-compatible shortest formatting; the JSON number is first
    // represented as binary64, just as in the original formatter's runtime.
    Ok(ryu_js::Buffer::new().format(value).to_owned())
}
