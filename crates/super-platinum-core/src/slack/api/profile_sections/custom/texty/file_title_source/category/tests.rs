use super::*;
use crate::config::WorkspaceSession;
use serde_json::json;
fn owner() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    }
}
fn apply(store: &mut Store, rows: Vec<Value>) {
    store
        .ingest_batch(&json!({"ok":true,"files":rows}), &rows, &owner())
        .unwrap();
}
fn canvas() -> Value {
    json!({"id":"F1","mode":"quip","filetype":"quip","title":"Canvas","created":1})
}
fn hosted() -> Value {
    json!({"id":"F1","mode":"hosted","filetype":"pdf","title":"PDF","created":1})
}
#[test]
fn hosted_exclusion_is_separate_from_unavailable_title_and_candidate() {
    let mut store = Store::new();
    apply(&mut store, vec![hosted()]);
    assert!(store.row("F1").is_err());
    assert!(store.candidate("F1").is_err());
    let category = store.category("F1").unwrap();
    assert_eq!(category.exclusion(), Some(Exclusion::NonNative));
    assert!(category.current(&store));
    let collection = store.candidates();
    assert!(collection.rows().is_empty());
    assert!(collection.unavailable_ids().is_empty());
    assert_eq!(collection.excluded_ids().collect::<Vec<_>>(), ["F1"]);
    apply(&mut store, vec![json!({"id":"F1","title":"Partial title"})]);
    assert!(!category.current(&store));
    assert!(!collection.current(&store));
    let after = store.category("F1").unwrap();
    assert_eq!(after.effective()["mode"], "hosted");
    assert_eq!(after.effective()["filetype"], "pdf");
    assert_eq!(after.source(), &json!({"id":"F1","title":"Partial title"}));
    assert_eq!(after.exclusion(), Some(Exclusion::NonNative));
}
#[test]
fn both_inherited_discriminants_prevent_false_exclusion() {
    for update in [
        json!({"id":"F1","mode":"hosted"}),
        json!({"id":"F1","filetype":"pdf"}),
    ] {
        let mut store = Store::new();
        apply(&mut store, vec![canvas()]);
        apply(&mut store, vec![update]);
        let category = store.category("F1").unwrap();
        assert_eq!(category.exclusion(), None);
        assert!(store.candidates().excluded().is_empty());
    }
    let mut store = Store::new();
    apply(&mut store, vec![canvas()]);
    apply(&mut store, vec![hosted()]);
    assert_eq!(
        store.category("F1").unwrap().exclusion(),
        Some(Exclusion::NonNative)
    );
}
#[test]
fn duplicate_rows_project_category_from_one_prebatch_snapshot() {
    let mut store = Store::new();
    apply(&mut store, vec![canvas()]);
    apply(
        &mut store,
        vec![hosted(), json!({"id":"F1","mode":"hosted"})],
    );
    let final_row = store.category("F1").unwrap();
    assert_eq!(final_row.effective()["filetype"], "quip");
    assert_eq!(final_row.exclusion(), None);
    assert_eq!(store.candidates().unavailable_ids(), ["F1"]);
    let mut other = Store::new();
    apply(&mut other, vec![canvas()]);
    apply(
        &mut other,
        vec![json!({"id":"F1","mode":"hosted"}), hosted()],
    );
    assert_eq!(
        other.category("F1").unwrap().exclusion(),
        Some(Exclusion::NonNative)
    );
}
#[test]
fn not_visible_passthrough_drops_predecessor_discriminants_without_title_authority() {
    for field in ["mode", "file_access"] {
        for status in ["not_visible", "template_not_visible"] {
            let mut store = Store::new();
            apply(&mut store, vec![canvas()]);
            let mut incoming = json!({"id":"F1","opaque":{"preserve":true}});
            incoming[field] = json!(status);
            apply(&mut store, vec![incoming.clone()]);
            let category = store.category("F1").unwrap();
            assert_eq!(category.exclusion(), Some(Exclusion::NotVisible));
            assert!(category.effective().get("filetype").is_none());
            assert_eq!(category.source(), &incoming);
            assert!(store.row("F1").is_err());
            assert_eq!(
                store.candidates().excluded_ids().collect::<Vec<_>>(),
                ["F1"]
            );
        }
    }
}
#[test]
fn poison_or_unknown_history_never_becomes_a_fresh_hosted_exclusion() {
    for patch in [
        json!({"restriction_type":1}),
        json!({"is_tombstoned":true}),
        json!({"canvas_template_mode":"published"}),
        json!({"list_template_mode":"draft"}),
        json!({"isUnknown":true}),
        json!({"isNonExistent":true}),
        json!({"mode":"unobserved-status"}),
    ] {
        let mut store = Store::new();
        apply(&mut store, vec![canvas()]);
        let mut incoming = patch;
        incoming["id"] = json!("F1");
        apply(&mut store, vec![incoming]);
        apply(&mut store, vec![hosted()]);
        assert!(store.category("F1").is_err());
        assert_eq!(store.candidates().unavailable_ids(), ["F1"]);
        // A later explicit passthrough can prove exclusion, not repaired history.
        apply(
            &mut store,
            vec![json!({"id":"F1","file_access":"not_visible"})],
        );
        assert_eq!(
            store.category("F1").unwrap().exclusion(),
            Some(Exclusion::NotVisible)
        );
        assert!(store.row("F1").is_err());
    }
}
#[test]
fn id_only_and_malformed_category_values_remain_unresolved() {
    for incoming in [
        json!({"id":"F1"}),
        json!({"id":"F1","mode":7}),
        json!({"id":"F1","filetype":false}),
    ] {
        let mut store = Store::new();
        apply(&mut store, vec![incoming]);
        assert_eq!(store.candidates().unavailable_ids(), ["F1"]);
        assert!(store.candidates().excluded().is_empty());
    }
}
#[test]
fn poll_is_nonnative_unless_canvas_discriminant_wins() {
    let mut store = Store::new();
    apply(
        &mut store,
        vec![json!({"id":"F1","mode":"list","filetype":"list","subtype":"poll","created":1})],
    );
    assert_eq!(
        store.category("F1").unwrap().exclusion(),
        Some(Exclusion::NonNative)
    );
    apply(&mut store, vec![json!({"id":"F1","filetype":"quip"})]);
    assert_eq!(store.category("F1").unwrap().exclusion(), None);
}

#[test]
fn original_category_projection_cases_match_effective_discriminants() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    assert_eq!(oracle["status"], "PASS");
    assert_eq!(oracle["count"], 12);
    for case in oracle["tests"].as_array().unwrap() {
        let mut store = Store::new();
        for step in case["steps"].as_array().unwrap() {
            apply(&mut store, step["rows"].as_array().unwrap().clone());
            for expected in step["effective"].as_array().unwrap() {
                let observed = store.category(expected["id"].as_str().unwrap()).unwrap();
                assert_eq!(observed.effective(), expected, "case {}", case["name"]);
            }
        }
    }
}
