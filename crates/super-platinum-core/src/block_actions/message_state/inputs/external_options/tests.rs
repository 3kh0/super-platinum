use super::*;
fn option(value: &str) -> ExternalOption {
    ExternalOption::parse(&json!({"text":{"type":"plain_text","text":value,"emoji":false},"value":value,"description":{"type":"plain_text","text":"raw description"}})).unwrap()
}
fn key(id: &str) -> InputKey {
    InputKey {
        block_id: "b".into(),
        action_id: id.into(),
    }
}
fn message() -> Message {
    serde_json::from_value(json!({"ts":"1.000001","blocks":[{"type":"actions","block_id":"b","elements":[{"type":"multi_external_select","action_id":"a","initial_options":[option("initial").raw()]},{"type":"multi_external_select","action_id":"s","initial_options":[option("sibling").raw()]},{"type":"multi_external_select","action_id":"empty","initial_options":[]},{"type":"multi_external_select","action_id":"absent"}]}]})).unwrap()
}
#[test]
fn ordered_raw_commit_initial_siblings_and_explicit_clear() {
    let mut inputs = MessageInputs::parse(&message()).unwrap();
    assert!(inputs.values().unwrap().is_none());
    inputs
        .commit_external_options(&key("a"), &[option("z"), option("a")])
        .unwrap();
    let wire = inputs.values().unwrap().unwrap();
    assert_eq!(
        wire["b"]["a"]["selected_options"],
        json!([option("z").raw(), option("a").raw()])
    );
    assert_eq!(
        wire["b"]["s"]["selected_options"],
        json!([option("sibling").raw()])
    );
    assert_eq!(wire["b"]["empty"]["selected_options"], json!([]));
    assert!(wire["b"].get("absent").is_none());
    inputs.commit_external_options(&key("a"), &[]).unwrap();
    assert_eq!(inputs.saved_external_options(&key("a")), Some(vec![]));
}
#[test]
fn outer_initial_removal_resets_display_without_rewriting_committed_wire() {
    let mut raw = message();
    let mut inputs = MessageInputs::parse(&raw).unwrap();
    inputs
        .commit_external_options(&key("a"), &[option("remote")])
        .unwrap();
    raw.blocks[0]["elements"][0]
        .as_object_mut()
        .unwrap()
        .remove("initial_options");
    inputs.reconcile(&raw).unwrap();
    assert_eq!(
        inputs.fields[&key("a")].value,
        InputValue::ExternalOptions(vec![])
    );
    assert_eq!(
        inputs.saved_external_options(&key("a")),
        Some(vec![option("remote")])
    );
    raw.blocks[0]["elements"].as_array_mut().unwrap().remove(0);
    inputs.reconcile(&raw).unwrap();
    assert_eq!(inputs.saved_external_options(&key("a")), None);
}
#[test]
fn invalid_commit_is_atomic_and_untouched_overmax_sibling_retains_raw() {
    let mut raw = message();
    raw.blocks[0]["elements"][1]["initial_options"] =
        json!([option("s1").raw(), option("s2").raw()]);
    raw.blocks[0]["elements"][1]["max_selected_items"] = json!(1);
    let mut inputs = MessageInputs::parse(&raw).unwrap();
    inputs
        .commit_external_options(&key("a"), &[option("chosen")])
        .unwrap();
    let before = inputs.values().unwrap();
    assert_eq!(
        before.as_ref().unwrap()["b"]["s"]["selected_options"],
        json!([option("s1").raw(), option("s2").raw()])
    );
    assert!(
        inputs
            .commit_external_options(&key("s"), &[option("s1"), option("s2")])
            .is_err()
    );
    assert_eq!(inputs.values().unwrap(), before);
    raw.blocks[0]["elements"][0]["initial_options"] = json!([{"value":"bad"}]);
    assert!(inputs.reconcile(&raw).is_err());
    assert_eq!(inputs.values().unwrap(), before);
}
