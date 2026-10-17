use super::*;
#[test]
fn configured_number_gate_is_exact_and_legacy_mode_identity_stays_default() {
    for alias in [
        "slack-list-number-precision-field-create",
        "slack-list-number-precision-field-create-narrow",
    ] {
        assert_eq!(
            configured_number_gate("1", true, Some(alias)),
            Some(FieldCreateMode::NumberPrecision)
        );
        assert_eq!(configured_number_gate("1", false, Some(alias)), None);
        assert_eq!(configured_number_gate("true", true, Some(alias)), None);
    }
    for alias in [
        None,
        Some("slack-list-number-field-create"),
        Some("slack-list-text-field-create"),
        Some("slack-list-number-precision-field-create-other"),
    ] {
        assert_eq!(configured_number_gate("1", true, alias), None);
    }
    assert_eq!(
        FieldCreateMode::Number.kind(),
        FieldCreateMode::NumberPrecision.kind()
    );
    assert_ne!(
        FieldCreateMode::Number.prefix(),
        FieldCreateMode::NumberPrecision.prefix()
    );
}
#[test]
fn synthetic_addition_preserves_all_metadata_and_uses_exact_configured_options() {
    let mut before = json!({"list_metadata":{"schema":[{"id":"C_NAME","key":"NAME","type":"text"}],"views":[{"id":"V_FILTER","columns":[{"id":"C_NAME","key":"NAME","visible":true}],"sorts":[{"key":"score","ascending":true}],"filters":[],"info_column_filters":[]}],"opaque":{"keep":7}}});
    let original = before.clone();
    add_field(&mut before, FieldCreateMode::NumberPrecision).unwrap();
    let added = before["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    before["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert_eq!(before, original);
    assert_eq!(added["options"], json!({"precision":3}));
    assert_eq!(added["type"], "number");
    assert_eq!(added["id"], "C_NUMBER_OBSERVED");
    let mut default = original;
    add_field(&mut default, FieldCreateMode::Number).unwrap();
    assert_eq!(
        default["list_metadata"]["schema"][1]["options"],
        json!({"precision":0})
    );
}
