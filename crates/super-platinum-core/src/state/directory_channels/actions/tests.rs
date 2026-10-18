use super::*;
use serde_json::json;
fn owner() -> WorkspaceSession {
    serde_json::from_value(json!({"team_id":"TONE","user_id":"USELF","name":"Fixture","token":"synthetic","url":"https://fixture.slack.com"})).unwrap()
}
fn facts() -> Facts {
    Facts {
        self_id: "USELF".into(),
        current_team: "TONE".into(),
        omit_team: "TONE".into(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    }
}
fn setup() -> (
    Store,
    BTreeMap<String, Channel>,
    freshness::Scope,
    freshness::Manager,
    freshness::Batch,
) {
    let mut store = Store::default();
    let boot = store.begin_full_boot("A", &owner(), 1).unwrap();
    assert!(store.finish_full_boot(boot, "A", &owner(), 1));
    store.begin("A", &owner(), &BTreeMap::new()).unwrap();
    let scope = freshness::Scope::new("A", &owner(), 1).unwrap();
    let mut manager = freshness::Manager::new(scope.clone());
    let observation = freshness::Observation {
        slow_connection_count: Some(1),
        major_version: Some(1),
        enabled: true,
    };
    manager
        .maybe_refetch(
            &scope,
            freshness::Clock::new(0).unwrap(),
            observation,
            "C00000001",
            false,
        )
        .unwrap();
    let batch = manager
        .begin_due(&scope, freshness::Clock::new(200).unwrap(), observation)
        .unwrap()
        .unwrap();
    (store, BTreeMap::new(), scope, manager, batch)
}
fn raw(store: &mut Store, name: &str) {
    store
        .enqueue(
            "A",
            &owner(),
            1,
            Origin::Direct,
            vec![json!({"id":"C00000001","name":name,"is_channel":true,"is_member":false})],
        )
        .unwrap();
}

#[test]
fn resync_unchanged_waits_for_original_predecessor_without_conditional_batch() {
    let (mut store, mut native, _, _, _) = setup();
    raw(&mut store, "accepted-earlier");
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    (store, native) = store
        .queue_resync_unchanged(
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &["C00000001".into()],
        )
        .unwrap()
        .into_parts();
    raw(&mut store, "accepted-later");
    drain_raw(&mut store, &mut native);
    assert_eq!(
        native["C00000001"].name.as_deref(),
        Some("accepted-earlier")
    );
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    (store, native) = store
        .drain_source_action(&observed, "A", &owner(), 1, "Q", &native)
        .unwrap()
        .unwrap()
        .into_parts();
    assert!(store.has_pending());
    assert_eq!(
        native["C00000001"].name.as_deref(),
        Some("accepted-earlier")
    );
    drain_raw(&mut store, &mut native);
    assert_eq!(native["C00000001"].name.as_deref(), Some("accepted-later"));
}

#[test]
fn resync_unchanged_missing_row_and_retired_observation_do_not_create_absence() {
    let (mut store, mut native, _, _, _) = setup();
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    (store, native) = store
        .queue_resync_unchanged(
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &["C00000001".into()],
        )
        .unwrap()
        .into_parts();
    assert!(
        store
            .queue_resync_unchanged(
                &observed,
                "A",
                &owner(),
                1,
                "Q",
                &native,
                &["C00000001".into()]
            )
            .is_err()
    );
    let current = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    assert!(
        store
            .drain_source_action(&current, "A", &owner(), 1, "Q", &native)
            .is_err()
    );
    assert!(native.is_empty());
    assert!(store.has_pending());
}
fn drain_raw(store: &mut Store, native: &mut BTreeMap<String, Channel>) {
    let f = facts();
    let pending = store
        .prepare("A", &owner(), 1, "Q", native, f.clone())
        .unwrap();
    assert!(pending.fields().iter().all(|v| v.is_ascii()));
    let reply = RendererReply {
        generation: "Q".into(),
        fields: pending.fields().to_vec(),
        values: pending
            .fields()
            .iter()
            .map(|v| v.to_ascii_lowercase())
            .collect(),
    };
    (*store, *native) = store
        .finish(pending, &f, reply, "A", &owner(), 1, "Q", native)
        .unwrap()
        .into_parts();
}
#[test]
fn membership_action_follows_earlier_raw_and_precedes_later_raw() {
    let (mut store, mut native, scope, manager, batch) = setup();
    raw(&mut store, "first");
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    (store, native) = manager
        .queue_joined_memberships(
            &scope,
            &batch,
            &store,
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &["C00000001".into()],
        )
        .unwrap()
        .into_parts();
    raw(&mut store, "later");
    assert_eq!(store.saved.as_ref().unwrap().pending.len(), 3);
    drain_raw(&mut store, &mut native);
    assert_eq!(native["C00000001"].name.as_deref(), Some("first"));
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    (store, native) = store
        .drain_source_action(&observed, "A", &owner(), 1, "Q", &native)
        .unwrap()
        .unwrap()
        .into_parts();
    assert_eq!(
        native["C00000001"].extra.get("is_member"),
        Some(&json!(true))
    );
    assert_eq!(store.saved.as_ref().unwrap().pending.len(), 1);
    assert_eq!(native["C00000001"].name.as_deref(), Some("first"));
    drain_raw(&mut store, &mut native);
    assert_eq!(native["C00000001"].name.as_deref(), Some("later"));
}
#[test]
fn unchanged_action_roundtrips_and_requires_exact_native_predecessor() {
    let (mut store, mut native, scope, manager, batch) = setup();
    raw(&mut store, "first");
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    (store, native) = manager
        .queue_unchanged_channels(
            &scope,
            &batch,
            &store,
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &["C00000001".into()],
        )
        .unwrap()
        .into_parts();
    let cached = store.cached().unwrap();
    store = Store::restore(Some(&cached), &owner());
    assert_eq!(store.cached(), Some(cached));
    drain_raw(&mut store, &mut native);
    native.get_mut("C00000001").unwrap().name = Some("drift".into());
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    let before = store.cached();
    assert!(
        store
            .drain_source_action(&observed, "A", &owner(), 1, "Q", &native)
            .is_err()
    );
    assert_eq!(store.cached(), before);
    native.get_mut("C00000001").unwrap().name = Some("first".into());
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    (store, native) = store
        .drain_source_action(&observed, "A", &owner(), 1, "Q", &native)
        .unwrap()
        .unwrap()
        .into_parts();
    assert!(store.ready("A", &owner(), 1, "Q", &native).is_ok());
}
#[test]
fn action_mint_refuses_foreign_batch_identity_and_restored_mixed_payload() {
    let (store, native, scope, manager, batch) = setup();
    let observed = store.observation("A", &owner(), 1, "Q", &native).unwrap();
    assert!(
        manager
            .queue_joined_memberships(
                &scope,
                &batch,
                &store,
                &observed,
                "B",
                &owner(),
                1,
                "Q",
                &native,
                &["C00000001".into()]
            )
            .is_err()
    );
    assert!(
        manager
            .queue_joined_memberships(
                &scope,
                &batch,
                &store,
                &observed,
                "A",
                &owner(),
                1,
                "Q",
                &native,
                &["C00000002".into()]
            )
            .is_err()
    );
    let (store, _) = manager
        .queue_joined_memberships(
            &scope,
            &batch,
            &store,
            &observed,
            "A",
            &owner(),
            1,
            "Q",
            &native,
            &["C00000001".into()],
        )
        .unwrap()
        .into_parts();
    let mut raw = store.cached().unwrap();
    raw["pending"][0]["rows"] = json!([{"id":"C00000001"}]);
    assert!(Store::restore(Some(&raw), &owner()).cached().is_none());
}
