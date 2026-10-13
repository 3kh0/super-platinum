//! Exact frozen action revision semantics, with borrowed comparisons for the
//! large block tree. Bot profiles retain their flattened Serde representation.
use super::Message;
use serde_json::{Value, json};

pub(super) fn capture(message: &Message) -> Value {
    json!({
        "blocks": message.blocks, "edited": message.edited,
        "bot_id": message.bot_id, "bot_profile": message.bot_profile,
        "thread_ts": message.thread_ts,
        "function_execution_id": message.extra.get("function_execution_id"),
        "is_ephemeral": message.extra.get("is_ephemeral"),
    })
}

pub(super) fn matches(message: &Message, snapshot: &Value) -> bool {
    let Some(snapshot) = snapshot.as_object().filter(|object| object.len() == 7) else {
        return false;
    };
    snapshot.get("blocks").and_then(Value::as_array) == Some(&message.blocks)
        && optional_value(snapshot.get("edited"), message.edited.as_ref())
        && optional_string(snapshot.get("bot_id"), message.bot_id.as_deref())
        && optional_string(snapshot.get("thread_ts"), message.thread_ts.as_deref())
        && optional_value(snapshot.get("function_execution_id"), message.extra.get("function_execution_id"))
        && optional_value(snapshot.get("is_ephemeral"), message.extra.get("is_ephemeral"))
        // Flattened extra fields may shadow known profile/icon fields. Comparing
        // Rust fields would differ from the capture's exact JSON representation.
        && snapshot.get("bot_profile") == Some(&json!(message.bot_profile))
}

fn optional_value(snapshot: Option<&Value>, current: Option<&Value>) -> bool {
    snapshot.is_some_and(|snapshot| snapshot == current.unwrap_or(&Value::Null))
}

fn optional_string(snapshot: Option<&Value>, current: Option<&str>) -> bool {
    snapshot.is_some_and(|snapshot| match current {
        Some(value) => snapshot.as_str() == Some(value),
        None => snapshot.is_null(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Message {
        serde_json::from_value(json!({
            "bot_id":"B1", "thread_ts":"1.000001", "edited":{"ts":"2.000001"},
            "is_ephemeral":false, "function_execution_id":"F1",
            "bot_profile":{"id":"B1","icons":{"image_36":"https://example.invalid/icon","unknown":{"nested":[1,null]}}},
            "blocks":[{"type":"section","block_id":"a","text":{"type":"mrkdwn","text":"A"}},
                {"type":"actions","block_id":"b","elements":[{"type":"button","action_id":"go","value":"one"}]}]
        })).unwrap()
    }

    #[test]
    fn borrowed_revision_equals_capture_oracle_across_all_tracked_fields() {
        let baseline = message();
        let mut variants = vec![baseline.clone(), Message::default()];
        for change in 0..12 {
            let mut message = baseline.clone();
            match change {
                0 => message.blocks[1]["elements"][0]["value"] = json!("two"),
                1 => message.blocks.swap(0, 1),
                2 => message.blocks.push(json!(null)),
                3 => message.edited = None,
                4 => message.bot_id = None,
                5 => message.thread_ts = Some("3.000001".into()),
                6 => {
                    message
                        .extra
                        .insert("function_execution_id".into(), json!(null));
                }
                7 => {
                    message.extra.insert("is_ephemeral".into(), json!(true));
                }
                8 => message.bot_profile = None,
                9 => {
                    message
                        .bot_profile
                        .as_mut()
                        .unwrap()
                        .extra
                        .insert("unknown".into(), json!({"nested":[2,1]}));
                }
                10 => {
                    message
                        .bot_profile
                        .as_mut()
                        .unwrap()
                        .icons
                        .as_mut()
                        .unwrap()
                        .extra
                        .insert("unknown".into(), json!({"nested":[1,2]}));
                }
                _ => {
                    message.text = Some("not revision content".into());
                    message.channel = Some("COTHER".into());
                }
            }
            variants.push(message);
        }
        for old in &variants {
            let snapshot = capture(old);
            for current in &variants {
                assert_eq!(matches(current, &snapshot), capture(current) == snapshot);
            }
        }
    }

    #[test]
    fn absent_and_explicit_null_keep_the_original_serialization_equivalence() {
        let absent = Message::default();
        let mut nulls = absent.clone();
        nulls.edited = Some(Value::Null);
        nulls
            .extra
            .insert("function_execution_id".into(), Value::Null);
        nulls.extra.insert("is_ephemeral".into(), Value::Null);
        assert_eq!(capture(&absent), capture(&nulls));
        assert!(matches(&nulls, &capture(&absent)));
        assert!(matches(&absent, &capture(&nulls)));
    }

    #[test]
    fn flattened_profile_and_icon_overrides_compare_serialized_values() {
        let mut first = message();
        let profile = first.bot_profile.as_mut().unwrap();
        profile.extra.insert("id".into(), json!("BOvERRIDE"));
        profile
            .icons
            .as_mut()
            .unwrap()
            .extra
            .insert("image_36".into(), json!({"unknown":"shadow"}));
        let mut second = first.clone();
        let profile = second.bot_profile.as_mut().unwrap();
        profile.id = Some("BCHANGED".into());
        profile.icons.as_mut().unwrap().image_36 = Some("https://different.invalid/".into());
        assert_eq!(capture(&first), capture(&second));
        assert!(matches(&second, &capture(&first)));
        second
            .bot_profile
            .as_mut()
            .unwrap()
            .icons
            .as_mut()
            .unwrap()
            .extra
            .insert("image_36".into(), json!({"unknown":"different"}));
        assert!(!matches(&second, &capture(&first)));
    }

    #[test]
    fn malformed_snapshots_cannot_equal_a_capture() {
        let message = message();
        for invalid in [Value::Null, json!([]), json!({})] {
            assert!(!matches(&message, &invalid));
        }
        for key in [
            "blocks",
            "edited",
            "bot_id",
            "bot_profile",
            "thread_ts",
            "function_execution_id",
            "is_ephemeral",
        ] {
            let mut snapshot = capture(&message);
            snapshot.as_object_mut().unwrap().remove(key);
            snapshot["unrecognized"] = Value::Null;
            assert!(!matches(&message, &snapshot));
        }
        let mut snapshot = capture(&message);
        snapshot["unrecognized"] = Value::Null;
        assert!(!matches(&message, &snapshot));
    }
}
