use super::*;

fn scope(thread: bool) -> Scope {
    Scope {
        account_id: "00000000-0000-4000-8000-000000000071".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: thread.then(|| "100.000001".into()),
        },
    }
}

fn raw(scope: &Scope, kind: &str) -> RawEvent {
    let snapshot = codec::encode(&scope.key, "Remote text").unwrap();
    serde_json::from_value(json!({
        "type":kind, "draft_id":"Dr1", "event_ts":"10.000001",
        "draft":{
            "id":"Dr1", "client_msg_id":"00000000-0000-4000-8000-000000000072",
            "team_id":"T1", "user_id":"U1", "last_updated_ts":"2000",
            "is_deleted":false, "is_sent":false, "date_scheduled":0,
            "blocks":snapshot.blocks, "file_ids":[], "destinations":snapshot.destinations
        }
    }))
    .unwrap()
}

#[test]
fn ordinary_create_and_update_accept_absent_or_boolean_false_flag() {
    for thread in [false, true] {
        let scope = scope(thread);
        for kind in ["draft_create", "draft_update"] {
            for explicit_false in [false, true] {
                let mut raw = raw(&scope, kind);
                if explicit_false {
                    raw.rest.get_mut("draft").unwrap()["is_from_composer"] = json!(false);
                }
                let PreparedKind::Event(event) = decode(&scope, "U1", &raw).unwrap().content else {
                    panic!("active event required")
                };
                let ObservationKind::Active(remote) = event.kind else {
                    panic!("ordinary false flag must remain active")
                };
                assert_eq!(remote.scope, scope);
                assert_eq!(remote.id, "Dr1");
                assert_eq!(remote.client_msg_id, "00000000-0000-4000-8000-000000000072");
                assert_eq!(remote.last_updated_ts, "2000");
                assert_eq!(
                    remote.snapshot,
                    codec::encode(&scope.key, "Remote text").unwrap()
                );
            }
        }
    }
}

#[tokio::test]
async fn true_or_untyped_flag_retains_raw_evidence_and_blocks_owned_writes() {
    for value in [
        json!(true),
        Value::Null,
        json!("false"),
        json!(0),
        json!([]),
        json!({}),
    ] {
        let f = Fixture::new().await;
        let mut raw = f.raw("Remote", "10.000001");
        raw.rest.get_mut("draft").unwrap()["is_from_composer"] = value;
        let evidence = raw.rest["draft"].clone();
        let observed = f.owner.observe_event(f.prepared(&raw)).await.unwrap();
        assert!(same_local(&observed.after.local, &f.local));
        let stored = observed.after.stored.unwrap();
        assert_eq!(
            stored.state.blocked_observation().unwrap().kind,
            ObservationKind::Unsupported(evidence)
        );
        assert!(matches!(
            f.owner
                .dispatch(stored.revision, |_| async { panic!("must not dispatch") })
                .await,
            Err(Fault::Pending)
        ));
        assert_eq!(
            f.restart().load().await.unwrap().stored.unwrap().state,
            stored.state
        );
    }
}

#[test]
fn false_flag_does_not_admit_partial_unknown_or_terminal_draft_shapes() {
    let scope = scope(true);
    for mutation in 0..7 {
        let mut raw = raw(&scope, "draft_update");
        let draft = raw.rest.get_mut("draft").unwrap();
        draft["is_from_composer"] = json!(false);
        match mutation {
            0 => {
                draft.as_object_mut().unwrap().remove("blocks");
            }
            1 => draft["future_semantics"] = json!(false),
            2 => draft["is_sent"] = json!(true),
            3 => draft["is_deleted"] = json!(true),
            4 => draft["date_scheduled"] = json!(500),
            5 => draft["attachments"] = json!([{"text":"Opaque"}]),
            _ => draft["destinations"][0]["channel_id"] = json!("Other"),
        }
        let evidence = draft.clone();
        let PreparedKind::Event(event) = decode(&scope, "U1", &raw).unwrap().content else {
            panic!("event required")
        };
        assert_eq!(event.kind, ObservationKind::Unsupported(evidence));
    }
}

#[test]
fn false_flag_does_not_bypass_source_or_exact_identity_checks() {
    let scope = scope(false);
    for (field, expected) in [
        ("user_id", Invalid::Source),
        ("team_id", Invalid::Source),
        ("client_msg_id", Invalid::Identity),
        ("id", Invalid::Identity),
    ] {
        let mut raw = raw(&scope, "draft_update");
        let draft = raw.rest.get_mut("draft").unwrap();
        draft["is_from_composer"] = json!(false);
        draft[field] = json!("wrong");
        assert_eq!(decode(&scope, "U1", &raw).err(), Some(expected));
    }
}

#[tokio::test]
async fn false_flag_observation_commits_paired_content_and_ordering_before_restart() {
    let f = Fixture::new().await;
    let mut raw = f.raw("Remote ordinary text", "10.000002");
    raw.rest.get_mut("draft").unwrap()["is_from_composer"] = json!(false);
    let observed = f.owner.observe_event(f.prepared(&raw)).await.unwrap();
    assert!(observed.changed);
    assert_eq!(observed.after.local.composer.text, "Remote ordinary text");
    let stored = observed.after.stored.unwrap();
    assert!(stored.state.blocked_observation().is_none());
    assert_eq!(stored.state.acknowledged().unwrap().last_updated_ts, "2000");
    let restored = f.restart().load().await.unwrap();
    assert!(same_local(&restored.local, &observed.after.local));
    assert_eq!(restored.stored.unwrap().state, stored.state);
    let duplicate = f.owner.observe_event(f.prepared(&raw)).await.unwrap();
    assert!(!duplicate.changed);
    assert_eq!(duplicate.after.stored.unwrap().revision, stored.revision);
}
