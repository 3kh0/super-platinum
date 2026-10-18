//! +E6Z.AO uses truthy case-insensitive per-view overrides, then truthy base
//! position stringification. Both absent is null (cell-only fallback), not a
//! fresh midpoint. Ommm is shared with saved-view reorder without normalization.
use super::{ListError, Record};

fn invalid() -> ListError {
    ListError("This Board record position cannot be moved safely.")
}

pub(super) fn key(record: &Record, view: &str) -> Result<Option<String>, ListError> {
    let positions = record.raw.get("view_positions").filter(|v| !v.is_null());
    let mut selected = None;
    if let Some(positions) = positions {
        let positions = positions.as_object().ok_or_else(invalid)?;
        for (id, value) in positions {
            if id.eq_ignore_ascii_case(view) {
                if selected.is_some() {
                    return Err(invalid());
                }
                selected = Some(value);
            }
        }
    }
    let selected = selected.filter(|value| match value {
        serde_json::Value::Null => false,
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Number(n) => n.as_f64() != Some(0.0),
        serde_json::Value::Bool(b) => *b,
        _ => true,
    });
    let value = if let Some(value) = selected {
        // AO passes a view override unchanged; numeric truthy overrides must not
        // be stringified as though they were base positions.
        value.as_str().ok_or_else(invalid)?.to_owned()
    } else {
        match record.raw.get("position") {
            None | Some(serde_json::Value::Null) => return Ok(None),
            Some(serde_json::Value::String(s)) if s.is_empty() => return Ok(None),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) if n.as_f64() == Some(0.0) => return Ok(None),
            Some(serde_json::Value::Number(n)) => {
                let n = n.as_f64().ok_or_else(invalid)?;
                if !n.is_finite() || n.fract() != 0.0 || !(0.0..10_000_000_000.0).contains(&n) {
                    return Err(invalid());
                }
                (n as u64).to_string()
            }
            _ => return Err(invalid()),
        }
    };
    if !super::super::view_reorder::key::valid(&value) {
        return Err(invalid());
    }
    Ok(Some(value))
}

pub(super) fn between(
    left: Option<&Record>,
    right: Option<&Record>,
    view: &str,
) -> Result<Option<String>, ListError> {
    let left = left.map(|r| key(r, view)).transpose()?.flatten();
    let right = right.map(|r| key(r, view)).transpose()?.flatten();
    if left.is_none() && right.is_none() {
        return Ok(None);
    }
    super::super::view_reorder::key::between(left.as_deref(), right.as_deref()).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn record(position: serde_json::Value, overrides: serde_json::Value) -> Record {
        Record {
            id: "Rec1".into(),
            fields: Default::default(),
            raw: json!({"id":"Rec1","position":position,"view_positions":overrides,"fields":[]}),
        }
    }
    #[test]
    fn same_source_fractional_neighbors_and_null_are_distinct() {
        let a = record(json!("0000000001"), json!({}));
        let b = record(json!("0000000002"), json!({}));
        assert_eq!(
            between(Some(&a), Some(&b), "V1").unwrap().as_deref(),
            Some("0000000001.V")
        );
        assert_eq!(between(None, None, "V1").unwrap(), None);
        let missing = record(json!(0), json!({}));
        assert_eq!(between(Some(&missing), None, "V1").unwrap(), None);
    }
    #[test]
    fn truthy_override_is_case_insensitive_and_not_coerced() {
        let a = record(
            json!("0000000001"),
            json!({"v1":"0000000001.V","OTHER":{"retained":true}}),
        );
        assert_eq!(key(&a, "V1").unwrap().as_deref(), Some("0000000001.V"));
        for missing in [json!(0), json!(false), json!(null), json!("")] {
            let a = record(json!("0000000001"), json!({"v1":missing}));
            assert_eq!(key(&a, "V1").unwrap().as_deref(), Some("0000000001"));
        }
        assert!(
            key(
                &record(json!("0000000001"), json!({"v1":5000000000_u64})),
                "V1"
            )
            .is_err()
        );
        assert!(
            key(
                &record(
                    json!("0000000001"),
                    json!({"V1":"0000000002","v1":"0000000003"})
                ),
                "V1"
            )
            .is_err()
        );
    }
    #[test]
    fn source_base_numbers_stringify_without_padding_or_legacy_normalization() {
        assert_eq!(
            key(&record(json!(5000000000_u64), json!({})), "V1")
                .unwrap()
                .as_deref(),
            Some("5000000000")
        );
        for malformed in [
            json!(1),
            json!(1.5),
            json!(-1),
            json!("0000000001.0"),
            json!("legacy"),
        ] {
            assert!(key(&record(malformed, json!({})), "V1").is_err());
        }
    }
}
