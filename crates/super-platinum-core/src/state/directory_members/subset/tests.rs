use super::*;
use crate::slack::models::User;
use serde_json::json;
use std::collections::HashMap;
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
fn row(id: &str, name: &str) -> Value {
    json!({"id":id,"name":name,"profile":{"display_name":name}})
}
fn receipt(p: &Pending) -> RendererReply {
    RendererReply {
        generation: p.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: p.fields().to_vec(),
        lowered: p.fields().iter().map(|s| s.to_lowercase()).collect(),
    }
}
fn settle(s: &mut Store, native: &mut HashMap<String, User>) {
    let p = s.prepare("A", &owner(), 7, "Q", flags(), native).unwrap();
    let response = receipt(&p);
    let commit = s
        .adopt(p, response, &flags(), "A", &owner(), 7, "Q", native)
        .unwrap();
    let (_, staged) = commit.into_parts();
    *native = staged;
}
fn seeded() -> (Store, HashMap<String, User>) {
    let mut s = Store::default();
    let mut native = HashMap::new();
    s.begin("A", &owner(), &BTreeSet::new()).unwrap();
    s.enqueue(
        "A",
        &owner(),
        Origin::Directory,
        vec![row("UZLAST123", "Z"), row("UAFIRST12", "A")],
        json!({"UZLAST123":true,"UAFIRST12":true}),
    )
    .unwrap();
    settle(&mut s, &mut native);
    (s, native)
}
#[test]
fn bounded_members_ignore_unrelated_incompleteness_but_never_certify_absence() {
    let (mut store, mut native) = seeded();
    let wanted = BTreeSet::from(["UZLAST123".into()]);
    native.get_mut("UAFIRST12").unwrap().name = Some("unrelated drift".into());
    store
        .enqueue(
            "A",
            &owner(),
            Origin::Directory,
            vec![row("UNEW12345", "pending")],
            json!({"UNEW12345":true}),
        )
        .unwrap();
    assert!(store.ready("A", &owner(), 7, "Q", &native).is_err());
    let witness = store
        .subset("A", &owner(), 7, "Q", &wanted, &native)
        .unwrap();
    assert_eq!(witness.rows().len(), 1);
    assert_eq!(witness.rows()[0].source["id"], "UZLAST123");
    assert!(witness.current(&store, "A", &owner(), 7, "Q", &native));
    assert!(
        store
            .subset(
                "A",
                &owner(),
                7,
                "Q",
                &BTreeSet::from(["UABSENT12".into()]),
                &native
            )
            .is_err()
    );
    native.get_mut("UZLAST123").unwrap().name = Some("target drift".into());
    assert!(!witness.current(&store, "A", &owner(), 7, "Q", &native));
}
#[test]
fn target_pending_rows_interactions_disconnections_and_invalidations_refuse() {
    let wanted = BTreeSet::from(["UZLAST123".into()]);
    for reason in 0..4 {
        let (mut store, native) = seeded();
        let saved = store.saved.as_mut().unwrap();
        if reason == 3 {
            saved.invalidated.insert("UZLAST123".into());
        } else {
            saved.pending.push(Batch {
                origin: Origin::MemberInfo,
                rows: if reason == 0 {
                    vec![row("UZLAST123", "pending")]
                } else {
                    vec![]
                },
                interactions: if reason == 1 {
                    json!({"UZLAST123":false})
                } else {
                    json!({})
                },
                disconnection: (reason == 2).then(|| Disconnection {
                    requested: vec!["UZLAST123".into()],
                    marked: vec![],
                }),
            });
        }
        assert!(
            store
                .subset("A", &owner(), 7, "Q", &wanted, &native)
                .is_err()
        );
    }
}
#[test]
fn original_scope_and_source_revision_remain_required() {
    let (mut store, native) = seeded();
    let wanted = BTreeSet::from(["UZLAST123".into(), "UAFIRST12".into()]);
    let witness = store
        .subset("A", &owner(), 7, "Q", &wanted, &native)
        .unwrap();
    assert_eq!(
        witness
            .rows()
            .iter()
            .map(|r| r.source["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["UZLAST123", "UAFIRST12"]
    );
    assert!(!witness.current(&store, "B", &owner(), 7, "Q", &native));
    assert!(!witness.current(&store, "A", &owner(), 8, "Q", &native));
    assert!(!witness.current(&store, "A", &owner(), 7, "later", &native));
    let mut rotated = owner();
    rotated.token = "changed synthetic".into();
    assert!(!witness.current(&store, "A", &rotated, 7, "Q", &native));
    store.changed();
    assert!(!witness.current(&store, "A", &owner(), 7, "Q", &native));
}

#[test]
fn bounded_current_rows_survive_unrelated_pending_and_adopted_intake() {
    let (mut store, mut native) = seeded();
    let witness = store
        .subset(
            "A",
            &owner(),
            7,
            "Q",
            &BTreeSet::from(["UZLAST123".into()]),
            &native,
        )
        .unwrap();
    store
        .enqueue(
            "A",
            &owner(),
            Origin::Directory,
            vec![row("UNEW12345", "New")],
            json!({}),
        )
        .unwrap();
    assert!(witness.current_rows(&store, "A", &owner(), 7, "Q", &native));
    settle(&mut store, &mut native);
    assert!(witness.current_rows(&store, "A", &owner(), 7, "Q", &native));
    assert!(!witness.current(&store, "A", &owner(), 7, "Q", &native));
    let clone = store.clone();
    assert!(witness.current_rows(&clone, "A", &owner(), 7, "Q", &native));
}
#[test]
fn bounded_current_rows_reject_requested_pending_source_native_and_scope_drift() {
    for change in 0..7 {
        let (mut store, mut native) = seeded();
        let witness = store
            .subset(
                "A",
                &owner(),
                7,
                "Q",
                &BTreeSet::from(["UZLAST123".into()]),
                &native,
            )
            .unwrap();
        match change {
            0 => store
                .enqueue(
                    "A",
                    &owner(),
                    Origin::Directory,
                    vec![row("UZLAST123", "Z")],
                    json!({}),
                )
                .unwrap(),
            1 => {
                native.get_mut("UZLAST123").unwrap().is_restricted = true;
            }
            2 => {
                store.saved.as_mut().unwrap().records[0].source["opaque"] = json!("changed");
            }
            3 => {
                store
                    .saved
                    .as_mut()
                    .unwrap()
                    .invalidated
                    .insert("UZLAST123".into());
            }
            4 => {
                store.saved.as_mut().unwrap().records[0].interaction = Some(false);
            }
            5 => {
                // Rebinding only the record's typed witness cannot lend an old
                // permission capture new native metadata with unchanged source.
                native.get_mut("UZLAST123").unwrap().is_restricted = true;
                store.saved.as_mut().unwrap().records[0].native_witness =
                    serde_json::to_value(&native["UZLAST123"]).unwrap();
            }
            _ => store
                .enqueue(
                    "A",
                    &owner(),
                    Origin::Directory,
                    vec![],
                    json!({"UZLAST123":false}),
                )
                .unwrap(),
        }
        assert!(
            !witness.current_rows(&store, "A", &owner(), 7, "Q", &native),
            "change {change}"
        );
    }
    let (store, native) = seeded();
    let witness = store
        .subset(
            "A",
            &owner(),
            7,
            "Q",
            &BTreeSet::from(["UZLAST123".into()]),
            &native,
        )
        .unwrap();
    for (account, epoch, generation) in [("foreign", 7, "Q"), ("A", 8, "Q"), ("A", 7, "other")] {
        assert!(!witness.current_rows(&store, account, &owner(), epoch, generation, &native));
    }
    let mut changed = owner();
    changed.token = "rotated".into();
    assert!(!witness.current_rows(&store, "A", &changed, 7, "Q", &native));
}
#[test]
fn identical_rows_after_begin_or_cache_restore_do_not_reauthorize_old_subset() {
    let (mut store, native) = seeded();
    let witness = store
        .subset(
            "A",
            &owner(),
            7,
            "Q",
            &BTreeSet::from(["UZLAST123".into()]),
            &native,
        )
        .unwrap();
    let cache = store.cached().unwrap();
    let restored = Store::restore(Some(&cache), &owner());
    assert!(
        restored
            .subset(
                "A",
                &owner(),
                7,
                "Q",
                &BTreeSet::from(["UZLAST123".into()]),
                &native
            )
            .is_ok()
    );
    assert!(!witness.current_rows(&restored, "A", &owner(), 7, "Q", &native));
    let original = store.saved.clone();
    store.begin("A", &owner(), &BTreeSet::new()).unwrap();
    store.saved = original; // identical rows deliberately isolate the lineage fence
    assert!(!witness.current_rows(&store, "A", &owner(), 7, "Q", &native));
}
