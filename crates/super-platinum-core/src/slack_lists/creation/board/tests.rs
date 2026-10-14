use super::*;

fn fixture(filters: Value) -> (Document, PersonalTaskLists) {
    let document = Document::parse(json!({"id":"F_LIST","mode":"list","access":"write",
        "is_locked":false,"list_limits":{"over_row_maximum":false,"row_count":3},
        "list_metadata":{"schema":[
            {"id":"C_NAME","key":"name","name":"Name","type":"text"},
            {"id":"C_DONE","key":"done","name":"Completed","type":"todo_completed"},
            {"id":"C_STAGE","key":"stage","name":"Stage","type":"select","options":{"choices":[
                {"value":"one","label":"One"},{"value":"two","label":"Two"}]}},
            {"id":"C_USER","key":"people","name":"People","type":"user"},
            {"id":"C_RATING","key":"rating","name":"Rating","type":"rating"},
            {"id":"C_CHECK","key":"check","name":"Checked","type":"checkbox"},
            {"id":"C_DATE","key":"date","name":"Date","type":"date"}],
            "views":[{"id":"V_BOARD","name":"Board","type":"board","filters":filters,
                "grouping":{"group_by":"stage","group_by_column_id":"C_STAGE"}}]}}))
    .unwrap();
    let policy = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap();
    (document, policy)
}
fn capture(
    document: &Document,
    policy: &PersonalTaskLists,
    lane: &str,
) -> Result<CreationIntent, ListError> {
    CreationIntent::capture_board(
        document,
        Some("full"),
        Some(policy),
        "V_BOARD",
        lane,
        "U00000001",
    )
}
fn change(document: &Document, path: &str, value: Value) -> Document {
    let mut raw = document.raw.clone();
    *raw.pointer_mut(path).unwrap() = value;
    Document::parse(raw).unwrap()
}
fn receipt(document: &Document, fields: &[Value]) -> Record {
    let fields = fields
        .iter()
        .map(|field| {
            let mut field = field.clone();
            let column = document
                .columns
                .iter()
                .find(|c| field["column_id"] == c.id)
                .unwrap();
            field["key"] = json!(column.key);
            field
        })
        .collect::<Vec<_>>();
    Record::parse(json!({"id":"R_NEW","fields":fields})).unwrap()
}

