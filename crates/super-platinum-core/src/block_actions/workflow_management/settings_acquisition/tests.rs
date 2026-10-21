use super::*;
use crate::slack::client::RequestBody;
use std::time::Duration;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: Some("E00000001".into()),
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
fn endpoint(owner: &WorkspaceSession, now: Instant) -> DirectoryEndpoint {
    DirectoryEndpoint::decode(
        owner,
        &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/E00000001"}),
        now,
    )
    .unwrap()
}
#[test]
fn exact_preset_bodies_do_not_borrow_other_search_defaults() {
    let w = owner();
    for (kind, expected) in [
        (
            SearchKind::Members,
            json!({"query":"raw @Ada","count":24,"fuzz":1,"enable_workspace_ranking":true,"filter":"NOT deactivated AND NOT guests","include_profile_only_users":true,"marker":"next"}),
        ),
        (
            SearchKind::Channels,
            json!({"query":"raw @Ada","count":24,"fuzz":1,"include_record_channels":true,"check_membership":true,"marker":"next"}),
        ),
        (
            SearchKind::ExternalTeams,
            json!({"query":"raw @Ada","marker":"next"}),
        ),
    ] {
        assert_eq!(
            Read::new(&w, kind, "raw @Ada", Some("next"))
                .unwrap()
                .body(),
            &expected
        );
    }
    assert_eq!(
        Read::new(&w, SearchKind::AvailableExternal, "", Some("next"))
            .unwrap()
            .body(),
        &json!({"count":100,"filter":"available","marker":"next"})
    );
    assert!(Read::new(&w, SearchKind::AvailableExternal, "query", None).is_err());
}
#[test]
fn internal_search_exact_method_serialized_query_and_cursor() {
    let w = owner();
    let client = SlackClient::default();
    let r = Read::new(&w, SearchKind::InternalTeams, "a\"b", Some("cursor")).unwrap();
    assert_eq!(
        r.body(),
        &json!({"query":r#"{"type":"and","clauses":[{"type":"is","value":"team"},{"type":"fuzzy","value":"a\"b"}]}"#,"count":24,"cursor_mark":"cursor"})
    );
    let req = r.request(&client, &w, None, Instant::now()).unwrap();
    assert!(req.url.contains("/api/search.enterprise"));
    let RequestBody::Form(fields) = req.body else {
        panic!("form")
    };
    assert!(fields.contains(&("_x_reason".into(), "fetchAndUpsertTeamsWithQuery".into())));
    assert_eq!(
        Read::new(&w, SearchKind::InternalTeams, "", None)
            .unwrap()
            .body()["query"],
        r#"{"type":"is","value":"team"}"#
    );
}
#[test]
fn owned_endpoint_and_original_read_are_both_required_without_allowlist_expansion() {
    let w = owner();
    let now = Instant::now();
    let endpoint = endpoint(&w, now);
    let client = SlackClient::default();
    let read = Read::new(&w, SearchKind::ExternalTeams, "", None).unwrap();
    assert!(read.request(&client, &w, None, now).is_err());
    assert!(
        endpoint
            .request(
                &client,
                &w,
                now,
                "externalTeams/search",
                json!({"query":""})
            )
            .is_err()
    );
    let req = read.request(&client, &w, Some(&endpoint), now).unwrap();
    assert!(req.url.contains("/cache/E00000001/externalTeams/search"));
    assert!(
        read.request(&client, &w, Some(&endpoint), now + Duration::from_secs(60))
            .is_err()
    );
    for field in 0..5 {
        let mut next = w.clone();
        match field {
            0 => next.team_id = "T00000002".into(),
            1 => next.user_id = "U00000002".into(),
            2 => next.token = "changed".into(),
            3 => next.url = "https://other.test".into(),
            _ => next.enterprise_id = None,
        }
        assert!(!read.current(&next));
        assert!(read.request(&client, &next, Some(&endpoint), now).is_err());
    }
}
#[test]
fn same_byte_map_order_unknowns_membership_and_marker_survive() {
    let read = Read::new(&owner(), SearchKind::Channels, "", None).unwrap();
    let raw = json!({"ok":true,"results":{"z":[{"id":"C00000002","opaque":[false,null]}],"a":{"id":"C00000001"}},"member_channels":["C00000002"],"next_marker":"cursor","extra":0});
    assert!(read.decode(raw.clone()).is_err());
    let page = read
        .decode_ordered(raw.clone(), Some(&["z".into(), "a".into()]))
        .unwrap();
    assert_eq!(page.raw(), &raw);
    assert_eq!(page.rows()[0]["id"], "C00000002");
    assert_eq!(page.membership_ids(), ["C00000002"]);
    assert_eq!(page.next_marker(), Some("cursor"));
    assert!(page.read().current(&owner()));
    for keys in [
        vec!["a".into()],
        vec!["a".into(), "a".into()],
        vec!["a".into(), "foreign".into()],
    ] {
        assert!(read.decode_ordered(raw.clone(), Some(&keys)).is_err());
    }
}
#[test]
fn team_array_producers_and_exact_original_available_order() {
    let internal = Read::new(&owner(), SearchKind::InternalTeams, "", None).unwrap();
    let page=internal.decode(json!({"ok":true,"items":[{"id":"T00000002"},{"id":"E00000001"}],"next_cursor_mark":"next"})).unwrap();
    assert_eq!(page.next_marker(), Some("next"));
    assert_eq!(page.rows()[0]["id"], "T00000002");
    let available = Read::new(&owner(), SearchKind::AvailableExternal, "", None).unwrap();
    assert!(
        available
            .decode(json!({"ok":true,"results":{"one":{"id":"T00000001"}}}))
            .is_err()
    );
    let raw =
        json!({"ok":true,"results":[{"id":"T00000002"},{"id":"T00000001"},{"id":"T00000002"}]});
    assert_eq!(
        available.decode(raw.clone()).unwrap().rows(),
        raw["results"].as_array().unwrap()
    );
}
#[test]
fn malformed_error_and_repeated_cursor_do_not_become_authority() {
    let read = Read::new(&owner(), SearchKind::Members, "", Some("same")).unwrap();
    for raw in [
        json!({"ok":false,"results":[]}),
        json!({"ok":true,"error":"oops","results":[]}),
        json!({"results":false}),
        json!({"results":[{"id":"T00000001"}]}),
        json!({"results":[],"next_marker":"same"}),
        json!({"results":[],"member_channels":false}),
    ] {
        assert!(read.decode(raw).is_err());
    }
    assert!(Read::new(&owner(), SearchKind::Members, "", Some("\n")).is_err());
    assert!(Read::new(&owner(), SearchKind::Members, "\0", None).is_err());
}
#[test]
fn bounded_team_hydration_has_exact_original_source_body() {
    let ids = vec!["T00000001".into(), "E00000002".into()];
    let read = Read::teams_by_ids(&owner(), &ids, Some("original-caller")).unwrap();
    assert_eq!(read.method(), "team.info");
    assert_eq!(
        read.body(),
        &json!({"teams":ids,"include_unlisted_if_admin":false})
    );
    assert_eq!(
        read.decode(json!({"ok":true,"teams":[{"id":"E00000002","opaque":false}]}))
            .unwrap()
            .rows()[0]["opaque"],
        false
    );
    for ids in [
        vec![],
        vec!["U00000001".into()],
        vec!["T00000001".into(); 251],
        vec!["T00000001".into(); 2],
    ] {
        assert!(Read::teams_by_ids(&owner(), &ids, Some("reason")).is_err());
    }
}

#[test]
fn team_hydration_preserves_omitted_reason_without_inventing_empty_metadata() {
    let w = owner();
    let ids = vec!["T00000001".into()];
    for reason in [None, Some("source-caller")] {
        let read = Read::teams_by_ids(&w, &ids, reason).unwrap();
        let request = read
            .request(&SlackClient::default(), &w, None, Instant::now())
            .unwrap();
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        let observed = fields
            .iter()
            .filter(|(key, _)| key == "_x_reason")
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>();
        assert_eq!(observed, reason.into_iter().collect::<Vec<_>>());
        assert_eq!(
            read.body(),
            &json!({"teams":ids,"include_unlisted_if_admin":false})
        );
    }
    for reason in ["", "invalid\nreason"] {
        assert!(Read::teams_by_ids(&w, &ids, Some(reason)).is_err());
    }
}
