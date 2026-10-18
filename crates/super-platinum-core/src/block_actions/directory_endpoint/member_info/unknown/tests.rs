use super::*;
use crate::state::directory_members::Origin;

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
fn store() -> Store {
    let mut store = Store::default();
    store.begin("A", &owner(), &BTreeSet::new()).unwrap();
    store
}
fn capture(store: &Store, ids: &[&str]) -> Capture {
    Capture::network_only(
        store.observations("A", &owner(), 7, "Q").unwrap(),
        &ids.iter().map(|id| (*id).into()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn asset() -> PlaceholderAsset {
    PlaceholderAsset::observed("https://source-fixture.invalid/assets/").unwrap()
}

#[test]
fn exact_source_nonexistent_and_ephemeral_factory_values_match() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let store = store();
    let pending = capture(&store, &["U00000002", "U00000001"]);
    assert_eq!(
        json!(pending.unknown_rows(&asset())),
        oracle["cases"][0]["ephemeral"]
    );
    let actual = pending
        .complete(
            &store,
            "A",
            &owner(),
            7,
            "Q",
            NetworkTerminal::Complete(vec![
                json!({"id":"U00000001","name":"actual","updated":1,"is_stranger":false}),
            ]),
            &asset(),
        )
        .unwrap();
    assert_eq!(actual[1], oracle["cases"][0]["events"][1]["rows"][0]);
    assert_eq!(actual[0]["is_stranger"], false);
    assert!(actual[1].get("is_stranger").is_none());
}

#[test]
fn absent_conditional_reads_chunk_and_keep_marker_mode() {
    let store = store();
    let ids = (1..=101).map(|n| format!("U{n:08}")).collect::<Vec<_>>();
    let plan =
        Capture::network_only(store.observations("A", &owner(), 7, "Q").unwrap(), &ids).unwrap();
    let reads = plan.reads().unwrap();
    assert_eq!(
        reads.iter().map(|r| r.ids().len()).collect::<Vec<_>>(),
        vec![100, 1]
    );
    assert!(reads[0].body().get("ids").is_none());
    assert_eq!(
        reads[0].body()["updated_ids"].as_object().unwrap().len(),
        100
    );
    assert_eq!(
        reads[1].next_page("more").unwrap().body(),
        json!({"updated_ids":{"U00000101":0},"check_interaction":true,"include_profile_only_users":true,"marker":"more"})
    );
}

#[test]
fn changed_scope_or_source_queue_refuses_whole_terminal_projection() {
    let mut store = store();
    let pending = capture(&store, &["U00000001"]);
    assert!(!pending.current(&store, "B", &owner(), 7, "Q"));
    assert!(!pending.current(&store, "A", &owner(), 8, "Q"));
    store
        .enqueue("A", &owner(), Origin::DirectUpsert, vec![], json!({}))
        .unwrap();
    assert!(
        pending
            .complete(
                &store,
                "A",
                &owner(),
                7,
                "Q",
                NetworkTerminal::Failed,
                &asset()
            )
            .is_err()
    );
}

#[test]
fn malformed_or_unrequested_fetched_rows_never_become_placeholders() {
    let store = store();
    for row in [
        json!({"id":"U00000002","is_stranger":false}),
        json!({"id":"U00000001"}),
        json!({"id":"U00000001","is_stranger":"false"}),
    ] {
        assert!(
            capture(&store, &["U00000001"])
                .complete(
                    &store,
                    "A",
                    &owner(),
                    7,
                    "Q",
                    NetworkTerminal::Complete(vec![row]),
                    &asset()
                )
                .is_err()
        );
    }
    let rows = capture(&store, &["U00000001", "U00000001"])
        .complete(
            &store,
            "A",
            &owner(),
            7,
            "Q",
            NetworkTerminal::Failed,
            &asset(),
        )
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["isNonExistent"], true);
}

#[test]
fn asset_observation_has_no_implicit_host_or_credentials() {
    for value in [
        "",
        "http://example.invalid/",
        "https://u:p@example.invalid/",
        "https://example.invalid/?x=y",
        "https://example.invalid/path",
    ] {
        assert!(PlaceholderAsset::observed(value).is_err());
    }
}

#[test]
fn pending_subset_preserves_original_authority_order_and_conditional_mode() {
    let store = store();
    let capture = capture(&store, &["U00000001", "U00000002", "U00000003"]);
    let reads = capture
        .pending_reads(&["U00000003".into(), "U00000001".into()])
        .unwrap();
    assert_eq!(reads[0].ids(), &["U00000003", "U00000001"]);
    assert_eq!(
        reads[0].body(),
        json!({"updated_ids":{"U00000003":0,"U00000001":0},"check_interaction":true,"include_profile_only_users":true})
    );
    assert!(capture.pending_reads(&[]).unwrap().is_empty());
    assert!(capture.pending_reads(&["UNEW0001".into()]).is_err());
    assert!(
        capture
            .pending_reads(&["U00000001".into(), "U00000001".into()])
            .is_err()
    );
}
