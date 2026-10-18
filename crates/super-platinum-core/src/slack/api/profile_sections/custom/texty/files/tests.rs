use super::*;
use crate::slack::client::RequestBody;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn complete_positive_assignment_only_enables_exact_on() {
    for group in ["on", "off", "control", "treatment", "", "ON"] {
        let policy = Policy::capture(&json!({"ok":true,"assignments":{
            "texty_autocomplete_files":{"group":group}}}))
        .unwrap();
        assert_eq!(policy.enabled(), group == "on");
        assert_eq!(policy.remote_eligible("abcd"), group == "on");
    }
    assert!(
        !Policy::capture(&json!({"ok":true,"assignments":{}}))
            .unwrap()
            .enabled()
    );
    for raw in [
        json!({}),
        json!({"ok":false,"assignments":{}}),
        json!({"ok":true,"error":"failed","assignments":{}}),
        json!({"ok":true}),
        json!({"ok":true,"assignments":[]}),
        json!({"ok":true,"assignments":{"texty_autocomplete_files":null}}),
        json!({"ok":true,"assignments":{"texty_autocomplete_files":{"group":true}}}),
    ] {
        assert!(Policy::capture(&raw).is_err());
    }
}

#[test]
fn exact_request_has_no_category_filter_or_pagination() {
    let owner = WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000001".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    for query in ["", " A & B ", "@cafe", "\u{4e2d}\u{1f642}"] {
        let request = request(&SlackClient::default(), &owner, query);
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.url.split('?').next(),
            Some("https://fixture.invalid/api/search.autocomplete.files")
        );
        let RequestBody::Form(body) = request.body else {
            panic!("form request")
        };
        let mut body: BTreeMap<_, _> = body.into_iter().collect();
        assert_eq!(body.remove("token").as_deref(), Some("synthetic"));
        assert_eq!(
            body,
            BTreeMap::from([
                ("query".into(), query.into()),
                ("include_shares".into(), "true".into()),
                ("_x_reason".into(), "texty-autocomplete".into())
            ])
        );
    }
}

#[test]
fn response_keeps_duplicates_order_and_unknown_categories_without_empty_on_error() {
    let rows = json!([
        {"id":"F2","mode":"list","opaque":{"n":1}},
        {"id":"F1","filetype":"pdf","unknown":true},
        {"id":"F2","title":"later duplicate"}]);
    assert_eq!(
        response(&json!({"ok":true,"files":rows})).unwrap(),
        rows.as_array().unwrap().clone()
    );
    assert!(response(&json!({"ok":true,"files":[]})).unwrap().is_empty());
    for raw in [
        json!({"ok":false,"files":[]}),
        json!({"ok":true,"error":"failed","files":[]}),
        json!({"ok":true}),
        json!({"ok":true,"files":{}}),
        json!({"ok":true,"files":[null]}),
        json!({"ok":true,"files":[{"id":""}]}),
    ] {
        assert!(response(&raw).is_err());
    }
}

#[test]
fn exact_cjk_bmp_and_utf16_threshold_match_original_factory_oracle() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let ranges = oracle["ranges"].as_array().unwrap();
    for n in 0..=65535 {
        let Some(c) = char::from_u32(n) else { continue };
        let expected = ranges
            .iter()
            .any(|r| n >= r[0].as_u64().unwrap() as u32 && n <= r[1].as_u64().unwrap() as u32);
        assert_eq!(remote_threshold(&c.to_string()), expected, "U+{n:04X}");
    }
    for row in oracle["thresholds"].as_array().unwrap() {
        assert_eq!(
            remote_threshold(row["query"].as_str().unwrap()),
            row["result"].as_bool().unwrap(),
            "{row}"
        );
    }
}

#[test]
fn original_keuo_class_trace_preserves_boundaries_order_and_exact_tokens() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let mut cache = QueryCache::new(1000);
    let mut tokens = BTreeMap::new();
    for row in oracle["cases"].as_array().unwrap() {
        let now = row["now"].as_i64().unwrap();
        let arg = row["args"][0].as_str().unwrap_or("");
        let result = match row["op"].as_str().unwrap() {
            "add" => {
                cache.add(arg, now);
                Value::Null
            }
            "addAsPrefix" => {
                cache.add_as_prefix(arg, now);
                Value::Null
            }
            "contains" => json!(cache.contains(arg, now)),
            "containsPrefixOf" => json!(cache.contains_prefix_of(arg, now)),
            "setRequestInFlight" => {
                tokens.insert(arg.to_owned(), cache.set_request_in_flight(now));
                Value::Null
            }
            "clearRequestInFlight" => {
                cache.clear_request_in_flight(&tokens[arg]);
                Value::Null
            }
            "isRequestInFlight" => json!(cache.is_request_in_flight(now)),
            "reset" => {
                cache.reset(now);
                Value::Null
            }
            _ => panic!("unknown oracle op"),
        };
        assert_eq!(result, row["result"], "{row}");
    }
}

#[test]
fn raw_small_success_and_adopted_positive_result_are_separate_cache_events() {
    for count in [0, 1, 9, 10, 25] {
        let mut cache = QueryCache::new(1000);
        let token = cache.set_request_in_flight(1000);
        cache.record_response("query", count, Some(&token), false, 1001);
        assert_eq!(cache.contains_prefix_of("query-longer", 1001), count < 10);
        assert_eq!(cache.is_request_in_flight(1001), count >= 10);
        assert!(!cache.contains("query", 1001));
        cache.record_upsert("query", count, false, 1002);
        assert_eq!(cache.contains("query", 1002), count > 0);
    }
}

#[test]
fn aborted_or_failed_reads_do_not_become_empty_success_and_old_reply_cannot_clear_new_token() {
    let mut cache = QueryCache::new(1000);
    let old = cache.set_request_in_flight(1000);
    cache.record_response("aborted", 0, Some(&old), true, 1001);
    cache.record_upsert("aborted", 1, true, 1001);
    assert!(!cache.contains("aborted", 1001));
    assert!(!cache.contains_prefix_of("aborted-more", 1001));
    assert!(cache.is_request_in_flight(1399));
    assert!(!cache.is_request_in_flight(1400));
    let new = cache.set_request_in_flight(1400);
    cache.record_response("old", 1, Some(&old), false, 1401);
    assert!(cache.contains_prefix_of("old-more", 1401));
    assert!(cache.is_request_in_flight(1401));
    cache.record_response("new", 0, Some(&new), false, 1402);
    assert!(!cache.is_request_in_flight(1402));
    // Failed HTTP/malformed responses never call either successful receipt hook.
    assert!(!cache.contains("failed", 1402));
    assert!(!cache.contains_prefix_of("failed-more", 1402));
}
