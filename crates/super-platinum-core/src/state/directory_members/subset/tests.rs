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
