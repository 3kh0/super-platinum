use super::*;
use serde_json::json;
fn inputs() -> (Inventory, WorkspaceSession) {
    let raw = json!({"id":"C00000001","properties":{"opaque":{"keep":[3,1,2]},"channel_knowledge_canvas":{"file_id":"F00000001","is_enabled":true,"date_last_generated":123,"future":"keep"},"tabz":[{"id":null,"type":"files","opaque":9},{"id":"Ct00000001","type":"canvas","label":"Knowledge","data":{"file_id":"F00000001","unknown":true}},{"id":"Ct00000002","type":"future","data":{"a":[2,1]}}]}});
    (
        Inventory::from_channel(&raw).unwrap(),
        WorkspaceSession {
            team_id: "T00000001".into(),
            enterprise_id: None,
            user_id: "U00000001".into(),
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        },
    )
}
#[test]
fn disable_then_remove_exact_wire_and_no_replay() {
    let (base, w) = inputs();
    let c = SlackClient::default();
    let mut op = KnowledgeRemoval::capture(base.clone(), "Ct00000001").unwrap();
    let req = op.begin_disable(&base, &c, &w).unwrap();
    assert_eq!(
        url::Url::parse(&req.url).unwrap().path(),
        "/api/ai.channelKnowledge.disable"
    );
    assert!(!req.retry_safe());
    let crate::slack::client::RequestBody::Form(form) = req.body else {
        panic!()
    };
    assert_eq!(
        form,
        vec![
            ("token".into(), "synthetic".into()),
            ("channel_id".into(), "C00000001".into())
        ]
    );
    assert!(op.begin_disable(&base, &c, &w).is_err());
    assert!(op.begin_remove(&base, &c, &w).is_err());
    let disabled = op.disabled();
    assert_eq!(op.observe(&disabled), KnowledgeStage::Disabled);
    let req = op.begin_remove(&disabled, &c, &w).unwrap();
    assert!(!req.retry_safe());
    assert_eq!(
        url::Url::parse(&req.url).unwrap().path(),
        "/api/conversations.removeTab"
    );
    let crate::slack::client::RequestBody::Form(form) = req.body else {
        panic!()
    };
    assert_eq!(
        form,
        vec![
            ("token".into(), "synthetic".into()),
            ("channel_id".into(), "C00000001".into()),
            ("id".into(), "Ct00000001".into())
        ]
    );
    assert!(op.begin_remove(&disabled, &c, &w).is_err());
    let removed = op.removed();
    assert_eq!(op.observe(&removed), KnowledgeStage::Complete);
    assert_eq!(
        removed.rows(),
        &[base.rows()[0].clone(), base.rows()[2].clone()]
    );
    assert_eq!(removed.properties()["opaque"], base.properties()["opaque"]);
    assert_eq!(
        removed.properties()["channel_knowledge_canvas"]["date_last_generated"],
        123
    );
}
#[test]
fn unchanged_or_missing_metadata_never_proves_disabled_or_allows_retry() {
    let (base, w) = inputs();
    let mut op = KnowledgeRemoval::capture(base.clone(), "Ct00000001").unwrap();
    op.begin_disable(&base, &SlackClient::default(), &w)
        .unwrap();
    assert_eq!(op.observe(&base), KnowledgeStage::DisableUnconfirmed);
    let mut missing = base.clone();
    missing
        .properties
        .as_object_mut()
        .unwrap()
        .remove("channel_knowledge_canvas");
    assert_eq!(op.observe(&missing), KnowledgeStage::DisableUnconfirmed);
    assert!(
        op.begin_disable(&base, &SlackClient::default(), &w)
            .is_err()
    );
}
#[test]
fn unrelated_sibling_or_metadata_changes_refuse_each_stage() {
    let (base, w) = inputs();
    let c = SlackClient::default();
    let mut op = KnowledgeRemoval::capture(base.clone(), "Ct00000001").unwrap();
    let mut changed = base.clone();
    changed.properties["opaque"] = json!({"different":true});
    assert!(op.begin_disable(&changed, &c, &w).is_err());
    assert_eq!(op.stage(), KnowledgeStage::Ready);
    op.begin_disable(&base, &c, &w).unwrap();
    let mut changed = op.disabled();
    changed.rows[2]["data"]["a"] = json!([1, 2]);
    changed.properties["tabz"] = json!(changed.rows);
    assert_eq!(op.observe(&changed), KnowledgeStage::DisableUnconfirmed);
    let disabled = op.disabled();
    op.observe(&disabled);
    assert!(op.begin_remove(&changed, &c, &w).is_err());
    op.begin_remove(&disabled, &c, &w).unwrap();
    let mut wrong = op.removed();
    wrong.properties["opaque"] = Value::Null;
    assert_eq!(op.observe(&wrong), KnowledgeStage::RemoveUnconfirmed);
}
#[test]
fn knowledge_canvas_is_readable_but_ordinary_intent_cannot_skip_disable() {
    let (base, _) = inputs();
    assert!(base.canvas_record("Ct00000001").is_ok());
    assert!(base.canvas("Ct00000001").is_err());
    assert!(
        Intent::capture(
            base.clone(),
            Action::Remove {
                id: "Ct00000001".into()
            }
        )
        .is_err()
    );
    for enabled in [None, Some(Value::Null), Some(json!(true))] {
        let mut b = base.clone();
        match enabled {
            Some(v) => b.properties["channel_knowledge_canvas"]["is_enabled"] = v,
            None => {
                b.properties["channel_knowledge_canvas"]
                    .as_object_mut()
                    .unwrap()
                    .remove("is_enabled");
            }
        }
        assert!(KnowledgeRemoval::capture(b, "Ct00000001").is_ok());
    }
    for enabled in [json!(false), json!(0), json!("true")] {
        let mut b = base.clone();
        b.properties["channel_knowledge_canvas"]["is_enabled"] = enabled;
        assert!(KnowledgeRemoval::capture(b, "Ct00000001").is_err());
    }
}
