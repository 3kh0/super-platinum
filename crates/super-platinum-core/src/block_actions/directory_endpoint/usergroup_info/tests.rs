use super::*;
use crate::slack::client::RequestBody;

fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000001".into(),
        enterprise_id: Some("E00000001".into()),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn read() -> Read {
    Read::capture(vec!["S00000001".into(), "S00000002".into()]).unwrap()
}
#[test]
fn info_uses_observed_route_exact_body_and_original_owner() {
    let now = Instant::now();
    let owner = owner();
    let endpoint = DirectoryEndpoint::decode(
        &owner,
        &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/E00000001"}),
        now,
    )
    .unwrap();
    let request = endpoint
        .usergroup_info(&SlackClient::default(), &owner, now, &read())
        .unwrap();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/cache/E00000001/usergroups/info"
    );
    assert_eq!(
        request.body,
        RequestBody::Json(json!({"token":"synthetic","ids":["S00000001","S00000002"]}))
    );
    assert!(!request.retry_safe());
    let next = read().next_page("next").unwrap();
    let request = endpoint
        .usergroup_info(&SlackClient::default(), &owner, now, &next)
        .unwrap();
    assert_eq!(
        request.body,
        RequestBody::Json(
            json!({"token":"synthetic","ids":["S00000001","S00000002"],"marker":"next"})
        )
    );
    for (key, value) in [("token", "different"), ("user", "U00000009")] {
        let mut changed = owner.clone();
        if key == "token" {
            changed.token = value.into();
        } else {
            changed.user_id = value.into();
        }
        assert!(
            endpoint
                .usergroup_info(&SlackClient::default(), &changed, now, &read())
                .is_err()
        );
    }
    assert!(
        endpoint
            .usergroup_info(
                &SlackClient::default(),
                &owner,
                now + Duration::from_secs(60),
                &read()
            )
            .is_err()
    );
}
#[test]
fn request_chunks_refuse_duplicate_invalid_and_oversize_ids_or_marker_replay() {
    for ids in [
        vec![],
        vec!["S00000001".into(); 2],
        vec!["U00000001".into()],
        vec!["S1".into()],
        (0..101).map(|n| format!("S{n:08}")).collect(),
    ] {
        assert!(Read::capture(ids).is_err());
    }
    assert!(Read::capture((0..100).map(|n| format!("S{n:08}")).collect()).is_ok());
    for marker in ["", "\n", "a\0b"] {
        assert!(read().next_page(marker).is_err());
    }
    assert!(read().next_page("next").unwrap().next_page("next").is_err());
}
#[test]
fn page_preserves_source_order_duplicate_rows_all_outcomes_and_raw_metadata() {
    let raw = json!({"ok":true,"results":{"z":[{"id":"S00000002","future":1},{"id":"S00000001","version":1}],"a":{"id":"S00000001","version":2}},
        "failed_ids":["S00000001"],"pending_ids":["S00000002"],"invalid_ids":["S00000001"],"next_marker":"next","unknown":{"keep":true}});
    let read = read();
    let page = Page::decode(&read, raw.clone(), Some(vec!["z".into(), "a".into()])).unwrap();
    assert_eq!(page.raw(), &raw);
    assert!(page.request() == &read);
    assert_eq!(
        page.rows()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["S00000002", "S00000001", "S00000001"]
    );
    assert_eq!(page.rows()[2]["version"], 2);
    assert_eq!(page.failed_ids(), ["S00000001"]);
    assert_eq!(page.pending_ids(), ["S00000002"]);
    assert_eq!(page.invalid_ids(), ["S00000001"]);
    assert_eq!(page.next_marker(), Some("next"));
    assert!(Page::decode(&read, raw.clone(), None).is_err());
    assert!(Page::decode(&read, raw, Some(vec!["a".into(), "a".into()])).is_err());
}
#[test]
fn empty_and_pending_do_not_synthesize_rows_and_malformed_or_foreign_pages_refuse() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"results":[]}),
        json!({"ok":true,"pending_ids":["S00000001"]}),
    ] {
        assert!(Page::decode(&read(), raw, None).unwrap().rows().is_empty());
    }
    for raw in [
        json!({"results":[]}),
        json!({"ok":true,"error":"bad"}),
        json!({"ok":true,"results":null}),
        json!({"ok":true,"results":[{"id":"S00000009"}]}),
        json!({"ok":true,"failed_ids":["S00000009"]}),
        json!({"ok":true,"pending_ids":false}),
        json!({"ok":true,"invalid_ids":[3]}),
        json!({"ok":true,"next_marker":42}),
    ] {
        assert!(Page::decode(&read(), raw, None).is_err());
    }
    assert!(Page::decode(&read(), json!({"ok":true,"results":[]}), Some(vec![])).is_err());
}
#[test]
fn fallback_exact_disabled_inclusive_options_and_raw_rows() {
    let fallback = Fallback::capture(
        vec!["S00000001".into(), "S00000002".into()],
        "fetchRawUserGroupsById",
    )
    .unwrap();
    let req = fallback.request(&SlackClient::default(), &owner()).unwrap();
    assert_eq!(
        url::Url::parse(&req.url).unwrap().path(),
        "/api/subteams.info"
    );
    assert!(!req.retry_safe());
    let RequestBody::Form(fields) = req.body else {
        panic!("form");
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            ("include_users".into(), "false".into()),
            ("include_count".into(), "true".into()),
            ("include_disabled".into(), "true".into()),
            ("usergroups".into(), "S00000001,S00000002".into()),
            ("_x_reason".into(), "fallback:fetchRawUserGroupsById".into())
        ]
    );
    assert!(valid_fallback_fields(&fields));
    let raw = json!({"ok":true,"subteams":[{"id":"S00000002","date_delete":123,"future":1},{"id":"S00000001"}],"future":{"keep":true}});
    let page = FallbackPage::decode(&fallback, raw.clone()).unwrap();
    assert_eq!(page.raw(), &raw);
    assert!(page.request() == &fallback);
    assert_eq!(page.rows()[0]["date_delete"], 123);
    assert!(
        FallbackPage::decode(&fallback, json!({"ok":true,"subteams":[]}))
            .unwrap()
            .rows()
            .is_empty()
    );
    for raw in [
        json!({"ok":true}),
        json!({"ok":false,"subteams":[]}),
        json!({"ok":true,"subteams":[{"id":"S00000009"}]}),
    ] {
        assert!(FallbackPage::decode(&fallback, raw).is_err());
    }
    assert!(Fallback::capture((0..250).map(|n| format!("S{n:08}")).collect(), "reason").is_ok());
    assert!(Fallback::capture((0..251).map(|n| format!("S{n:08}")).collect(), "reason").is_err());
}
