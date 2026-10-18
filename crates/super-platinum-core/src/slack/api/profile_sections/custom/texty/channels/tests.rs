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
    assert!(request.url.contains("/channels/search"));
    let RequestBody::Json(mut body) = request.body else {
        panic!("expected JSON")
    };
    body.as_object_mut().unwrap().remove("token");
    body
}
fn page(raw: Value) -> Page {
    Page::decode_ordered(raw, None).unwrap()
}
fn rows(start: usize, n: usize) -> Vec<Value> {
    (start..start + n)
        .map(|i| json!({"id":format!("C{i:08}"),"unknown":{"ordinal":i}}))
        .collect()
}
#[test]
fn exact_am1o_53by_body_preserves_query_and_does_not_invent_local_filter() {
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    for raw in ["", " A & B ", "#channel", "\u{4e2d}\u{1f642}"] {
        let q = Query::capture(&w, raw, RequestFacts::default()).unwrap();
        assert_eq!(q.raw(), raw);
        assert_eq!(
            body(
                q.request(&SlackClient::default(), &w, &endpoint, now, None)
                    .unwrap()
            ),
            json!({"query":raw,"count":25,"fuzz":1,"filter":"xws",
                "include_record_channels":true,"check_membership":true})
        );
    }
}
#[test]
fn observed_priority_and_workspace_hints_and_marker_are_forwarded_exactly() {
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    let top = json!(["C00000002", "C00000001", "C00000002"]);
    let q = Query::capture(
        &w,
        "ship",
        RequestFacts {
            top_channels: Some(top.clone()),
            default_workspace: Some("T00000002".into()),
        },
    )
    .unwrap();
    let b = body(
        q.request(&SlackClient::default(), &w, &endpoint, now, Some("cursor"))
            .unwrap(),
    );
    assert_eq!(b["top_channels"], top);
    assert_eq!(b["default_workspace"], "T00000002");
    assert_eq!(b["marker"], "cursor");
    assert_eq!(b.as_object().unwrap().len(), 9);
    for value in [Value::Null, json!(false), json!(0), json!("")] {
        let q = Query::capture(
            &w,
            "x",
            RequestFacts {
                top_channels: Some(value),
                default_workspace: Some(String::new()),
            },
        )
        .unwrap();
        let b = body(
            q.request(&SlackClient::default(), &w, &endpoint, now, None)
                .unwrap(),
        );
        assert!(b.get("top_channels").is_none() && b.get("default_workspace").is_none());
    }
    let q = Query::capture(
        &w,
        "x",
        RequestFacts {
            top_channels: Some(json!([])),
            default_workspace: None,
        },
    )
    .unwrap();
    assert_eq!(
        body(
            q.request(&SlackClient::default(), &w, &endpoint, now, None)
                .unwrap()
        )["top_channels"],
        json!([])
    );
}
#[test]
fn owner_replacement_endpoint_expiry_and_invalid_marker_refuse_request() {
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    let q = Query::capture(&w, "x", RequestFacts::default()).unwrap();
    let mut changed = w.clone();
    changed.token = "replacement".into();
    assert!(!q.current(&changed));
    assert!(
        q.request(&SlackClient::default(), &changed, &endpoint, now, None)
            .is_err()
    );
    assert!(
        q.request(
            &SlackClient::default(),
            &w,
            &endpoint,
            now + Duration::from_secs(58),
            None
        )
        .is_err()
    );
    for marker in ["", "bad\nmarker"] {
        assert!(
            q.request(&SlackClient::default(), &w, &endpoint, now, Some(marker))
                .is_err()
        );
    }
}
#[test]
fn same_byte_object_order_flattening_duplicates_and_auxiliary_rows_are_retained() {
    let raw = json!({"results":{"a":{"id":"C1"},"z":[{"id":"C2","unknown":7},{"id":"C2","unknown":8}]},
        "mpims":[{"id":"G3"}],"huddles":[{"id":"R4"}],"opaque":{"kept":true}});
    let keys = vec!["z".into(), "a".into()];
    let p = Page::decode_ordered(raw.clone(), Some(&keys)).unwrap();
    assert_eq!(p.raw(), &raw);
    assert_eq!(
        p.rows(),
        &[
            json!({"id":"C2","unknown":7}),
            json!({"id":"C2","unknown":8}),
            json!({"id":"C1"}),
            json!({"id":"G3"}),
            json!({"id":"R4"})
        ]
    );
    assert!(Page::decode_ordered(raw.clone(), None).is_err());
    assert!(Page::decode_ordered(raw, Some(&["z".into(), "z".into()])).is_err());
}
#[test]
fn partial_pages_keep_all_outcomes_and_last_interaction_value() {
    let mut p = Pages::default();
    p.append(page(json!({"results":[{"id":"C1"}],"next_marker":"next",
        "failed_ids":["C2"],"pending_ids":["C3"],"invalid_ids":["C4"],
        "member_channels":["C1"],"disconnected_ids":["C5"],"can_interact":{"C1":false}})))
        .unwrap();
    assert!(!p.complete());
    assert!(p.rows().is_err());
    assert!(p.outcomes().is_err());
    p.append(page(
        json!({"results":[{"id":"C1","updated":7}],"failed_ids":["C2"],
        "member_channels":["C1"],"can_interact":{"C1":true,"C6":false}}),
    ))
    .unwrap();
    assert_eq!(p.rows().unwrap().len(), 2);
    let o = p.outcomes().unwrap();
    assert_eq!(o.failed_ids, ["C2", "C2"]);
    assert_eq!(o.pending_ids, ["C3"]);
    assert_eq!(o.invalid_ids, ["C4"]);
    assert_eq!(o.membership_ids, ["C1", "C1"]);
    assert_eq!(o.disconnected_ids, ["C5"]);
    assert_eq!(o.can_interact.get("C1"), Some(&json!(true)));
    assert_eq!(o.can_interact.get("C6"), Some(&json!(false)));
}
#[test]
fn count_boundary_returns_first_overflow_id_and_retains_raw_tail_outcomes() {
    let mut p = Pages::default();
    p.append(page(json!({"results":rows(1,9),"next_marker":"first"})))
        .unwrap();
    assert_eq!(p.marker(), Some("first"));
    p.append(page(
        json!({"results":rows(10,20),"next_marker":"server-marker","pending_ids":["C99"]}),
    ))
    .unwrap();
    assert_eq!(p.rows().unwrap(), rows(1, 25));
    assert_eq!(p.marker(), Some("C00000026"));
    assert_eq!(p.pages()[1].rows().len(), 20);
    assert_eq!(p.outcomes().unwrap().pending_ids, ["C99"]);
    let mut exact = Pages::default();
    exact
        .append(page(
            json!({"results":rows(1,25),"next_marker":"server-marker"}),
        ))
        .unwrap();
    assert!(exact.complete());
    assert_eq!(exact.marker(), Some("server-marker"));
}
#[test]
fn empty_terminal_response_is_not_failed_or_pending_synthesis() {
    for raw in [
        json!({}),
        json!({"ok":true,"results":[],"next_marker":""}),
        json!({"results":{},"next_marker":null}),
    ] {
        let keys = raw["results"].is_object().then(Vec::<String>::new);
        let mut p = Pages::default();
        p.append(Page::decode_ordered(raw, keys.as_deref()).unwrap())
            .unwrap();
        assert!(p.rows().unwrap().is_empty());
        assert_eq!(p.marker(), None);
        assert!(p.outcomes().unwrap().pending_ids.is_empty());
    }
}
#[test]
fn repeated_marker_poison_prevents_partial_or_retried_success() {
    let mut p = Pages::default();
    p.append(page(json!({"results":rows(1,1),"next_marker":"same"})))
        .unwrap();
    assert!(
        p.append(page(json!({"results":[],"next_marker":"same"})))
            .is_err()
    );
    assert!(p.rows().is_err());
    assert!(p.outcomes().is_err());
    assert!(p.append(page(json!({"results":rows(2,1)}))).is_err());
    assert!(!p.complete());
}
#[test]
fn malformed_rows_outcomes_and_semantic_errors_are_not_empty_success() {
    for raw in [
        json!(null),
        json!({"ok":false}),
        json!({"ok":true,"error":"no"}),
        json!({"results":null}),
        json!({"results":[{}]}),
        json!({"results":[{"id":7}]}),
        json!({"next_marker":7}),
        json!({"failed_ids":false}),
        json!({"pending_ids":[false]}),
        json!({"can_interact":[]}),
        json!({"huddles":false}),
    ] {
        assert!(Page::decode_ordered(raw, None).is_err());
    }
    assert!(Page::decode_ordered(json!({"results":[]}), Some(&[])).is_err());
}
