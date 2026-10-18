use super::*;
use serde_json::json;
use std::collections::HashMap;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Flags, RendererReply},
    config::WorkspaceSession,
    state::directory_members::{Origin, Store},
};
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TTEST1234".into(),
        user_id: "USELF1234".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.slack.com/".into(),
        token: "synthetic".into(),
    }
}
fn seeded() -> (Store, HashMap<String, User>) {
    let mut store = Store::default();
    let mut native = HashMap::new();
    let owner = owner();
    store.begin("account", &owner, &BTreeSet::new()).unwrap();
    store.enqueue("account",&owner,Origin::Directory,
        vec![json!({"id":"U00000002","name":"alpha","profile":{"display_name":"Alpha"},"unknown":{"kept":true}}),
            json!({"id":"U00000003","name":"blocked","profile":{},"is_stranger":true})],
        json!({"U00000002":true,"U00000003":false})).unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = store
        .prepare("account", &owner, 7, GENERATION, flags.clone(), &native)
        .unwrap();
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
    };
    let result = store
        .adopt(
            pending, reply, &flags, "account", &owner, 7, GENERATION, &native,
        )
        .unwrap();
    native = result.into_parts().1;
    (store, native)
}
#[test]
fn actual_adopted_source_is_required_not_wire_or_typed_substitute() {
    let (store, native) = seeded();
    let subset = store
        .subset(
            "account",
            &owner(),
            7,
            GENERATION,
            &BTreeSet::from(["U00000002".into(), "U00000003".into()]),
            &native,
        )
        .unwrap();
    let rows: Vec<_> = subset.rows().iter().map(|r| r.source.clone()).collect();
    let admitted = selected(&subset, &rows).unwrap();
    assert_eq!(admitted[0].0.id, "U00000002");
    assert_eq!(admitted[1].1, Some(false));
    assert!(selected(&subset, &[rows[0].clone(), rows[0].clone()]).is_err());
    let mut changed = rows[0].clone();
    changed["unknown"] = json!({"kept":false});
    assert!(selected(&subset, &[changed]).is_err());
    assert!(selected(&subset, &[json!({"id":"U00000002"})]).is_err());
    assert!(selected(&subset, &[json!({"id":"U99999999"})]).is_err());
}
#[test]
fn source_native_owner_and_pending_changes_retire_policy_witness() {
    let (mut store, mut native) = seeded();
    let owner = owner();
    let subset = store
        .subset(
            "account",
            &owner,
            7,
            GENERATION,
            &BTreeSet::from(["U00000002".into()]),
            &native,
        )
        .unwrap();
    assert!(subset.current(&store, "account", &owner, 7, GENERATION, &native));
    assert!(!subset.current(&store, "foreign", &owner, 7, GENERATION, &native));
    assert!(!subset.current(&store, "account", &owner, 8, GENERATION, &native));
    let mut rotated = owner.clone();
    rotated.token = "rotated".into();
    assert!(!subset.current(&store, "account", &rotated, 7, GENERATION, &native));
    native.get_mut("U00000002").unwrap().deleted = true;
    assert!(!subset.current(&store, "account", &owner, 7, GENERATION, &native));
    native.get_mut("U00000002").unwrap().deleted = false;
    assert!(subset.current(&store, "account", &owner, 7, GENERATION, &native));
    store
        .enqueue(
            "account",
            &owner,
            Origin::Directory,
            vec![json!({"id":"U00000002","profile":{}})],
            json!({"U00000002":true}),
        )
        .unwrap();
    assert!(!subset.current(&store, "account", &owner, 7, GENERATION, &native));
}
#[test]
fn eligibility_is_bound_to_full_raw_row_including_unknown_fields() {
    let row = json!({"id":"U00000002","profile":{},"unknown":7});
    let rows = BTreeMap::from([("U00000002".into(), (row.clone(), true))]);
    assert!(observed(&rows, &row).unwrap());
    let mut changed = row.clone();
    changed["deleted"] = json!(true);
    assert!(observed(&rows, &changed).is_err());
    changed = row.clone();
    changed["unknown"] = json!(8);
    assert!(observed(&rows, &changed).is_err());
    assert!(observed(&rows, &json!({"id":"U99999999"})).is_err());
    assert!(
        !observed(
            &BTreeMap::from([("U00000002".into(), (row.clone(), false))]),
            &row
        )
        .unwrap()
    );
}
