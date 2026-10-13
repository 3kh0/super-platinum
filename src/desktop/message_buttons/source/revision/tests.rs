use super::*;

fn message() -> Message {
    serde_json::from_value(json!({
        "ts":"1.0", "channel":"C1", "type":"message", "user":"U1", "bot_id":"B1",
        "bot_profile":{"id":"B1","name":"Helper","app_id":"A1","team_id":"T1",
            "icons":{"image_72":"https://example.test/icon","unknown":[1,null]}},
        "blocks":[{"type":"actions","block_id":"review","elements":[{
            "type":"button","action_id":"approve","value":"v1",
            "confirm":{"text":{"type":"mrkdwn","text":"Approve *this*?"}}
        }]}, {"type":"section","text":{"type":"plain_text","text":"Details"}}]
    }))
    .unwrap()
}

#[test]
fn matches_exact_capture_for_every_revision_field_and_unrelated_changes() {
    let original = message();
    let snapshot = capture(&original);
    assert!(matches(&original, &snapshot));
    let raw = serde_json::to_value(&original).unwrap();
    for (key, value) in [
        ("blocks", json!([])),
        ("edited", json!({"ts":"2.0","user":"U2"})),
        ("bot_id", json!("B2")),
        ("bot_profile", Value::Null),
        ("thread_ts", json!("0.9")),
        ("channel", json!("C2")),
        ("subtype", json!("bot_message")),
        ("type", json!("other")),
        ("user", json!("U2")),
        ("function_execution_id", json!({"unexpected":[1,null]})),
        ("is_ephemeral", json!("false")),
        ("app_id", json!(["A2"])),
    ] {
        let mut changed = raw.clone();
        changed[key] = value;
        let changed: Message = serde_json::from_value(changed).unwrap();
        assert_ne!(
            capture(&changed),
            snapshot,
            "field {key} belongs to revision"
        );
        assert!(
            !matches(&changed, &snapshot),
            "field {key} must retire revision"
        );
        assert!(matches(&changed, &capture(&changed)), "fresh field {key}");
    }
    let mut changed = original.clone();
    changed.text = Some("Updated fallback".into());
    changed
        .extra
        .insert("unrelated".into(), json!({"anything":true}));
    assert_eq!(capture(&changed), snapshot);
    assert!(matches(&changed, &snapshot));
}

#[test]
fn raw_nested_action_content_and_order_remain_exact() {
    let original = message();
    let snapshot = capture(&original);
    for case in 0..5 {
        let mut changed = original.clone();
        match case {
            0 => changed.blocks.swap(0, 1),
            1 => changed.blocks[0]["elements"][0]["value"] = json!("v2"),
            2 => {
                changed.blocks[0]["elements"][0]["confirm"]["text"]["text"] =
                    json!("New confirmation")
            }
            3 => changed.blocks[0]["elements"][0]["unknown"] = json!({"nested":[1,null,true]}),
            _ => changed.blocks[0]["elements"][0]["action_id"] = json!("different"),
        }
        assert_ne!(capture(&changed), snapshot);
        assert!(!matches(&changed, &snapshot), "case {case}");
    }
}

#[test]
fn optional_nulls_and_profile_flattening_match_serialization() {
    let original = message();
    let snapshot = capture(&original);
    let mut changed = original.clone();
    changed.edited = Some(Value::Null);
    for key in ["function_execution_id", "is_ephemeral", "app_id"] {
        changed.extra.insert(key.into(), Value::Null);
    }
    assert_eq!(capture(&changed), snapshot);
    assert!(matches(&changed, &snapshot));

    // Flattened extra keys may collide with known fields. Comparing typed
    // fields by hand would incorrectly reject this serialization-equivalent edit.
    let profile = changed.bot_profile.as_mut().unwrap();
    profile.extra.insert("id".into(), json!("OVERRIDE"));
    profile
        .icons
        .as_mut()
        .unwrap()
        .extra
        .insert("image_72".into(), Value::Null);
    let flattened = capture(&changed);
    let profile = changed.bot_profile.as_mut().unwrap();
    profile.id = Some("IGNORED".into());
    profile.icons.as_mut().unwrap().image_72 = Some("ignored".into());
    assert_eq!(capture(&changed), flattened);
    assert!(matches(&changed, &flattened));
    changed
        .bot_profile
        .as_mut()
        .unwrap()
        .extra
        .insert("unknown".into(), json!([true, {"x":null}]));
    assert_ne!(capture(&changed), flattened);
    assert!(!matches(&changed, &flattened));
    changed.bot_profile = None;
    assert!(matches(&changed, &capture(&changed)));
    assert!(!matches(&changed, &flattened));
}

#[test]
fn malformed_snapshot_cannot_match_a_valid_capture() {
    let original = message();
    let snapshot = capture(&original);
    for key in snapshot.as_object().unwrap().keys() {
        let mut malformed = snapshot.clone();
        malformed.as_object_mut().unwrap().remove(key);
        assert!(!matches(&original, &malformed), "missing {key}");
        malformed["unexpected"] = Value::Null;
        assert!(!matches(&original, &malformed), "replaced {key}");
    }
    let mut extra = snapshot.clone();
    extra["unexpected"] = Value::Null;
    assert!(!matches(&original, &extra));
    for invalid in [Value::Null, json!([]), json!("invalid")] {
        assert!(!matches(&original, &invalid));
    }
}
