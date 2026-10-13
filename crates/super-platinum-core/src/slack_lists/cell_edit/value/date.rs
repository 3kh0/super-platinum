//! Ordinary calendar dates only. Captured B uDYD7312962 emits date:[value]
//! or date:[]; hh3m18140064 uses iLfW's YYYY-MM-DD date picker format.
//! Timestamp To-dos/Later and date ranges have different semantics and are not
//! silently converted. A retained timestamp[-1] is the verified date sentinel.
use super::*;
use chrono::{Datelike, NaiveDate};

pub(super) fn valid(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        && NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .is_ok_and(|date| (1..=9999).contains(&date.year()))
}

pub(super) fn read(field: Option<&Value>) -> Result<Option<&str>, ListError> {
    let Some(field) = field else {
        return Ok(None);
    };
    let object = field
        .as_object()
        .ok_or(ListError("This date field is malformed."))?;
    let Some(raw) = object.get("date") else {
        // Only an absent/empty field is a new calendar value. In particular an
        // existing timestamp-only sentinel is not guessed to be a cleared date.
        if object.iter().any(|(key, value)| {
            !matches!(key.as_str(), "key" | "column_id" | "row_id")
                && !(value.is_null() || (key == "text" && value.as_str() == Some("")))
        }) {
            return Err(ListError(
                "This field does not contain an editable calendar date.",
            ));
        }
        return Ok(None);
    };
    let values = raw
        .as_array()
        .ok_or(ListError("This date field is malformed."))?;
    let date = match values.as_slice() {
        [] => None,
        [Value::String(date)] if valid(date) => Some(date.as_str()),
        _ => {
            return Err(ListError(
                "This date is malformed or contains an unsupported range.",
            ));
        }
    };
    if let Some(timestamp) = object.get("timestamp") {
        let sentinel = timestamp.as_array().is_some_and(|values| {
            values.is_empty() || (values.len() == 1 && values[0].as_f64() == Some(-1.0))
        });
        if !sentinel {
            return Err(ListError(
                "This date has a timestamp value that cannot be safely edited here.",
            ));
        }
    }
    Ok(date)
}
