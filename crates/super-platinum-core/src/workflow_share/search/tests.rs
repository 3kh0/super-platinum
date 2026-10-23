use super::*;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Fixture".into(),
        url: "https://fixture.test".into(),
        token: "synthetic".into(),
    }
}
fn rows(start: usize, n: usize) -> Value {
    json!(
        (start..start + n)
            .map(|n| json!({"id":format!("U{n}")}))
            .collect::<Vec<_>>()
    )
}
#[test]
fn share_search_query_routing_preserves_nonprefix_bytes() {
    for (raw, wire, kinds) in [
        ("\u{ff20}Ada", "Ada", vec![Kind::Members]),
        ("\u{ff03}ship", "ship", vec![Kind::Channels]),
        ("/Ada", "Ada", vec![Kind::Members, Kind::Channels]),
        (
            "\u{ff0b}Ada",
            "\u{ff0b}Ada",
            vec![Kind::Members, Kind::Channels],
        ),
        (":Ada", ":Ada", vec![Kind::Members, Kind::Channels]),
        (" @Ada ", " @Ada ", vec![Kind::Members, Kind::Channels]),
        ("", "", vec![Kind::Members, Kind::Channels]),
    ] {
        let q = Query::new(raw).unwrap();
        assert_eq!(q.raw(), raw);
        assert_eq!(q.wire(), wire);
        assert_eq!(q.kinds(), kinds);
    }
}
#[test]
fn share_search_exact_first_email_precedes_prefix_routing() {
    for (raw, expected) in [
        (
            "# label <Ada+tag@EXAMPLE.COM> later@b.test",
            "Ada+tag@EXAMPLE.COM",
        ),
        (
            "\u{4f60}\u{597d} (a.b@foo-bar.example)",
            "a.b@foo-bar.example",
        ),
        ("a..b@example.test", "b@example.test"),
        ("a@localhost", "a@localhost"),
        ("@a@example.test", "a@example.test"),
    ] {
        let q = Query::new(raw).unwrap();
        assert_eq!(q.parsed(), expected);
        if expected.contains('.') {
            assert_eq!(q.kinds(), [Kind::Members, Kind::Channels]);
        }
    }
}
#[test]
fn share_search_wire_is_exact24_and_original_owner() {
    let w = owner();
    let q = Query::new("Ada").unwrap();
    let now = Instant::now();
    let endpoint = DirectoryEndpoint::decode(
        &w,
        &json!({"ok":true,"ttl_seconds":60,"url":"https://edgeapi.slack.com/cache/T1"}),
        now,
    )
    .unwrap();
    for kind in [Kind::Members, Kind::Channels] {
        let r = Read::new(&w, kind, &q, Some("next")).unwrap();
        let request = r
            .request(&SlackClient::default(), &w, &endpoint, now)
            .unwrap();
        let crate::slack::client::RequestBody::Json(body) = request.body else {
            panic!("JSON")
        };
        assert_eq!(body["count"], 24);
        assert_eq!(body["marker"], "next");
        assert_eq!(body["query"], "Ada");
        assert_eq!(
            body["filter"],
            if kind == Kind::Members {
                "NOT deactivated"
            } else {
                "xws"
            }
        );
        assert!(body.get("_x_reason").is_none());
        let mut changed = w.clone();
        changed.token = "replacement".into();
        assert!(
            r.request(&SlackClient::default(), &changed, &endpoint, now)
                .is_err()
        );
    }
    assert!(Read::new(&w, Kind::Channels, &Query::new("@Ada").unwrap(), None).is_err());
    assert!(Read::new(&w, Kind::Members, &q, Some("")).is_err());
}
#[test]
fn share_search24_stops_but25_uses_first_excluded_id() {
    for n in [24, 25, 30] {
        let mut p = Pages::default();
        p.accept(json!({"results":rows(0,n),"next_marker":"server"}), None)
            .unwrap();
        assert!(p.complete());
        assert_eq!(p.rows().unwrap().len(), 24);
        assert_eq!(p.marker(), Some(if n > 24 { "U24" } else { "server" }));
        assert_eq!(p.pages()[0].rows().len(), n);
    }
}
#[test]
fn share_search_partial_pages_order_and_companions() {
    let mut p = Pages::default();
    p.accept(json!({"results":rows(0,23),"next_marker":"next","failed_ids":["F"],"can_interact":{"U0":false}}),None).unwrap();
    assert!(!p.complete());
    assert!(p.rows().is_err());
    assert_eq!(p.marker(), Some("next"));
    p.accept(json!({"results":{"z":[{"id":"Z"},{"id":"OVER"}],"a":{"id":"A"}},"member_channels":["C"],"pending_ids":["P"],"invalid_ids":["I"],"disconnected_ids":["D"],"can_interact":{"U0":true}}),Some(&["z".into(),"a".into()])).unwrap();
    assert_eq!(p.rows().unwrap()[23]["id"], "Z");
    assert_eq!(p.marker(), Some("OVER"));
    let o = p.outcomes().unwrap();
    assert_eq!(o.failed_ids, ["F"]);
    assert_eq!(o.membership_ids, ["C"]);
    assert_eq!(o.pending_ids, ["P"]);
    assert_eq!(o.invalid_ids, ["I"]);
    assert_eq!(o.disconnected_ids, ["D"]);
    assert_eq!(o.can_interact["U0"], true);
}
#[test]
fn share_search_failed_later_page_never_exposes_partial_or_relabels_success() {
    for transport in [false, true] {
        let mut p = Pages::default();
        p.accept(json!({"results":rows(0,2),"next_marker":"next"}), None)
            .unwrap();
        if transport {
            p.fail();
        } else {
            assert!(
                p.accept(json!({"ok":false,"error":"offline"}), None)
                    .is_err()
            );
        }
        assert!(p.accept(json!({"results":rows(2,1)}), None).is_err());
        assert!(!p.complete());
        assert!(p.rows().is_err());
        assert!(p.outcomes().is_err());
        assert_eq!(p.pages().len(), 1);
        assert_eq!(p.marker(), None);
    }
}
#[test]
fn share_search_repeated_marker_and_unordered_object_refused() {
    let mut p = Pages::default();
    p.accept(json!({"results":[],"next_marker":"same"}), None)
        .unwrap();
    assert!(
        p.accept(json!({"results":[],"next_marker":"same"}), None)
            .is_err()
    );
    assert!(p.rows().is_err());
    assert!(
        Pages::default()
            .accept(json!({"results":{"z":{"id":"Z"}}}), None)
            .is_err()
    );
}
#[test]
fn share_search_empty_complete_and_duplicates_preserved() {
    let mut p = Pages::default();
    p.accept(json!({"results":[]}), None).unwrap();
    assert!(p.rows().unwrap().is_empty());
    let mut p = Pages::default();
    p.accept(json!({"results":[{"id":"U1"},{"id":"U1"}]}), None)
        .unwrap();
    assert_eq!(p.rows().unwrap().len(), 2);
}
