use super::*;
fn row(value: Value) -> groups::Snapshot {
    groups::Snapshot::capture(value.clone(), value).unwrap()
}
fn field(id: &str, lowered: &str, label: &str) -> Field {
    Field {
        id: id.into(),
        lowered: lowered.into(),
        label: label.into(),
    }
}
#[test]
fn name_handle_stored_alias_and_model_names_all_join_shared_observation() {
    let row = row(
        json!({"id":"SGROUP001","name":" Actual Name ","_name_lc":" Stored Alias ",
        "handle":"@@Hand\nSecond","is_usergroup":true}),
    );
    let mut fields = BTreeSet::new();
    let mut collation = BTreeSet::from(["Member 2".into()]);
    extend(
        &row,
        &field(row.id(), " actual name ", "@@@Hand\nSecond"),
        &mut fields,
        &mut collation,
    )
    .unwrap();
    for expected in [
        " Actual Name ",
        " Stored Alias ",
        "@@Hand\nSecond",
        "@Hand\nSecond",
        "@Hand",
        "Second",
    ] {
        assert!(fields.contains(expected), "{expected}");
    }
    assert!(collation.contains("Member 2"));
    assert!(collation.contains("@@Hand\nSecond") && collation.contains(" Actual Name "));
    assert!(!fields.contains(" actual name ")); // stored scoring alias wins
}
#[test]
fn missing_or_empty_alias_uses_untrimmed_locale_observation_then_shared_hndq() {
    for stored in [Value::Null, json!("")] {
        let row = row(json!({"id":"SGROUP001","name":" I Name ","_name_lc":stored,"handle":"@"}));
        let mut fields = BTreeSet::new();
        let mut collation = BTreeSet::new();
        extend(
            &row,
            &field(row.id(), " ? name ", "@@"),
            &mut fields,
            &mut collation,
        )
        .unwrap();
        assert!(fields.contains(" ? name "));
        assert!(fields.contains("@") && fields.contains(""));
        assert_eq!(collation, BTreeSet::from([" I Name ".into()]));
        let names = groups::Names::observe(
            &row,
            |s| {
                assert_eq!(s, " I Name ");
                Ok(" ? name ".into())
            },
            |s| {
                assert!(fields.contains(s), "missing normalization: {s}");
                Ok(super_platinum_core::block_actions::home::workflow::directory_local::normalize_lowered_query(s.trim()))
            },
        );
        assert!(names.is_ok());
    }
}
#[test]
fn initial_comparison_uses_actual_marker_and_final_model_fallback_is_observed() {
    for marker in [
        Value::Null,
        json!(false),
        json!(0),
        json!(""),
        json!(true),
        json!(1),
    ] {
        let row = row(
            json!({"id":"SGROUP001","name":"","string":"Model fallback","handle":"Handle","is_usergroup":marker}),
        );
        let mut fields = BTreeSet::new();
        let mut collation = BTreeSet::new();
        extend(
            &row,
            &field(row.id(), "", "@Handle"),
            &mut fields,
            &mut collation,
        )
        .unwrap();
        assert!(collation.contains(if truth(&marker) { "Handle" } else { "" }));
        assert!(collation.contains("Model fallback") && fields.contains("Model fallback"));
    }
}
#[test]
fn renderer_input_retains_missing_null_false_zero_and_exact_handle_for_literal_labels() {
    let missing = input(&row(json!({"id":"SGROUP001"}))).unwrap();
    assert!(missing.get("handle").is_none());
    for handle in [
        Value::Null,
        json!(false),
        json!(0),
        json!(""),
        json!("@Cafe?"),
    ] {
        let input = input(&row(json!({"id":"SGROUP001","handle":handle}))).unwrap();
        assert_eq!(input["handle"], handle);
    }
}
fn reply(inputs: &[Value]) -> Reply {
    Reply {
        ticket: "ticket".into(),
        stage: 4,
        locale: "en-US".into(),
        inputs: inputs.to_vec(),
        fields: inputs
            .iter()
            .map(|row| field(row["id"].as_str().unwrap(), "lower", "@label"))
            .collect(),
    }
}
#[test]
fn exact_reply_identity_locale_order_and_original_fields_are_required() {
    let inputs = vec![
        json!({"id":"SGROUP001","name":"First"}),
        json!({"id":"SGROUP002","name":"Second"}),
    ];
    assert!(decode("ticket", 4, "en-US", &inputs, reply(&inputs)).is_ok());
    for change in 0..6 {
        let mut bad = reply(&inputs);
        match change {
            0 => bad.ticket = "foreign".into(),
            1 => bad.stage = 5,
            2 => bad.locale = "tr-TR".into(),
            3 => bad.inputs[0]["name"] = json!("Replaced"),
            4 => bad.fields.reverse(),
            _ => {
                bad.fields.pop();
            }
        }
        assert!(decode("ticket", 4, "en-US", &inputs, bad).is_err());
    }
}
#[test]
fn malformed_truthy_group_fields_and_wrong_row_receipt_refuse() {
    assert!(
        groups::Snapshot::capture(
            json!({"id":"SGROUP001"}),
            json!({"id":"SGROUP001","handle":7})
        )
        .is_err()
    );
    let row = row(json!({"id":"SGROUP001","name":"Name"}));
    assert!(
        extend(
            &row,
            &field("SGROUP002", "name", "@handle"),
            &mut BTreeSet::new(),
            &mut BTreeSet::new()
        )
        .is_err()
    );
}

#[test]
fn same_id_label_does_not_authorize_changed_source_or_effective_record() {
    let original = row(json!({"id":"SGROUP001","name":"Name","handle":"same","opaque":1}));
    let originals = BTreeMap::from([(original.id().to_owned(), original.clone())]);
    assert!(original_pair(&originals, &original));
    let mut changed = original.source().clone();
    changed["opaque"] = json!(2);
    let raw_change = groups::Snapshot::capture(changed, original.effective().clone()).unwrap();
    assert!(!original_pair(&originals, &raw_change));
    let mut changed = original.effective().clone();
    changed["date_update"] = json!(42);
    let effective_change = groups::Snapshot::capture(original.source().clone(), changed).unwrap();
    assert!(!original_pair(&originals, &effective_change));
    assert!(!original_pair(
        &originals,
        &row(json!({"id":"SGROUP002","name":"Name","handle":"same"}))
    ));
}
