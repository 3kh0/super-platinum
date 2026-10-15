//! Only source-proven neutral metadata may be omitted from a supported snapshot.
use serde_json::Value;

/// Validates two optional fields without rewriting raw evidence or inventing
/// missing values. This is not a complete draft/identity/lifecycle validator.
///
/// Retained B/S hashes: docs/slack-external-select-protocol.md. S2371349 resets
/// date_stashed to numeric 0 when attaching; B8468799 tests its truthiness, so
/// string "0" is not equivalent. B6893099 builds a permission array; B6893577
/// omits the empty array on send; B6801646/6804626 default read permissions to [].
/// Objects, nonempty grants and mpdm_name are not neutralized. S3728158 preserves
/// the distinction between absent and empty mpdm_name in draft equality.
///
/// The new admission is deliberately file-free. Existing payloads with neither
/// field retain their previous file handling; each caller still checks unknown
/// fields, the exact destination, content, authentication and lifecycle.
pub fn supports_neutral_metadata(raw: &Value) -> bool {
    let Some(object) = raw.as_object() else {
        return false;
    };
    let stash = object.get("date_stashed");
    let permissions = object.get("file_permissions");
    stash.is_none_or(|value| value.as_u64() == Some(0))
        && permissions.is_none_or(|value| matches!(value, Value::Array(items) if items.is_empty()))
        && ((stash.is_none() && permissions.is_none())
            || object
                .get("file_ids")
                .is_some_and(|value| matches!(value, Value::Array(items) if items.is_empty())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_explicit_neutral_types_are_supported_without_defaulting() {
        for fields in [
            json!({}),
            json!({"date_stashed":0}),
            json!({"file_permissions":[]}),
            json!({"date_stashed":0,"file_permissions":[]}),
        ] {
            let mut raw = fields;
            raw["file_ids"] = json!([]);
            assert!(supports_neutral_metadata(&raw));
        }
        for value in [
            json!("0"),
            json!(0.0),
            json!(1),
            json!(-1),
            Value::Null,
            json!(false),
            json!(true),
            json!([]),
            json!({}),
        ] {
            assert!(!supports_neutral_metadata(
                &json!({"file_ids":[],"date_stashed":value})
            ));
        }
        for value in [
            json!({}),
            json!([{"file_id":"F1","grant":false}]),
            json!([null]),
            json!("[]"),
            Value::Null,
            json!(false),
            json!(0),
        ] {
            assert!(!supports_neutral_metadata(
                &json!({"file_ids":[],"file_permissions":value})
            ));
        }
        assert!(!supports_neutral_metadata(&json!({"date_stashed":0})));
        assert!(!supports_neutral_metadata(
            &json!({"file_ids":["F1"],"file_permissions":[]})
        ));
        // No change to existing file-bearing payloads when both fields absent.
        assert!(supports_neutral_metadata(&json!({"file_ids":["F1"]})));
        assert!(!supports_neutral_metadata(&Value::Null));
    }
}
