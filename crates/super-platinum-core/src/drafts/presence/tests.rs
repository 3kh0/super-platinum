use super::*;
use serde_json::json;
fn row(id: &str, key: &str, updated: &str) -> Value {
    json!({"id":id,"client_msg_id":format!("client-{id}"),"destinations":[{"channel_id":key}],"last_updated_ts":updated})
}
fn event(kind: &str, id: &str, ts: &str, draft: Value) -> Value {
    json!({"type":kind,"draft_id":id,"event_ts":ts,"draft":draft})
}
#[test]
fn attached_composer_thread_and_unique_identity_follow_source_keys() {
    let mut s = Presence::default();
    s.observe(&row("A", "C1", "1")).unwrap();
    assert!(s.has_channel("C1"));
    let mut composer = row("B", "C2", "1");
    composer["is_from_composer"] = json!(true);
    s.observe(&composer).unwrap();
    assert!(!s.has_channel("C2"));
    assert!(s.keys.contains("client-B"));
    let mut thread = row("C", "C3", "1");
    thread["destinations"][0]["thread_ts"] = json!("1.2");
    s.observe(&thread).unwrap();
    assert!(!s.has_channel("C3"));
    assert!(s.keys.contains("C3-1.2"));
    let mut collision = row("D", "C4", "1");
    collision["client_msg_id"] = json!("client-A");
    assert!(!s.observe(&collision).unwrap());
    assert!(!s.has_channel("C4"));
    let mut update = row("A", "OTHER", "2");
    s.observe(&update).unwrap();
    assert!(s.has_channel("C1"));
    assert!(!s.has_channel("OTHER"));
    update["is_from_composer"] = json!(true);
    update["last_updated_ts"] = json!("3");
    s.observe(&update).unwrap();
    assert!(!s.has_channel("C1"));
    assert!(s.keys.contains("client-A"));
}
#[test]
fn source_active_cap_purge_snapshot_and_terminal_empty_ids_are_preserved() {
    let mut s = Presence::default();
    s.observe(&row("OLD", "COLD", "1")).unwrap();
    let mut r = Read::begin(&mut s);
    for page in 0..8 {
        let rows = (0..25)
            .map(|i| {
                row(
                    &format!("D{page}-{i}"),
                    &format!("C{page}-{i}"),
                    &(page * 25 + i + 2).to_string(),
                )
            })
            .collect::<Vec<_>>();
        let step = r
            .active(&mut s, &json!({"ok":true,"drafts":rows,"has_more":true}))
            .unwrap();
        assert_eq!(step == Step::Done, page == 7);
    }
    assert_eq!(s.records.len(), 201);
    assert!(r.needs_purge());
    assert_eq!(
        r.active_ids(
            &mut s,
            &json!({"ok":true,"active_draft_ids":["OLD"],"has_more":true,"next_ts":"1"})
        )
        .unwrap(),
        Step::More("1".into())
    );
    r.active_ids(
        &mut s,
        &json!({"ok":true,"active_draft_ids":[],"has_more":false}),
    )
    .unwrap();
    assert!(!s.has_channel("COLD"));
    assert!(s.has_channel("C0-0"));
    assert_eq!(s.records.len(), 200);
}
#[test]
fn malformed_page_is_atomic_and_new_reader_or_socket_retires_old_read() {
    let mut s = Presence::default();
    let mut old = Read::begin(&mut s);
    let before = s.persisted();
    assert!(
        old.active(
            &mut s,
            &json!({"ok":true,"has_more":false,"drafts":[row("A","C1","1"),{"id":"BAD"}]})
        )
        .is_err()
    );
    assert_eq!(s.persisted(), before);
    let mut fresh = Read::begin(&mut s);
    let empty = json!({"ok":true,"has_more":false,"drafts":[]});
    assert!(old.active(&mut s, &empty).is_err());
    s.realtime(
        "T",
        "draft_create",
        &event("draft_create", "A", "10.1", row("A", "C1", "2")),
    )
    .unwrap();
    assert!(fresh.active(&mut s, &empty).is_err());
    assert!(s.has_channel("C1"));
}
#[test]
fn accepted_socket_clocks_unknown_updates_send_and_cross_team_are_separate() {
    let mut s = Presence::default();
    let unknown = event("draft_update", "A", "10.1", row("A", "C1", "2"));
    assert!(!s.realtime("T", "draft_update", &unknown).unwrap());
    assert!(!s.has_channel("C1"));
    s.realtime(
        "T",
        "draft_create",
        &event("draft_create", "A", "11.1", row("A", "C1", "3")),
    )
    .unwrap();
    assert!(s.has_channel("C1"));
    assert!(
        !s.realtime(
            "T",
            "draft_delete",
            &event("draft_delete", "A", "10.9", Value::Null)
        )
        .unwrap()
    );
    assert!(s.has_channel("C1"));
    let mut foreign = event("draft_update", "A", "12.1", row("A", "C1", "4"));
    foreign["draft"]["team_id"] = json!("OTHER");
    assert!(s.realtime("T", "draft_update", &foreign).is_err());
    assert!(
        s.realtime(
            "T",
            "draft_send",
            &event("draft_send", "A", "12.2", Value::Null)
        )
        .unwrap()
    );
    assert!(!s.has_channel("C1"));
    assert!(!s.observe(&row("A", "C1", "999")).unwrap());
    assert!(!s.has_channel("C1"));
}
#[test]
fn stored_metadata_has_no_body_authority_and_legacy_cache_is_empty() {
    let mut s = Presence::default();
    let mut v = row("A", "C1", "1");
    v["blocks"] = json!([{"text":"private body"}]);
    v["file_ids"] = json!(["FPRIVATE"]);
    s.observe(&v).unwrap();
    let saved = s.persisted();
    assert!(!saved.to_string().contains("private"));
    assert!(!saved.to_string().contains("FPRIVATE"));
    let restored = Presence::restore(saved.clone());
    assert!(restored.has_channel("C1"));
    assert_eq!(restored.persisted(), saved);
    assert_eq!(restored.revision(), 0);
    assert!(!Presence::restore(Value::Null).has_channel("C1"));
}
#[test]
fn contradictory_and_repeated_cursor_pages_never_purge_good_metadata() {
    let mut s = Presence::default();
    s.observe(&row("A", "C1", "1")).unwrap();
    let mut r = Read::begin(&mut s);
    assert!(
        r.active_ids(
            &mut s,
            &json!({"ok":true,"error":"no","active_draft_ids":[],"has_more":false})
        )
        .is_err()
    );
    assert!(s.has_channel("C1"));
    let page = json!({"ok":true,"active_draft_ids":["A"],"has_more":true,"next_ts":"same"});
    r.active_ids(&mut s, &page).unwrap();
    assert!(r.active_ids(&mut s, &page).is_err());
    assert!(s.has_channel("C1"));
}

