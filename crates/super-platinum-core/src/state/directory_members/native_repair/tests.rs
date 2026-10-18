use super::*;
use serde_json::json;

fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST1234".into(),
        user_id: "USELF1234".into(),
        enterprise_id: None,
        name: "Test".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    }
}
fn flags() -> Flags {
    Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap()
}
fn wire(id: &str) -> Value {
    json!({"id":id,"name":"Original","updated":17,"is_stranger":false,"profile":{"display_name":"Original"}})
}
fn renderer(fields: &[String], generation: &str) -> RendererReply {
    RendererReply {
        generation: generation.into(),
        operation: "ordinary-toLowerCase".into(),
        fields: fields.to_vec(),
        lowered: fields.iter().map(|s| s.to_lowercase()).collect(),
    }
}
fn settle(store: &mut Store, native: &mut HashMap<String, User>) {
    let pending = store
        .prepare("A", &owner(), 7, "Q", flags(), native)
        .unwrap();
    let reply = renderer(pending.fields(), pending.generation());
    let commit = store
        .adopt(pending, reply, &flags(), "A", &owner(), 7, "Q", native)
        .unwrap();
    *native = commit.into_parts().1;
}
fn seeded() -> (Store, HashMap<String, User>) {
    let mut store = Store::default();
    let mut native = HashMap::new();
    store.begin("A", &owner(), &BTreeSet::new()).unwrap();
    store
        .enqueue(
            "A",
            &owner(),
            Origin::DirectUpsert,
            vec![wire("U00000001"), wire("U00000002")],
            json!({}),
        )
        .unwrap();
    settle(&mut store, &mut native);
    (store, native)
}
fn change(native: &mut HashMap<String, User>, id: &str, key: &str, value: Value) {
    let mut raw = serde_json::to_value(native.get(id).unwrap()).unwrap();
    raw[key] = value;
    native.insert(id.into(), serde_json::from_value(raw).unwrap());
}
fn prepare(store: &Store, native: &HashMap<String, User>, fresh: Value) -> NativeRepair {
    store
        .prepare_native_repair("A", &owner(), 7, "Q", flags(), vec![fresh], native)
        .unwrap()
}
fn adopt(
    store: &mut Store,
    native: &HashMap<String, User>,
    pending: NativeRepair,
) -> Result<NativeRepairCommit, Error> {
    let reply = renderer(pending.fields(), pending.generation());
    store.adopt_native_repair(pending, reply, &flags(), "A", &owner(), 7, "Q", native)
}

fn assert_store_unchanged(store: &Store, before: &Store) {
    // Cover both durable records and ephemeral custody without serializing Store.
    assert!(store.saved == before.saved);
    assert_eq!(store.revision, before.revision);
    assert!(Arc::ptr_eq(&store.epoch, &before.epoch));
}

#[test]
fn equal_version_source_skip_stays_exact_then_native_only_repair_converges() {
    let (mut store, mut native) = seeded();
    let prior = store.saved.as_ref().unwrap().records.clone();
    change(&mut native, "U00000001", "name", json!("Drift"));
    store
        .enqueue_member_info(
            "A",
            &owner(),
            vec![wire("U00000001")],
            json!({"U00000001":true}),
            vec!["U00000001".into()],
            vec![],
        )
        .unwrap();
    settle(&mut store, &mut native);
    assert!(store.saved.as_ref().unwrap().records == prior);
    assert!(store.ready("A", &owner(), 7, "Q", &native).is_err());
    let before_source = store
        .saved
        .as_ref()
        .unwrap()
        .records
        .iter()
        .map(Record::current)
        .collect::<Vec<_>>();
    let ledger = store
        .observations("A", &owner(), 7, "Q")
        .unwrap()
        .disconnected()
        .clone();
    let pending = prepare(&store, &native, wire("U00000001"));
    native = adopt(&mut store, &native, pending).unwrap().into_native();
    assert_eq!(
        serde_json::to_value(&native["U00000001"]).unwrap()["name"],
        "Original"
    );
    assert!(store.ready("A", &owner(), 7, "Q", &native).is_ok());
    assert!(
        store
            .saved
            .as_ref()
            .unwrap()
            .records
            .iter()
            .map(Record::current)
            .collect::<Vec<_>>()
            == before_source
    );
    assert_eq!(
        store
            .observations("A", &owner(), 7, "Q")
            .unwrap()
            .disconnected(),
        &ledger
    );
}

#[test]
fn uncovered_native_field_drift_is_not_blessed() {
    let (mut store, mut native) = seeded();
    change(&mut native, "U00000001", "is_admin", json!(true));
    let before = store.clone();
    let pending = prepare(&store, &native, wire("U00000001"));
    assert!(adopt(&mut store, &native, pending).is_err());
    assert_store_unchanged(&store, &before);
    assert!(store.ready("A", &owner(), 7, "Q", &native).is_err());
}

#[test]
fn conflicting_fresh_source_projection_refuses_without_mutation() {
    let (mut store, mut native) = seeded();
    change(&mut native, "U00000001", "name", json!("Drift"));
    let mut fresh = wire("U00000001");
    fresh["profile"]["display_name"] = json!("Conflict");
    let before = store.clone();
    let pending = prepare(&store, &native, fresh);
    assert!(adopt(&mut store, &native, pending).is_err());
    assert_store_unchanged(&store, &before);
}

#[test]
fn late_native_or_scope_replacement_refuses_repair() {
    let (mut store, mut native) = seeded();
    let pending = prepare(&store, &native, wire("U00000001"));
    change(&mut native, "U00000002", "name", json!("Late"));
    assert!(adopt(&mut store, &native, pending).is_err());
    let pending = prepare(&store, &native, wire("U00000001"));
    let reply = renderer(pending.fields(), pending.generation());
    assert!(
        store
            .adopt_native_repair(pending, reply, &flags(), "A", &owner(), 8, "Q", &native)
            .is_err()
    );
}

#[test]
fn repair_does_not_certify_an_unrelated_native_drift() {
    let (mut store, mut native) = seeded();
    change(&mut native, "U00000001", "name", json!("One"));
    change(&mut native, "U00000002", "name", json!("Two"));
    let pending = prepare(&store, &native, wire("U00000001"));
    native = adopt(&mut store, &native, pending).unwrap().into_native();
    assert_eq!(
        serde_json::to_value(&native["U00000001"]).unwrap()["name"],
        "Original"
    );
    assert_eq!(
        serde_json::to_value(&native["U00000002"]).unwrap()["name"],
        "Two"
    );
    assert!(store.ready("A", &owner(), 7, "Q", &native).is_err());
}

#[test]
fn unknown_ids_duplicate_ids_and_pending_batches_are_not_native_repair() {
    let (mut store, native) = seeded();
    assert!(
        store
            .prepare_native_repair(
                "A",
                &owner(),
                7,
                "Q",
                flags(),
                vec![wire("UNEW0001")],
                &native
            )
            .is_err()
    );
    assert!(
        store
            .prepare_native_repair(
                "A",
                &owner(),
                7,
                "Q",
                flags(),
                vec![wire("U00000001"), wire("U00000001")],
                &native
            )
            .is_err()
    );
    store
        .enqueue("A", &owner(), Origin::DirectUpsert, vec![], json!({}))
        .unwrap();
    assert!(
        store
            .prepare_native_repair(
                "A",
                &owner(),
                7,
                "Q",
                flags(),
                vec![wire("U00000001")],
                &native
            )
            .is_err()
    );
}