#[test]
fn board_seeds_first_filter_value_first_key_then_lane_and_completed() {
    let (mut document, policy) = fixture(json!([
        {"key":"name","operator":"contains","values":["Task","ignored"]},
        {"key":"people","operator":"is_me","values":["viewer","U00000002"]},
        {"key":"rating","operator":"equal","values":["3"]},
        {"key":"check","operator":"is_not_empty","values":[]},
        {"key":"date","operator":"equal","values":["2026-10-14"]},
        {"key":"name","operator":"equal","values":["ignored duplicate"]}
    ]));
    document.raw["list_metadata"]["views"][0]["default_view_key"] = json!("completed_items");
    document.raw["list_metadata"]["views"][0]["info_column_filters"] = json!([
        {"key":"people","operator":"equal","values":["U00000009"]}
    ]);
    document = Document::parse(document.raw).unwrap();
    let intent = capture(&document, &policy, "two").unwrap();
    let fields = intent.initial_fields().unwrap().as_array().unwrap();
    assert_eq!(
        fields
            .iter()
            .map(|f| f["column_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "C_NAME", "C_USER", "C_RATING", "C_CHECK", "C_STAGE", "C_DONE"
        ]
    );
    assert_eq!(
        fields[0]["rich_text"][0]["elements"][0]["elements"][0],
        json!({"type":"text","text":"Task"})
    );
    assert_eq!(fields[1]["user"], json!(["U00000001"]));
    assert_eq!(fields[2]["rating"], json!([3]));
    assert_eq!(fields[3]["checkbox"], true);
    assert_eq!(fields[4]["select"], json!(["two"]));
    assert_eq!(fields[5]["checkbox"], true);
    let record = receipt(&document, fields);
    validate_initial_fields(&document, intent.initial_fields(), &record).unwrap();
}

#[test]
fn truthiness_does_not_invent_checkbox_viewer_or_zero_rating_defaults() {
    let (document, policy) = fixture(json!([
        {"key":"name","operator":"is_not_empty","values":[]},
        {"key":"people","operator":"is_me","values":[]},
        {"key":"rating","operator":"equal","values":["0"]},
        {"key":"check","operator":"equal","values":[true]}
    ]));
    let intent = capture(&document, &policy, "").unwrap();
    assert_eq!(
        intent.initial_fields(),
        Some(&json!([
            {"column_id":"C_NAME","rich_text":[]}, {"column_id":"C_STAGE","select":[]}
        ]))
    );
    let document = change(
        &document,
        "/list_metadata/views/0/grouping",
        json!({"group_by":"check","group_by_column_id":"C_CHECK"}),
    );
    let intent = capture(&document, &policy, "").unwrap();
    assert_eq!(
        intent.initial_fields().unwrap()[1],
        json!({"column_id":"C_CHECK","checkbox":false})
    );
    assert!(capture(&document, &policy, "false").is_err());
}

#[test]
fn source_order_retains_identical_duplicates_but_refuses_conflicts() {
    let (document, policy) =
        fixture(json!([{"key":"stage","operator":"equal","values":["one","two"]}]));
    let intent = capture(&document, &policy, "one").unwrap();
    assert_eq!(
        intent.initial_fields(),
        Some(&json!([
            {"column_id":"C_STAGE","select":["one"]},{"column_id":"C_STAGE","select":["one"]}
        ]))
    );
    assert!(capture(&document, &policy, "two").is_err());
    assert!(capture(&document, &policy, "").is_err());
    let invalid_choice = change(
        &document,
        "/list_metadata/views/0/filters/0/values",
        json!(["removed"]),
    );
    assert_eq!(
        capture(&invalid_choice, &policy, "two")
            .unwrap()
            .initial_fields(),
        Some(&json!([
            {"column_id":"C_STAGE","select":["two"]}
        ]))
    );
    let mut raw = document.raw.clone();
    raw["list_metadata"]["views"][0]["filters"] = json!([]);
    raw["list_metadata"]["views"][0]["grouping"] =
        json!({"group_by":"done","group_by_column_id":"C_DONE"});
    raw["list_metadata"]["views"][0]["default_view_key"] = json!("completed_items");
    let completed = Document::parse(raw).unwrap();
    assert!(capture(&completed, &policy, "").is_err());
    assert_eq!(
        capture(&completed, &policy, "true")
            .unwrap()
            .initial_fields()
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn board_scope_refusals_and_preflight_preserve_existing_authority() {
    let (document, policy) = fixture(json!([]));
    let intent = capture(&document, &policy, "one").unwrap();
    for (path, value) in [
        ("/list_metadata/views/0/type", json!("table")),
        (
            "/list_metadata/views/0/grouping/group_by_column_id",
            json!("replaced"),
        ),
        ("/list_metadata/views/0/grouping/group_by", json!("date")),
        ("/list_limits/over_row_maximum", json!(true)),
        ("/access", json!("read")),
    ] {
        let changed = change(&document, path, value);
        assert!(capture(&changed, &policy, "one").is_err(), "{path}");
        assert!(
            intent
                .revalidate(&changed, Some("full"), Some(&policy), Some("V_BOARD"))
                .is_err()
        );
    }
    let changed = change(
        &document,
        "/list_metadata/views/0/filters",
        json!([
            {"key":"name","operator":"equal","values":["Changed"]}
        ]),
    );
    assert!(
        intent
            .revalidate(&changed, Some("full"), Some(&policy), Some("V_BOARD"))
            .is_err()
    );
    let mut grouped = document.raw.clone();
    grouped["list_metadata"]["views"][0]["filter_set"] = json!({"match":"all","groups":[]});
    assert!(capture(&Document::parse(grouped).unwrap(), &policy, "one").is_err());
    assert!(capture(&document, &policy, "removed").is_err());
    let ungrouped = change(&document, "/list_metadata/views/0/grouping", json!({}));
    assert!(
        capture(&ungrouped, &policy, "")
            .unwrap()
            .initial_fields()
            .is_none()
    );
    assert!(capture(&ungrouped, &policy, "one").is_err());
}

#[test]
fn rich_text_seeds_use_source_full_url_rule_and_empty_array_distinction() {
    for (input, kind) in [
        ("https://example.com/\u{1f680}", "link"),
        ("example.com", "text"),
        (" https://example.com/", "text"),
    ] {
        let (document, policy) =
            fixture(json!([{"key":"name","operator":"contains","values":[input]}]));
        let intent = capture(&document, &policy, "one").unwrap();
        assert_eq!(
            intent.initial_fields().unwrap()[0]["rich_text"][0]["elements"][0]["elements"][0]["type"],
            kind
        );
    }
}

#[test]
fn typed_receipt_validator_checks_every_seed_not_only_checkbox() {
    let (document, policy) = fixture(json!([
        {"key":"name","operator":"contains","values":["Task"]},
        {"key":"people","operator":"equal","values":["U00000001"]},
        {"key":"rating","operator":"equal","values":[3]},
        {"key":"check","operator":"is_not_empty","values":[]}
    ]));
    let intent = capture(&document, &policy, "one").unwrap();
    let record = receipt(
        &document,
        intent.initial_fields().unwrap().as_array().unwrap(),
    );
    validate_initial_fields(&document, intent.initial_fields(), &record).unwrap();
    for (key, member, bad) in [
        ("name", "rich_text", json!([])),
        ("people", "user", json!([])),
        ("rating", "rating", json!([4])),
        ("check", "checkbox", json!(false)),
        ("stage", "select", json!(["two"])),
    ] {
        let mut changed = record.clone();
        changed.fields.get_mut(key).unwrap()[member] = bad;
        assert!(
            validate_initial_fields(&document, intent.initial_fields(), &changed).is_err(),
            "{key}"
        );
        changed.fields.remove(key);
        assert!(validate_initial_fields(&document, intent.initial_fields(), &changed).is_err());
    }
    let mut camel = record.clone();
    let field = camel
        .fields
        .get_mut("name")
        .unwrap()
        .as_object_mut()
        .unwrap();
    let rich = field.remove("rich_text").unwrap();
    field.insert("richText".into(), rich);
    validate_initial_fields(&document, intent.initial_fields(), &camel).unwrap();
    camel.fields.get_mut("name").unwrap()["rich_text"] = json!([]);
    assert!(validate_initial_fields(&document, intent.initial_fields(), &camel).is_err());
    let mut extra = record.clone();
    extra.fields.get_mut("stage").unwrap()["user"] = json!(["U00000001"]);
    assert!(validate_initial_fields(&document, intent.initial_fields(), &extra).is_err());

    // Integration hook: the public receipt path must use the typed validator.
    let mut response = json!({"ok":true,"record":record.raw});
    intent.validate_response(&response).unwrap();
    response["record"]["fields"][0]["rich_text"] = json!([]);
    assert!(intent.validate_response(&response).is_err());
}

#[test]
fn multi_select_assignee_and_board_wire_use_existing_single_create_protocol() {
    use crate::config::WorkspaceSession;
    use crate::slack::{
        api,
        client::{RequestBody, SlackClient},
    };
    let (document, policy) = fixture(json!([
        {"key":"stage","operator":"contains","values":["one","two"]},
        {"key":"people","operator":"equal","values":["U00000001|W00000002"]}
    ]));
    let mut raw = document.raw.clone();
    raw["list_metadata"]["schema"][2]["type"] = json!("multi_select");
    raw["list_metadata"]["schema"][3]["type"] = json!("assignee");
    raw["list_metadata"]["views"][0]["grouping"] =
        json!({"group_by":"check","group_by_column_id":"C_CHECK"});
    let document = Document::parse(raw).unwrap();
    let intent = capture(&document, &policy, "").unwrap();
    assert_eq!(
        intent.initial_fields(),
        Some(&json!([
            {"column_id":"C_STAGE","select":["one"]},
            {"column_id":"C_USER","user":["U00000001","W00000002"]},
            {"column_id":"C_CHECK","checkbox":false}
        ]))
    );
    let workspace = WorkspaceSession {
        team_id: "T_TEST".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let mut clock = ListClock::new();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let timestamp = clock.tick(now_ms).unwrap();
    let request = api::list_record_create(&SlackClient::default(), &workspace, &intent, &timestamp);
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form expected");
    };
    let encoded = fields
        .iter()
        .find(|(key, _)| key == "initial_fields")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&encoded.1).unwrap(),
        *intent.initial_fields().unwrap()
    );
    assert!(fields.contains(&("position".into(), String::new())));
    assert!(fields.contains(&("updated_timestamp".into(), timestamp.to_string())));
    assert!(!fields.iter().any(|(key, _)| matches!(
        key.as_str(),
        "view_id" | "fields" | "parent_record_id" | "duplicated_record_id" | "client_id"
    )));
}

#[test]
fn rating_conversion_and_invalid_reference_bounds_are_explicit() {
    for (input, expected) in [
        (json!(" 3 "), Some(json!(3))),
        (json!("0x10"), Some(json!(16))),
        (json!(true), Some(json!(1))),
        (json!("0"), None),
        (json!("not a number"), None),
        (Value::Null, None),
    ] {
        assert_eq!(rating(Some(&input)).unwrap().map(Value::Number), expected);
    }
    assert!(rating(Some(&json!("Infinity"))).is_err());
    assert!(rating(Some(&json!("0xffffffffffffffffffff"))).is_err());
    let (document, policy) = fixture(json!([{"key":"people","operator":"equal","values":["bad"]}]));
    assert!(capture(&document, &policy, "one").is_err());
    let (document, policy) =
        fixture(json!([{"key":"people","operator":"is_me","values":["viewer"]}]));
    assert!(
        CreationIntent::capture_board(
            &document,
            Some("full"),
            Some(&policy),
            "V_BOARD",
            "one",
            "bad"
        )
        .is_err()
    );
}

#[test]
fn select_filter_checks_whole_choice_before_splitting_wire_values() {
    let (document, policy) = fixture(json!([
        {"key":"stage","operator":"equal","values":["one|two"]}
    ]));
    let document = change(
        &document,
        "/list_metadata/views/0/grouping",
        json!({"group_by":"check","group_by_column_id":"C_CHECK"}),
    );
    // Separate matching choices do not authorize a combined filter value.
    assert_eq!(
        capture(&document, &policy, "").unwrap().initial_fields(),
        Some(&json!([{"column_id":"C_CHECK","checkbox":false}]))
    );
    let combined = change(
        &document,
        "/list_metadata/schema/2/options/choices",
        json!([{"value":"one|two","label":"Combined"}]),
    );
    // Only the whole literal is in the schema; its wire value is then split.
    assert_eq!(
        capture(&combined, &policy, "").unwrap().initial_fields(),
        Some(&json!([
            {"column_id":"C_STAGE","select":["one","two"]},
            {"column_id":"C_CHECK","checkbox":false}
        ]))
    );
}

#[test]
fn ungrouped_board_ignores_populated_filter_defaults() {
    let (document, policy) = fixture(json!([
        {"key":"name","operator":"contains","values":["Not a creation seed"]},
        {"key":"people","operator":"is_me","values":["viewer"]}
    ]));
    let mut raw = document.raw.clone();
    raw["list_metadata"]["views"][0]["grouping"] = json!({});
    raw["list_metadata"]["views"][0]["info_column_filters"] = json!([
        {"key":"rating","operator":"equal","values":[3]}
    ]);
    let ungrouped = Document::parse(raw).unwrap();
    assert!(
        capture(&ungrouped, &policy, "")
            .unwrap()
            .initial_fields()
            .is_none()
    );
    assert!(capture(&ungrouped, &policy, "one").is_err());
}

#[test]
fn receipt_unknown_nonnull_metadata_cannot_confirm_a_seed() {
    let (document, policy) = fixture(json!([]));
    let intent = capture(&document, &policy, "one").unwrap();
    let record = receipt(
        &document,
        intent.initial_fields().unwrap().as_array().unwrap(),
    );
    for value in [json!(false), json!(0), json!(""), json!([]), json!({})] {
        let mut changed = record.clone();
        changed.fields.get_mut("stage").unwrap()["future_metadata"] = value;
        assert!(validate_initial_fields(&document, intent.initial_fields(), &changed).is_err());
    }
    let mut null_metadata = record;
    null_metadata.fields.get_mut("stage").unwrap()["future_metadata"] = Value::Null;
    validate_initial_fields(&document, intent.initial_fields(), &null_metadata).unwrap();
}

#[test]
fn board_preflight_accepts_last_read_only_change_without_rebuilding_seeds() {
    let (document, policy) = fixture(json!([
        {"key":"people","operator":"is_me","values":["viewer"]}
    ]));
    let intent = capture(&document, &policy, "one").unwrap();
    let seeds = intent.initial_fields().cloned();
    let mut raw = document.raw.clone();
    raw["last_read"] = json!(100);
    let fresh = Document::parse(raw).unwrap();
    intent
        .revalidate(&fresh, Some("full"), Some(&policy), Some("V_BOARD"))
        .unwrap();
    assert_eq!(intent.initial_fields(), seeds.as_ref());
}
