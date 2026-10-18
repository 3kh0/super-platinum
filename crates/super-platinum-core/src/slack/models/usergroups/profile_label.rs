//! E1L -> 0dhJ -> XXup initial user-group label, from an observed source row.
use super::UserGroup;
use serde_json::Value;

impl UserGroup {
    /// Missing raw evidence is distinct from an observed empty handle. The
    /// latter inserts no text into Slack's Delta and therefore has length zero.
    /// This English client uses the source's English unknown-group message.
    pub fn profile_label(&self) -> Option<String> {
        let raw = self.label_source.as_ref()?;
        if raw.get("id")?.as_str()? != self.id {
            return None;
        }
        let handle = observed_handle(raw).or_else(|| unknown(raw).then_some(""))?;
        if handle != self.handle {
            return None;
        }
        if unknown(raw) {
            return Some("@unknown-group".into());
        }
        if handle.is_empty() {
            Some(String::new())
        } else {
            Some(format!("@{}", handle.strip_prefix('@').unwrap_or(handle)))
        }
    }
}

pub(super) fn observed_handle(raw: &Value) -> Option<&str> {
    match raw.get("handle") {
        None => Some(""),
        Some(Value::String(s)) => Some(s),
        Some(value) if !truthy(value) => Some(""),
        _ => None,
    }
}

pub(super) fn unknown(raw: &Value) -> bool {
    ["isUnknown", "isNonExistent"]
        .iter()
        .any(|key| raw.get(*key).is_some_and(truthy))
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn observed_handles_preserve_source_prefix_whitespace_unicode_and_empty() {
        for (handle, expected) in [
            ("crew", "@crew"),
            ("@crew", "@crew"),
            ("@@crew", "@@crew"),
            ("@", "@"),
            ("", ""),
            ("  Cafe\u{301}  ", "@  Cafe\u{301}  "),
        ] {
            let group = UserGroup::from_observed(
                &json!({"id":"S1","handle":handle,"date_delete":123,"is_visible":false}),
            )
            .unwrap();
            assert_eq!(group.profile_label().as_deref(), Some(expected));
        }
        let group = UserGroup::from_observed(&json!({"id":"S1"})).unwrap();
        assert_eq!(group.profile_label().as_deref(), Some(""));
        for handle in [Value::Null, json!(false), json!(0)] {
            let group = UserGroup::from_observed(&json!({"id":"S1","handle":handle})).unwrap();
            assert_eq!(group.profile_label().as_deref(), Some(""));
        }
        for handle in [json!(true), json!(1), json!([]), json!({})] {
            assert!(UserGroup::from_observed(&json!({"id":"S1","handle":handle})).is_err());
            let group =
                UserGroup::from_observed(&json!({"id":"S1","handle":handle,"isUnknown":true}))
                    .unwrap();
            assert_eq!(group.profile_label().as_deref(), Some("@unknown-group"));
        }
    }

    #[test]
    fn actual_unknown_flags_override_handle_using_source_truthiness() {
        for key in ["isUnknown", "isNonExistent"] {
            for flag in [json!(true), json!("false"), json!([]), json!({}), json!(1)] {
                let group =
                    UserGroup::from_observed(&json!({"id":"S1","handle":"crew",(key):flag}))
                        .unwrap();
                assert_eq!(group.profile_label().as_deref(), Some("@unknown-group"));
            }
            for flag in [Value::Null, json!(false), json!(""), json!(0)] {
                let group =
                    UserGroup::from_observed(&json!({"id":"S1","handle":"crew",(key):flag}))
                        .unwrap();
                assert_eq!(group.profile_label().as_deref(), Some("@crew"));
            }
        }
    }

    #[test]
    fn legacy_placeholder_and_native_drift_do_not_invent_label_authority() {
        let mut group: UserGroup =
            serde_json::from_value(json!({"id":"S1","handle":"crew"})).unwrap();
        assert!(group.profile_label().is_none());
        group = UserGroup::from_observed(&json!({"id":"S1","handle":"crew"})).unwrap();
        group.handle = "changed".into();
        assert!(group.profile_label().is_none());
        group.handle = "crew".into();
        group.id = "S2".into();
        assert!(group.profile_label().is_none());
    }

    #[test]
    fn directory_response_and_cached_roundtrip_keep_actual_label_source() {
        let raw = json!({"results":[{"id":"S1","handle":"@@crew","isUnknown":true,"opaque":1}]});
        let page: super::super::UserGroupsPage = serde_json::from_value(raw.clone()).unwrap();
        let group = &page.results[0];
        assert_eq!(group.label_source, Some(raw["results"][0].clone()));
        let restored: UserGroup =
            serde_json::from_value(serde_json::to_value(group).unwrap()).unwrap();
        assert_eq!(restored.label_source, group.label_source);
        assert_eq!(restored.profile_label(), group.profile_label());
    }
}
