use super::*;
use crate::slash_commands::{Owner, api, catalog::Catalog};
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, client::RequestBody},
};

fn cases() -> Vec<Value> {
    serde_json::from_str(include_str!("cases.json")).unwrap()
}
fn table(version: bool) -> Arc<Value> {
    let tables: Value = serde_json::from_str(include_str!("emoji-fixture.json")).unwrap();
    Arc::new(tables[if version { "17" } else { "16" }].clone())
}
fn owner(thread: bool) -> Owner {
    Owner {
        account: "c765fef0-1f85-49f0-ab9c-fdb779292337".into(),
        team: "T1".into(),
        user: "U1".into(),
        channel: "C1".into(),
        thread: thread.then(|| "1.000001".into()),
    }
}
fn normalized(converted: &Converted, thread: bool) -> Value {
    let owner = owner(thread);
    let catalog = Catalog::decode(
        owner.clone(),
        json!({"ok":true,"app_actions":[],"commands":[{
            "type":"app","name":"/demo","app":"A1","teams":["T1"],"is_runnable_from_thread":true
        }]}),
    )
    .unwrap();
    // Synthetic request builder only; this envelope does not mint a protected Claim.
    let frozen = api::Frozen::capture(
        owner,
        7,
        json!({"delta":converted.original()}),
        converted.display().into(),
        converted.blocks().cloned(),
        &catalog.choice(0).unwrap(),
        &catalog,
    )
    .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://synthetic.slack.com".into(),
        token: "synthetic-token".into(),
    };
    let request =
        api::prepare(&SlackClient::default(), &session, &frozen, "web-synthetic").unwrap();
    assert!(request.url.contains("/api/chat.command"));
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    let mut args = Map::new();
    let mut reason = None;
    for (key, value) in fields {
        if key == "token" {
            assert_eq!(value, "synthetic-token");
            continue;
        }
        if key == "_x_reason" {
            reason = Some(value);
            continue;
        }
        let value = if key == "blocks" {
            serde_json::from_str(&value).unwrap()
        } else {
            Value::String(value)
        };
        assert!(args.insert(key, value).is_none());
    }
    json!({"method":"chat.command","args":args,"reason":reason.unwrap()})
}

#[test]
fn source_goldens_compare_actual_normalized_request_fields_for_mixed_arguments() {
    let mut accepted = 0;
    for case in cases().into_iter().filter(|c| c["accepted"] == true) {
        let emoji = table(case["options"]["emoji17"] == true);
        let converted = convert(&case["ops"], emoji.clone())
            .unwrap_or_else(|e| panic!("{}: {e:?}", case["name"]));
        assert_eq!(converted.original(), &case["ops"]);
        assert!(converted.same_emoji_catalog(&emoji));
        assert_eq!(
            normalized(&converted, case["options"]["thread"] == true),
            case["source"]["request"],
            "{}",
            case["name"]
        );
        accepted += 1;
    }
    assert_eq!(accepted, 43);
}
#[test]
fn unsupported_user_content_is_retained_not_silently_dropped() {
    let mut refused = 0;
    for case in cases().into_iter().filter(|c| c["accepted"] == false) {
        let before = case["ops"].clone();
        assert!(
            convert(&case["ops"], table(false)).is_err(),
            "{}",
            case["name"]
        );
        assert_eq!(case["ops"], before);
        refused += 1;
    }
    assert_eq!(refused, 9);
}
#[test]
fn replacing_equal_emoji_data_is_a_new_generation() {
    let a = table(false);
    let b = Arc::new((*a).clone());
    let input = json!([{"insert":"/demo "},{"insert":{"slackemoji":{"text":":grinning:"}}}]);
    let converted = convert(&input, a.clone()).unwrap();
    assert!(converted.same_emoji_catalog(&a));
    assert!(!converted.same_emoji_catalog(&b));
}
#[test]
fn malformed_and_non_command_shapes_fail_without_coercion() {
    for input in [
        json!(null),
        json!([]),
        json!([{"insert":"/demo","attributes":null}]),
        json!([{"insert":"/demo ","attributes":{"bold":1}}]),
        json!([{"insert":"/demo","delete":1}]),
        json!([{"insert":"/demo","attributes":{"code":true}}]),
        json!([{"insert":" /demo"}]),
        json!([{"insert":"\u{ff0f}demo"}]),
    ] {
        assert!(convert(&input, table(false)).is_err());
    }
}
#[test]
fn malformed_emoji_or_unknown_table_row_cannot_erase_an_embed() {
    for text in [
        ":bad::unknown:",
        ":custom::skin-tone-2-3:",
        ":custom::skin-tone-9:",
    ] {
        assert!(
            convert(
                &json!([{"insert":"/demo "},{"insert":{"slackemoji":{"text":text}}}]),
                table(false)
            )
            .is_err()
        );
    }
    assert!(
        convert(
            &json!([{"insert":"/demo "},{"insert":{"slackemoji":{"text":":grinning:"}}}]),
            Arc::new(json!({"grinning":null}))
        )
        .is_err()
    );
}
#[test]
fn count_and_size_bounds_precede_conversion() {
    assert!(
        convert(
            &Value::Array(vec![json!({"insert":"/demo "}); 101]),
            table(false)
        )
        .is_err()
    );
    assert!(
        convert(
            &json!([{"insert":format!("/demo {}","x".repeat(262144))}]),
            table(false)
        )
        .is_err()
    );
}

#[test]
fn rich_block_source_goldens_compare_actual_full_and_command_requests() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("rich-cases.json")).unwrap();
    let mut admitted = 0;
    for case in cases.into_iter().filter(|c| c["accepted"] == true) {
        let emoji = table(false);
        let converted = convert(&case["ops"], emoji.clone())
            .unwrap_or_else(|e| panic!("{}: {e:?}", case["name"]));
        assert_eq!(
            normalized(&converted, false),
            case["source"]["request"],
            "{}",
            case["name"]
        );
        assert_eq!(
            full_blocks(&case["ops"], &emoji).unwrap(),
            Some(case["full"]["blocks"].clone()),
            "{}",
            case["name"]
        );
        assert_eq!(converted.original(), &case["ops"]);
        admitted += 1;
    }
    assert_eq!(admitted, 24);
}
#[test]
fn rich_first_line_unknown_attrs_and_metadata_dependency_refuse_without_mutation() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("rich-cases.json")).unwrap();
    for case in cases.into_iter().filter(|c| c["accepted"] == false) {
        let before = case["ops"].clone();
        assert!(
            convert(&case["ops"], table(false)).is_err(),
            "{}",
            case["name"]
        );
        assert_eq!(case["ops"], before);
    }
}
