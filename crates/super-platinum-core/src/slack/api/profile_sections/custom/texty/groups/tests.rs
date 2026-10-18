use super::*;
use crate::block_actions::home::workflow::directory_local::PreparedQuery;
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
    DirectoryEndpoint::decode(owner,&json!({"ok":true,"ttl_seconds":60,"url":format!("https://edgeapi.slack.com/cache/{}",owner.team_id)}),now).unwrap()
}
fn snap(raw: Value) -> Snapshot {
    Snapshot::capture(raw.clone(), raw).unwrap()
}
fn self_user() -> User {
    serde_json::from_value(json!({"id":"U00000001"})).unwrap()
}
#[test]
fn original_m0ek_request_keeps_raw_query_and_exact_profile_options() {
    let oracle: Value = serde_json::from_str(include_str!("request-oracle.json")).unwrap();
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    for case in oracle.as_array().unwrap() {
        let q = Query::capture(&w, case["query"].as_str().unwrap(), true).unwrap();
        let req = q
            .request(&SlackClient::default(), &w, &endpoint, now, None)
            .unwrap();
        assert!(req.url.contains("/usergroups/search"));
        assert_eq!(req.method, "POST");
        let RequestBody::Json(mut body) = req.body else {
            panic!("JSON request")
        };
        assert_eq!(
            body.as_object_mut().unwrap().remove("token"),
            Some(json!("synthetic"))
        );
        assert_eq!(body, case["expected"]);
    }
    let q = Query::capture(&w, " raw ", false).unwrap();
    let RequestBody::Json(body) = q
        .request(&SlackClient::default(), &w, &endpoint, now, Some("cursor"))
        .unwrap()
        .body
    else {
        panic!()
    };
    assert_eq!(
        body,
        json!({"query":" raw ","count":25,"org_wide":false,"marker":"cursor","token":"synthetic"})
    );
}
#[test]
fn request_full_owner_expiry_and_marker_fences_apply_to_groups_too() {
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    let query = Query::capture(&w, "", true).unwrap();
    for changed in ["team", "enterprise", "user", "token", "url", "name"] {
        let mut other = w.clone();
        match changed {
            "team" => other.team_id = "TOTHER001".into(),
            "enterprise" => other.enterprise_id = Some("E00000001".into()),
            "user" => other.user_id = "UOTHER001".into(),
            "token" => other.token = "other".into(),
            "url" => other.url = "https://other.test".into(),
            _ => other.name = "other".into(),
        }
        assert!(!query.current(&other));
        assert!(
            query
                .request(&SlackClient::default(), &other, &endpoint, now, None)
                .is_err()
        );
    }
    assert!(
        query
            .request(
                &SlackClient::default(),
                &w,
                &endpoint,
                now + Duration::from_secs(60),
                None
            )
            .is_err()
    );
    for m in ["", "bad\nmarker"] {
        assert!(
            query
                .request(&SlackClient::default(), &w, &endpoint, now, Some(m))
                .is_err()
        );
    }
    let mut enterprise = w;
    enterprise.team_id = "E00000001".into();
    enterprise.enterprise_id = Some(enterprise.team_id.clone());
    let ep = super::tests::endpoint(&enterprise, now);
    let q = Query::capture(&enterprise, "a", true).unwrap();
    let RequestBody::Json(body) = q
        .request(&SlackClient::default(), &enterprise, &ep, now, None)
        .unwrap()
        .body
    else {
        panic!()
    };
    assert_eq!(body["enterprise_token"], "synthetic");
}
#[test]
fn category_gate_and_actual_group_filter_do_not_invent_visibility_restrictions() {
    let oracle: Value = serde_json::from_str(include_str!("matching-oracle.json")).unwrap();
    for case in oracle["eligibility"].as_array().unwrap() {
        let policy = Policy::capture(&self_user(), case["unified"].as_bool().unwrap()).unwrap();
        assert_eq!(
            policy
                .allows(
                    &snap(case["row"].clone()),
                    Some(case["foreign"].as_bool().unwrap())
                )
                .unwrap(),
            case["allowed"].as_bool().unwrap()
        );
    }
    let row = snap(json!({"id":"S00000001","is_visible":false,"auto_type":"admin"}));
    assert!(
        Policy::capture(&self_user(), true)
            .unwrap()
            .allows(&row, None)
            .unwrap()
    );
    assert!(
        Policy::capture(&self_user(), false)
            .unwrap()
            .allows(&row, None)
            .is_err()
    );
    let mut user = self_user();
    user.is_ultra_restricted = true;
    assert!(Policy::capture(&user, true).unwrap().enabled());
    user.is_restricted = true;
    let p = Policy::capture(&user, true).unwrap();
    assert!(!p.enabled());
    assert!(!p.allows(&row, None).unwrap());
    for flag in ["isUnknown", "isNonExistent"] {
        assert!(
            !Policy::capture(&self_user(), true)
                .unwrap()
                .allows(&snap(json!({"id":"S00000001",(flag):true})), None)
                .unwrap()
        );
    }
}
#[test]
fn original_2ymw_matching_preserves_scores_exactness_and_duplicate_occurrences() {
    let oracle: Value = serde_json::from_str(include_str!("matching-oracle.json")).unwrap();
    let policy = Policy::capture(&self_user(), true).unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let rows: Vec<_> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .cloned()
            .map(snap)
            .collect();
        let results = matches(
            &PreparedQuery {
                normalized: case["normalized"].as_str().unwrap().into(),
                matcher_normalized: case["matcher"].as_str().unwrap().into(),
            },
            &rows,
            |r| policy.allows(r, None),
            |r| {
                Names::observe(
                    r,
                    |v| {
                        oracle["lowercase"][v]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(unavailable)
                    },
                    |v| {
                        oracle["normalized"][v]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(unavailable)
                    },
                )
            },
        )
        .unwrap();
        let projected: Vec<_> = results
            .iter()
            .map(|r| json!({"id":r.id,"score":r.score,"exact":r.is_exact_match}))
            .collect();
        assert_eq!(json!(projected), case["expected"], "{}", case["query"]);
        for r in results {
            assert!(
                rows.iter()
                    .any(|original| std::ptr::eq(original, r.snapshot))
            );
        }
    }
}
#[test]
fn explicit_effective_record_retains_distinct_source_and_missing_locale_refuses() {
    let source = json!({"id":"S00000001","date_update":3});
    let effective = json!({"id":"S00000001","date_update":3,"name":"Prior Name","handle":"@@crew"});
    let row = Snapshot::capture(source.clone(), effective.clone()).unwrap();
    assert_eq!(row.source(), &source);
    assert_eq!(row.effective(), &effective);
    assert!(Names::observe(&row, |_| Err(unavailable()), |v| Ok(v.into())).is_err());
    let stored = Snapshot::capture(
        source,
        json!({"id":"S00000001","name":"Prior Name","_name_lc":"captured alias","handle":"@@crew"}),
    )
    .unwrap();
    assert!(
        Names::observe(
            &stored,
            |_| panic!("stored alias avoids locale fallback"),
            |v| Ok(v.into())
        )
        .is_ok()
    );
    assert!(Snapshot::capture(json!({"id":"S00000001"}), json!({"id":"S00000002"})).is_err());
    assert!(
        Snapshot::capture(
            json!({"id":"S00000001"}),
            json!({"id":"S00000001","handle":true})
        )
        .is_err()
    );
}
#[test]
fn empty_query_does_not_require_unused_name_observations_and_no_remote_passthrough() {
    let rows = vec![snap(
        json!({"id":"S00000001","name":"Different","handle":"other"}),
    )];
    let empty = PreparedQuery {
        normalized: String::new(),
        matcher_normalized: String::new(),
    };
    let got = matches(
        &empty,
        &rows,
        |_| Ok(true),
        |_| panic!("empty query has no name access"),
    )
    .unwrap();
    assert_eq!(got[0].score, 0);
    assert_eq!(got[0].is_exact_match, None);
    let q = PreparedQuery {
        normalized: "zzzzzz".into(),
        matcher_normalized: "zzzzzz".into(),
    };
    assert!(
        matches(
            &q,
            &rows,
            |_| Ok(true),
            |r| Names::observe(r, |v| Ok(v.to_lowercase()), |v| Ok(v.to_lowercase()))
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn neutral_pages_retain_raw_order_outcomes_and_truncate_with_overflow_id() {
    let mut pages = Pages::default();
    pages.accept(json!({"ok":true,"results":{"z":[{"id":"S00000001","opaque":1},{"id":"S00000001","opaque":2}],"a":{"id":"S00000002"}},"failed_ids":["SFAIL0001"],"pending_ids":["SPEND0001"],"invalid_ids":["SBAD00001"],"next_marker":"next"}),Some(&["z".into(),"a".into()])).unwrap();
    assert!(!pages.complete());
    assert_eq!(pages.marker(), Some("next"));
    let rows: Vec<_> = (3..29)
        .map(|i| json!({"id":format!("S{i:08}"),"unknown":{"keep":true}}))
        .collect();
    pages
        .accept(
            json!({"ok":true,"results":rows,"next_marker":"server"}),
            None,
        )
        .unwrap();
    assert!(pages.complete());
    assert_eq!(pages.rows().unwrap().len(), 25);
    assert_eq!(pages.marker(), Some("S00000025"));
    assert_eq!(pages.rows().unwrap()[1]["opaque"], 2);
    assert_eq!(pages.outcomes().unwrap().failed_ids, ["SFAIL0001"]);
    assert_eq!(pages.outcomes().unwrap().pending_ids, ["SPEND0001"]);
}
#[test]
fn malformed_or_repeated_marker_permanently_refuses_partial_group_result() {
    for raw in [
        json!({"ok":false}),
        json!({"results":null}),
        json!({"results":{"S00000001":{"id":"S00000001"}}}),
        json!({"results":[],"next_marker":"next"}),
    ] {
        let mut pages = Pages::default();
        pages
            .accept(
                json!({"results":[{"id":"S00000001"}],"next_marker":"next"}),
                None,
            )
            .unwrap();
        assert!(pages.accept(raw, None).is_err());
        assert!(!pages.complete());
        assert!(pages.rows().is_err());
        assert!(pages.accept(json!({"results":[]}), None).is_err());
    }
}
