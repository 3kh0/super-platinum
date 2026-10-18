use super::*;
use crate::slack::client::RequestBody;
use std::time::Duration;

fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
fn endpoint(owner: &WorkspaceSession, now: Instant) -> DirectoryEndpoint {
    DirectoryEndpoint::decode(
        owner,
        &json!({"ok":true,"ttl_seconds":60,
        "url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id)}),
        now,
    )
    .unwrap()
}
fn body(request: PreparedRequest) -> Value {
    assert!(request.url.contains("/users/search"));
    let RequestBody::Json(mut body) = request.body else {
        panic!("expected JSON")
    };
    assert_eq!(
        body.as_object_mut().unwrap().remove("token"),
        Some(json!("synthetic"))
    );
    body
}
fn rows(start: usize, count: usize) -> Vec<Value> {
    (start..start + count)
        .map(|i| json!({"id":format!("U{i:08}"),"opaque":{"ordinal":i}}))
        .collect()
}

#[test]
fn wngx_profile_request_matches_retained_five_case_oracle_without_activity_options() {
    // tmp/profile-rich-ui-prep/test-member-request.mjs exercises actual WNgx
    // with aD6p/fRHe Profile options. The first five inputs are that oracle.
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    for raw in [
        "A",
        " A & B ",
        "name@example.test",
        "一",
        "🚀",
        "",
        "@raw\0",
    ] {
        let query = Query::capture(&w, raw, RequestFacts::default()).unwrap();
        assert_eq!(query.raw(), raw);
        assert_eq!(
            body(
                query
                    .request(&SlackClient::default(), &w, &endpoint, now, None)
                    .unwrap()
            ),
            json!({"query":raw,"count":25,"fuzz":1,"enable_workspace_ranking":true})
        );
    }
}

#[test]
fn actual_optional_hints_preserve_order_duplicates_and_javascript_truthiness() {
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    for top in [json!(["U2", "U1", "U2"]), json!([]), json!({"U1":7})] {
        let query = Query::capture(
            &w,
            "x",
            RequestFacts {
                top_users: Some(top.clone()),
                default_workspace: Some("T00000002".into()),
            },
        )
        .unwrap();
        assert_eq!(
            body(
                query
                    .request(&SlackClient::default(), &w, &endpoint, now, Some("next"))
                    .unwrap()
            ),
            json!({"query":"x","count":25,"fuzz":1,"enable_workspace_ranking":true,
                "top_users":top,"default_workspace":"T00000002","marker":"next"})
        );
    }
    for top in [Value::Null, json!(false), json!(0), json!("")] {
        let query = Query::capture(
            &w,
            "x",
            RequestFacts {
                top_users: Some(top),
                default_workspace: Some(String::new()),
            },
        )
        .unwrap();
        let body = body(
            query
                .request(&SlackClient::default(), &w, &endpoint, now, None)
                .unwrap(),
        );
        assert_eq!(body.as_object().unwrap().len(), 4);
    }
}

#[test]
fn all_session_replacements_and_endpoint_expiry_or_bad_marker_refuse() {
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    let query = Query::capture(&w, "x", RequestFacts::default()).unwrap();
    let mut replacements = vec![w.clone(); 6];
    replacements[0].team_id = "T00000002".into();
    replacements[1].user_id = "U00000002".into();
    replacements[2].enterprise_id = Some("E00000001".into());
    replacements[3].token = "replacement".into();
    replacements[4].url = "https://other.test".into();
    replacements[5].name = "Other".into();
    for replacement in replacements {
        assert!(!query.current(&replacement));
        assert!(
            query
                .request(&SlackClient::default(), &replacement, &endpoint, now, None)
                .is_err()
        );
    }
    assert!(
        query
            .request(
                &SlackClient::default(),
                &w,
                &endpoint,
                now + Duration::from_secs(58),
                None
            )
            .is_err()
    );
    for marker in ["", "bad\nmarker", &"m".repeat(4097)] {
        assert!(
            query
                .request(&SlackClient::default(), &w, &endpoint, now, Some(marker))
                .is_err()
        );
    }
}

#[test]
fn same_byte_order_and_opaque_rows_survive_without_eligibility_or_deduplication() {
    let raw = json!({"ok":true,"results":{
        "a":{"id":"U1","deleted":true},
        "z":[{"id":"U2","is_bot":true,"opaque":1},{"id":"U2","opaque":2}]},
        "mpims":[{"id":"G3"}],"huddles":[{"id":"R4"}],"unknown":{"kept":true}});
    let mut pages = Pages::default();
    pages
        .accept(raw.clone(), Some(&["z".into(), "a".into()]))
        .unwrap();
    assert_eq!(pages.pages()[0].raw(), &raw);
    assert_eq!(
        pages.rows().unwrap(),
        &[
            json!({"id":"U2","is_bot":true,"opaque":1}),
            json!({"id":"U2","opaque":2}),
            json!({"id":"U1","deleted":true}),
            json!({"id":"G3"}),
            json!({"id":"R4"}),
        ]
    );
}

#[test]
fn source_concatenation_truncates_at_25_with_first_omitted_id_and_retained_outcomes() {
    let mut pages = Pages::default();
    pages
        .accept(
            json!({"results":rows(1,9),"next_marker":"next","failed_ids":["U9"],
        "can_interact":{"U1":false}}),
            None,
        )
        .unwrap();
    assert!(!pages.complete());
    assert!(pages.rows().is_err());
    assert!(pages.outcomes().is_err());
    let last = json!({"results":rows(10,20),"next_marker":"server","failed_ids":["U9"],
        "pending_ids":["U90"],"invalid_ids":["U91"],"disconnected_ids":["U92"],
        "member_channels":["C1"],"can_interact":{"U1":true}});
    pages.accept(last.clone(), None).unwrap();
    assert_eq!(pages.rows().unwrap(), rows(1, 25));
    assert_eq!(pages.marker(), Some("U00000026"));
    assert_eq!(pages.pages()[1].raw(), &last);
    let outcomes = pages.outcomes().unwrap();
    assert_eq!(outcomes.failed_ids, ["U9", "U9"]);
    assert_eq!(outcomes.pending_ids, ["U90"]);
    assert_eq!(outcomes.invalid_ids, ["U91"]);
    assert_eq!(outcomes.disconnected_ids, ["U92"]);
    assert_eq!(outcomes.membership_ids, ["C1"]);
    assert_eq!(outcomes.can_interact.get("U1"), Some(&json!(true)));
}

#[test]
fn malformed_next_page_permanently_poisons_partial_accumulation() {
    for malformed in [
        json!(null),
        json!({"ok":false}),
        json!({"error":"bad"}),
        json!({"results":null}),
        json!({"results":[{}]}),
        json!({"results":[{"id":7}]}),
        json!({"results":{"a":{"id":"U1"}}}),
        json!({"pending_ids":false}),
        json!({"can_interact":[]}),
        json!({"next_marker":7}),
    ] {
        let mut pages = Pages::default();
        pages
            .accept(json!({"results":rows(1,1),"next_marker":"next"}), None)
            .unwrap();
        assert!(pages.accept(malformed, None).is_err());
        assert!(!pages.complete());
        assert!(pages.rows().is_err() && pages.outcomes().is_err());
        assert_eq!(pages.marker(), None);
        assert!(pages.accept(json!({"results":rows(2,1)}), None).is_err());
        assert_eq!(pages.pages().len(), 1);
    }
}

#[test]
fn cyclic_or_exhausted_pages_cannot_be_relabelled_complete() {
    let mut cycle = Pages::default();
    cycle
        .accept(json!({"results":[],"next_marker":"same"}), None)
        .unwrap();
    assert!(
        cycle
            .accept(json!({"results":[],"next_marker":"same"}), None)
            .is_err()
    );
    let mut exhausted = Pages::default();
    for i in 0..127 {
        exhausted
            .accept(json!({"results":[],"next_marker":format!("next{i}")}), None)
            .unwrap();
    }
    assert!(
        exhausted
            .accept(json!({"results":[],"next_marker":"too-many"}), None)
            .is_err()
    );
    for mut pages in [cycle, exhausted] {
        assert!(pages.accept(json!({"results":[]}), None).is_err());
        assert!(!pages.complete());
        assert!(pages.rows().is_err());
    }
}

#[test]
fn terminal_empty_response_and_exact_limit_keep_source_markers_without_synthetic_absence() {
    let mut empty = Pages::default();
    empty
        .accept(
            json!({"ok":true,"results":{},"next_marker":null}),
            Some(&[]),
        )
        .unwrap();
    assert!(empty.complete());
    assert!(empty.rows().unwrap().is_empty());
    assert!(empty.outcomes().unwrap().failed_ids.is_empty());
    assert!(empty.outcomes().unwrap().pending_ids.is_empty());
    let mut full = Pages::default();
    full.accept(json!({"results":rows(1,25),"next_marker":"server"}), None)
        .unwrap();
    assert!(full.complete());
    assert_eq!(full.marker(), Some("server"));
    assert!(full.accept(json!({"results":[]}), None).is_err());
    assert!(full.rows().is_err());
}