#[test]
fn actual_requests_keep_closed_read_shapes_and_synthetic_credentials() {
    let c = SlackClient::default();
    let w = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: String::new(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    use crate::slack::client::RequestBody;
    for (request, method, limit, active) in [
        (
            active_request(&c, &w, Some("12.3")),
            "drafts.list",
            "25",
            true,
        ),
        (
            active_ids_request(&c, &w, Some("12.3")),
            "drafts.listActive",
            "1000",
            false,
        ),
    ] {
        assert!(request.url.split('?').next().unwrap().ends_with(method));
        let RequestBody::Form(fields) = request.body else {
            panic!("expected form")
        };
        let map: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(map.get("token").map(String::as_str), Some("synthetic"));
        assert_eq!(map.get("limit").map(String::as_str), Some(limit));
        assert_eq!(map.get("next_ts").map(String::as_str), Some("12.3"));
        assert_eq!(
            map.get("_x_reason").map(String::as_str),
            Some("home-workflow-draft-presence")
        );
        assert_eq!(
            map.get("is_active").map(String::as_str),
            active.then_some("true")
        );
        assert_eq!(map.len(), if active { 5 } else { 4 });
    }
}

#[test]
fn scheduled_projection_preserves_composer_metadata_without_changing_body() {
    let raw = json!({"id":"S1","client_msg_id":"client-S1","is_from_composer":true,"last_updated_ts":"1","blocks":[{"text":"retained"}],"destinations":[{"channel_id":"C1"}],"date_scheduled":100});
    let mut message: crate::scheduled::ScheduledMessage =
        serde_json::from_value(raw.clone()).unwrap();
    let mut s = Presence::default();
    s.observe_scheduled(&message).unwrap();
    assert!(!s.has_channel("C1"));
    assert_eq!(message.blocks, raw["blocks"].as_array().unwrap().clone());
    message.id = "S2".into();
    message.client_msg_id = "client-S2".into();
    message.is_from_composer = Value::Null;
    s.observe_scheduled(&message).unwrap();
    assert!(s.has_channel("C1"));
}

#[test]
fn independent_active_and_purge_branches_keep_successful_sibling_effects() {
    for active_fails in [true, false] {
        let mut store = Presence::default();
        store.observe(&row("OLD", "COLD", "1")).unwrap();
        let mut read = Read::begin(&mut store);
        let failed = json!({"ok":false,"error":"synthetic_failure"});
        if active_fails {
            assert!(read.active(&mut store, &failed).is_err());
            assert_eq!(
                read.active_ids(&mut store, &json!({"ok":true,"active_draft_ids":[]}))
                    .unwrap(),
                Step::Done
            );
            assert!(!store.has_channel("COLD"));
        } else {
            assert!(read.active_ids(&mut store, &failed).is_err());
            assert_eq!(
                read.active(
                    &mut store,
                    &json!({"ok":true,"drafts":[row("NEW","CNEW","2")]})
                )
                .unwrap(),
                Step::Done
            );
            assert!(store.has_channel("COLD"));
            assert!(store.has_channel("CNEW"));
        }
        assert!(read.current(&store));
    }
}

#[test]
fn failed_later_page_preserves_accepted_partial_metadata_without_purging_absent_keys() {
    let mut store = Presence::default();
    store.observe(&row("OLD", "COLD", "1")).unwrap();
    let mut read = Read::begin(&mut store);
    assert_eq!(
        read.active(
            &mut store,
            &json!({"ok":true,"drafts":[row("NEW","CNEW","2")],"has_more":true})
        )
        .unwrap(),
        Step::More("2".into())
    );
    let before = store.persisted();
    assert!(
        read.active(
            &mut store,
            &json!({"ok":true,"drafts":[row("NEXT","CNEXT","3"),{"id":"BROKEN"}],"has_more":false})
        )
        .is_err()
    );
    assert_eq!(store.persisted(), before);
    assert!(store.has_channel("COLD"));
    assert!(store.has_channel("CNEW"));
    assert!(!store.has_channel("CNEXT"));
}

#[test]
fn accepted_noop_event_and_stream_gap_retire_reads_without_erasing_warm_keys() {
    let mut store = Presence::default();
    store.observe(&row("A", "C1", "1")).unwrap();
    let first = Read::begin(&mut store);
    let before = store.revision();
    assert!(
        !store
            .realtime(
                "T",
                "draft_update",
                &event(
                    "draft_update",
                    "UNKNOWN",
                    "10.1",
                    row("UNKNOWN", "COTHER", "2")
                )
            )
            .unwrap()
    );
    assert_ne!(store.revision(), before);
    assert!(!first.current(&store));
    let second = Read::begin(&mut store);
    store.invalidate();
    assert!(!second.current(&store));
    assert!(store.has_channel("C1"));
    assert!(!store.has_channel("COTHER"));
}

#[test]
fn malformed_cached_identity_cannot_be_restored_as_a_positive_presence() {
    let mut store = Presence::default();
    store.observe(&row("A", "C1", "1")).unwrap();
    let valid = store.persisted();
    for changed in 0..6 {
        let mut raw = valid.clone();
        match changed {
            0 => raw[0]["id"] = json!("bad\nidentity"),
            1 => raw[0]["unique"] = json!("x".repeat(256)),
            2 => raw[0]["key"] = json!("bad\nkey"),
            3 => {
                raw[0]["composer"] = json!(true);
            }
            4 => {
                let mut duplicate = raw[0].clone();
                duplicate["id"] = json!("B");
                duplicate["key"] = json!("C2");
                raw.as_array_mut().unwrap().push(duplicate);
            }
            _ => raw[0]["updated"] = json!(-1),
        }
        assert_eq!(Presence::restore(raw).persisted(), json!([]));
    }
    assert_eq!(Presence::restore(valid.clone()).persisted(), valid);
}

#[test]
fn scheduled_raw_composer_flag_is_retained_and_malformed_flag_refuses_presence_only() {
    let mut raw = row("A", "C1", "1");
    raw["is_from_composer"] = json!("unexpected");
    let scheduled: crate::scheduled::ScheduledMessage = serde_json::from_value(raw).unwrap();
    assert_eq!(scheduled.is_from_composer, json!("unexpected"));
    let mut store = Presence::default();
    assert!(store.observe_scheduled(&scheduled).is_err());
    assert!(!store.has_channel("C1"));
    let legacy: crate::scheduled::ScheduledMessage =
        serde_json::from_value(row("B", "C2", "2")).unwrap();
    assert!(legacy.is_from_composer.is_null());
    store.observe_scheduled(&legacy).unwrap();
    assert!(store.has_channel("C2"));
}
