use super::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Flags, RendererReply},
    config::WorkspaceSession,
    state::directory_members::{Origin, Store},
};
const SELF: &str = "USELF1234";
const OTHER: &str = "UOTHER001";
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "TGROUP001".into(),
        user_id: SELF.into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    }
}
fn adopt(store: &mut Store, native: &mut HashMap<String, User>, rows: Vec<Value>) {
    store
        .enqueue("A", &owner(), Origin::Directory, rows, json!({}))
        .unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = store
        .prepare("A", &owner(), 7, GENERATION, flags.clone(), native)
        .unwrap();
    assert!(pending.fields().iter().all(|s| s.is_ascii()));
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
    };
    *native = store
        .adopt(pending, reply, &flags, "A", &owner(), 7, GENERATION, native)
        .unwrap()
        .into_parts()
        .1;
}
fn seeded(restricted: bool) -> (Store, HashMap<String, User>) {
    let mut store = Store::default();
    let mut native = HashMap::new();
    store.begin("A", &owner(), &BTreeSet::new()).unwrap();
    adopt(
        &mut store,
        &mut native,
        vec![json!({"id":SELF,"profile":{},"is_restricted":restricted,"opaque":{"original":1}})],
    );
    (store, native)
}
fn subset(store: &Store, native: &HashMap<String, User>) -> Subset {
    store
        .subset(
            "A",
            &owner(),
            7,
            GENERATION,
            &BTreeSet::from([SELF.into()]),
            native,
        )
        .unwrap()
}
#[test]
fn actual_self_source_restricted_gate_and_explicit_unified_default() {
    for restricted in [false, true] {
        let (store, native) = seeded(restricted);
        let policy = selected(&subset(&store, &native), SELF).unwrap();
        assert_eq!(policy.enabled(), !restricted);
        assert!(policy.include_from_another_team());
        assert!(selected(&subset(&store, &native), OTHER).is_err());
    }
}

#[test]
fn ultra_restricted_alone_preserves_group_gate_and_hidden_active_group_is_allowed() {
    let (mut store, mut native) = seeded(false);
    adopt(
        &mut store,
        &mut native,
        vec![json!({"id":SELF,"profile":{},"is_restricted":false,"is_ultra_restricted":true})],
    );
    let policy = selected(&subset(&store, &native), SELF).unwrap();
    assert!(policy.enabled());
    let raw = json!({"id":"SLISTGRP1","handle":"hidden-team","is_hidden":true,"date_delete":0});
    let active = groups::Snapshot::capture(raw.clone(), raw.clone()).unwrap();
    assert!(policy.allows(&active, None).unwrap());
    let mut deleted = raw;
    deleted["date_delete"] = json!(101);
    let deleted = groups::Snapshot::capture(deleted.clone(), deleted).unwrap();
    assert!(!policy.allows(&deleted, None).unwrap());
}
#[test]
fn native_only_self_never_becomes_canonical_permission() {
    let native = HashMap::from([(
        SELF.into(),
        serde_json::from_value(json!({"id":SELF,"is_restricted":false})).unwrap(),
    )]);
    let store = Store::default();
    assert!(
        store
            .subset(
                "A",
                &owner(),
                7,
                GENERATION,
                &BTreeSet::from([SELF.into()]),
                &native
            )
            .is_err()
    );
    let (store, mut native) = seeded(false);
    native.get_mut(SELF).unwrap().is_restricted = true;
    assert!(
        store
            .subset(
                "A",
                &owner(),
                7,
                GENERATION,
                &BTreeSet::from([SELF.into()]),
                &native
            )
            .is_err()
    );
}
#[test]
fn original_account_epoch_generation_and_credentials_are_required() {
    let (store, native) = seeded(false);
    let source = subset(&store, &native);
    assert!(source.current_rows(&store, "A", &owner(), 7, GENERATION, &native));
    assert!(!source.current_rows(&store, "foreign", &owner(), 7, GENERATION, &native));
    assert!(!source.current_rows(&store, "A", &owner(), 8, GENERATION, &native));
    assert!(!source.current_rows(&store, "A", &owner(), 7, "successor", &native));
    let mut rotated = owner();
    rotated.token = "different".into();
    assert!(!source.current_rows(&store, "A", &rotated, 7, GENERATION, &native));
}
#[test]
fn pending_self_update_refuses_permission_until_real_adoption() {
    let (mut store, mut native) = seeded(false);
    let source = subset(&store, &native);
    store
        .enqueue(
            "A",
            &owner(),
            Origin::Directory,
            vec![json!({"id":SELF,"profile":{},"is_restricted":true})],
            json!({}),
        )
        .unwrap();
    assert!(!source.current_rows(&store, "A", &owner(), 7, GENERATION, &native));
    assert!(
        store
            .subset(
                "A",
                &owner(),
                7,
                GENERATION,
                &BTreeSet::from([SELF.into()]),
                &native
            )
            .is_err()
    );
    // A clean separate acquisition demonstrates the raw restricted value remains
    // the authority, rather than the old typed self or a name-policy default.
    let (next, next_native) = seeded(true);
    native = next_native;
    assert!(!selected(&subset(&next, &native), SELF).unwrap().enabled());
}

#[test]
fn unrelated_member_intake_preserves_exact_self_policy_but_self_source_change_retires_it() {
    let (mut store, mut native) = seeded(false);
    let source = subset(&store, &native);
    adopt(
        &mut store,
        &mut native,
        vec![json!({"id":OTHER,"profile":{},"is_restricted":true})],
    );
    assert!(source.current_rows(&store, "A", &owner(), 7, GENERATION, &native));
    assert!(selected(&source, SELF).unwrap().enabled());
    adopt(
        &mut store,
        &mut native,
        vec![json!({"id":SELF,"profile":{},"is_restricted":true,"opaque":{"original":1}})],
    );
    assert!(!source.current_rows(&store, "A", &owner(), 7, GENERATION, &native));
    assert!(!selected(&subset(&store, &native), SELF).unwrap().enabled());
}
