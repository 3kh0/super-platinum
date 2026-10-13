use super::*;
fn key(block: &str, action: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: action.into(),
    }
}
fn message() -> Message {
    serde_json::from_value(json!({"blocks":[
 {"type":"section","block_id":"s","accessory":{"type":"timepicker","action_id":"time"}},
 {"type":"actions","block_id":"a","elements":[{"type":"timepicker","action_id":"empty"},{"type":"timepicker","action_id":"zone","timezone":"Europe/Berlin"},{"type":"timepicker","action_id":"time","initial_time":"00:00","timezone":"America/New_York"},{"type":"datepicker","action_id":"date","initial_date":"2026-10-13"}]}
]})).unwrap()
}
#[test]
fn lazy_time_seed_distinguishes_missing_timezone_only_and_midnight() {
    let mut inputs = MessageInputs::parse(&message()).unwrap();
    assert_eq!(inputs.values().unwrap(), None);
    inputs.initialize();
    assert_eq!(
        inputs.values().unwrap().unwrap(),
        json!({"s":{"time":{"type":"timepicker"}},"a":{"zone":{"type":"timepicker","timezone":"Europe/Berlin"},"time":{"type":"timepicker","selected_time":"00:00","timezone":"America/New_York"},"date":{"type":"datepicker","selected_date":"2026-10-13"}}})
    );
    let before = inputs.clone();
    for invalid in ["24:00", "09:60", "9:00", ""] {
        assert!(inputs.commit(&key("a", "zone"), Some(invalid)).is_err());
        assert_eq!(inputs, before);
    }
    inputs.commit(&key("a", "zone"), Some("23:59")).unwrap();
    let values = inputs.values().unwrap().unwrap();
    assert_eq!(
        values["a"]["zone"],
        json!({"type":"timepicker","selected_time":"23:59","timezone":"Europe/Berlin"})
    );
    assert_eq!(
        values["a"]["date"],
        json!({"type":"datepicker","selected_date":"2026-10-13"})
    );
    inputs.commit(&key("a", "zone"), None).unwrap();
    assert_eq!(
        inputs.values().unwrap().unwrap()["a"]["zone"],
        json!({"type":"timepicker","selected_time":null,"timezone":"Europe/Berlin"})
    );
}
#[test]
fn timezone_update_retains_wire_and_display_until_new_interaction() {
    let mut message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    inputs.commit(&key("a", "time"), Some("09:15")).unwrap();
    let before = inputs.values().unwrap();
    message.blocks[1]["elements"][2]["timezone"] = json!("Asia/Tokyo");
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("a", "time")].value,
        InputValue::Time(Some("09:15".into()))
    );
    assert_eq!(inputs.values().unwrap(), before);
    message.blocks[1]["elements"][2]["initial_time"] = json!("11:30");
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key("a", "time")].value,
        InputValue::Time(Some("11:30".into()))
    );
    assert_eq!(inputs.values().unwrap(), before);
    inputs.commit(&key("a", "time"), Some("15:45")).unwrap();
    assert_eq!(
        inputs.values().unwrap().unwrap()["a"]["time"],
        json!({"type":"timepicker","selected_time":"15:45","timezone":"Asia/Tokyo"})
    );
}
#[test]
fn date_time_replacements_read_only_the_matching_saved_property() {
    let mut message = message();
    let mut inputs = MessageInputs::parse(&message).unwrap();
    let key = key("s", "time");
    inputs.commit(&key, Some("09:30")).unwrap();
    message.blocks[0]["accessory"] =
        json!({"type":"datepicker","action_id":"time","initial_date":"2026-10-13"});
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key].value,
        InputValue::Date(Some("2026-10-13".into()))
    );
    message.blocks[0]["accessory"] =
        json!({"type":"timepicker","action_id":"time","initial_time":"12:00"});
    inputs.reconcile(&message).unwrap();
    assert_eq!(
        inputs.fields[&key].value,
        InputValue::Time(Some("09:30".into()))
    );
}
