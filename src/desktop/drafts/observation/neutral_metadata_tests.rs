use super::*;

fn event(thread: bool, kind: &str) -> (Scope, RawEvent) {
    let scope = Scope {
        account_id: "00000000-0000-4000-8000-000000000071".into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: thread.then(|| "100.000001".into()),
        },
    };
    let snapshot = codec::encode(&scope.key, "Remote text").unwrap();
    let raw = serde_json::from_value(json!({"type":kind,"draft_id":"Dr1","event_ts":"10.000001","draft":{
        "id":"Dr1","client_msg_id":"00000000-0000-4000-8000-000000000072","team_id":"T1","user_id":"U1",
        "last_updated_ts":"2000","is_deleted":false,"is_sent":false,"is_from_composer":false,"date_scheduled":0,
        "blocks":snapshot.blocks,"file_ids":[],"destinations":snapshot.destinations
    }})).unwrap();
    (scope, raw)
}

#[test]
fn neutral_fields_keep_channel_and_thread_create_update_payload_exact() {
    for thread in [false, true] {
        for kind in ["draft_create", "draft_update"] {
            for fields in [
                json!({"date_stashed":0}),
                json!({"file_permissions":[]}),
                json!({"date_stashed":0,"file_permissions":[]}),
            ] {
                let (scope, mut raw) = event(thread, kind);
                raw.rest
                    .get_mut("draft")
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .extend(fields.as_object().unwrap().clone());
                let PreparedKind::Event(event) = decode(&scope, "U1", &raw).unwrap().content else {
                    panic!("event")
                };
                let ObservationKind::Active(remote) = event.kind else {
                    panic!("active")
                };
                assert_eq!(remote.scope, scope);
                assert_eq!(
                    remote.snapshot,
                    codec::encode(&scope.key, "Remote text").unwrap()
                );
                assert_eq!(remote.last_updated_ts, "2000");
            }
        }
    }
}

#[tokio::test]
async fn nonneutral_fields_retain_durable_raw_block_and_never_dispatch() {
    for (field, value) in [
        ("date_stashed", json!("0")),
        ("date_stashed", json!(1)),
        ("date_stashed", json!(-1)),
        ("date_stashed", Value::Null),
        ("date_stashed", json!(false)),
        ("file_permissions", json!({})),
        ("file_permissions", Value::Null),
        ("file_permissions", json!(false)),
        ("file_permissions", json!("[]")),
        ("file_permissions", json!([{"file_id":"F1","grant":false}])),
        ("mpdm_name", json!("")),
        ("mpdm_name", json!(" ")),
        ("file_ids", json!(["F1"])),
        ("is_from_composer", Value::Null),
        ("is_from_composer", json!("false")),
        ("is_sent", json!(true)),
        ("future_field", json!(false)),
    ] {
        let f = Fixture::new().await;
        let mut raw = f.raw("Remote", "10.000001");
        let draft = raw.rest.get_mut("draft").unwrap();
        draft["date_stashed"] = json!(0);
        draft["file_permissions"] = json!([]);
        draft[field] = value;
        let evidence = draft.clone();
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
fn neutral_fields_do_not_override_source_identity_or_destination_guards() {
    for (field, expected) in [
        ("team_id", Invalid::Source),
        ("user_id", Invalid::Source),
        ("id", Invalid::Identity),
        ("client_msg_id", Invalid::Identity),
    ] {
        let (scope, mut raw) = event(true, "draft_update");
        let draft = raw.rest.get_mut("draft").unwrap();
        draft["date_stashed"] = json!(0);
        draft["file_permissions"] = json!([]);
        draft[field] = json!("wrong");
        assert_eq!(decode(&scope, "U1", &raw).err(), Some(expected));
    }
    let (scope, mut raw) = event(true, "draft_update");
    let draft = raw.rest.get_mut("draft").unwrap();
    draft["date_stashed"] = json!(0);
    draft["file_permissions"] = json!([]);
    draft["destinations"][0]["channel_id"] = json!("C2");
    let evidence = draft.clone();
    let PreparedKind::Event(event) = decode(&scope, "U1", &raw).unwrap().content else {
        panic!("event")
    };
    assert_eq!(event.kind, ObservationKind::Unsupported(evidence));
}
