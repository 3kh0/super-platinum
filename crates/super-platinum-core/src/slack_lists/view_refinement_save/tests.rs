use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, client::RequestBody},
};
fn document() -> Document {
    Document::parse(json!({"id":"F1","mode":"list","created":1,"user":"U1","access":"write","list_limits":{"over_view_maximum":false},"list_metadata":{
        "opaque_metadata":[null,true,7],"schema":[
        {"id":"C_NAME","key":"name","name":"Name","type":"text"},
        {"id":"C_STATUS","key":"status","name":"Status","type":"select","options":{"format":"single_select","choices":[{"value":"A","label":"Alpha"},{"value":"B","label":"Beta"}]}}],
        "views":[{"id":"V1","name":"Delivery","type":"board","position":"0000000001","columns":[{"id":"C_NAME","key":"name","visible":true,"width":240,"should_wrap_text":true,"position":"0000000001","opaque":7},{"id":"C_STATUS","key":"status","visible":false,"position":"0000000002"}],"grouping":{"group_by":"status","opaque":{"keep":true}},"filters":[],"sorts":[],"opaque_view":5},
        {"id":"V2","name":"Planning","type":"table","position":"0000000002","opaque_sibling":[1,2]}]}})).unwrap()
}
fn personal() -> PersonalTaskLists {
    PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
    )
    .unwrap()
}
fn changed(doc: &Document) -> ViewRefinementDraft {
    ViewRefinementDraft::capture(doc, "V1")
        .unwrap()
        .with_completed(doc, true)
        .unwrap()
}
fn capture(
    doc: &Document,
    draft: &ViewRefinementDraft,
    destination: ViewRefinementDestination,
) -> ViewRefinementSave {
    ViewRefinementSave::capture(
        doc,
        draft,
        None,
        destination,
        Some("full"),
        Some(&personal()),
    )
    .unwrap()
}
fn fresh(intent: &ViewRefinementSave) -> Document {
    let mut raw = intent.document.raw.clone();
    raw["list_metadata"] = intent.expected_metadata.clone().unwrap();
    Document::parse(raw).unwrap()
}
#[test]
fn order_only_never_authorizes_save_but_dirty_save_merges_the_typed_provider() {
    let doc = document();
    let draft = ViewRefinementDraft::capture(&doc, "V1").unwrap();
    let provider = super::super::BoardLaneOrderDraft::moved(
        &doc,
        "V1",
        None,
        "B",
        "A",
        super::super::BoardLanePlacement::Before,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    assert!(
        ViewRefinementSave::capture(
            &doc,
            &draft,
            Some(&provider),
            ViewRefinementDestination::Existing,
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
    let intent = ViewRefinementSave::capture(
        &doc,
        &changed(&doc),
        Some(&provider),
        ViewRefinementDestination::Existing,
        Some("full"),
        Some(&personal()),
    )
    .unwrap();
    let after = fresh(&intent);
    assert_eq!(
        after.views[0]["grouping"]["order"],
        json!(provider.typed_order())
    );
    assert_eq!(
        after.views[0]["grouping"]["opaque"],
        doc.views[0]["grouping"]["opaque"]
    );
    assert_eq!(after.views[1], doc.views[1]);
    assert_eq!(
        after.raw["list_metadata"]["schema"],
        doc.raw["list_metadata"]["schema"]
    );
    assert_eq!(
        after.raw["list_metadata"]["opaque_metadata"],
        doc.raw["list_metadata"]["opaque_metadata"]
    );
    assert_eq!(intent.provider(), Some(&provider));
}
#[test]
fn visibility_wire_contains_only_overrides_and_observation_retains_column_metadata() {
    let doc = document();
    let draft = changed(&doc)
        .with_visibility(&doc, "C_NAME", false)
        .unwrap();
    let intent = capture(&doc, &draft, ViewRefinementDestination::Existing);
    let wire: Value = serde_json::from_str(
        &intent
            .fields
            .iter()
            .find(|(k, _)| k == "columns")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(wire, json!([{"key":"name","id":"C_NAME","visible":false}]));
    let after = fresh(&intent);
    let mut expected = doc.views[0]["columns"].clone();
    expected[0]["visible"] = json!(false);
    assert_eq!(after.views[0]["columns"], expected);
    assert_eq!(intent.observed_view_id(&after), Some("V1"));
    let mut corrupt = after.raw.clone();
    corrupt["list_metadata"]["views"][0]["columns"][0]["opaque"] = json!(8);
    assert!(
        intent
            .observed_view_id(&Document::parse(corrupt).unwrap())
            .is_none()
    );
}
#[test]
fn actual_permit_builder_has_exact_update_field_order_and_never_retries() {
    let doc = document();
    let intent = capture(&doc, &changed(&doc), ViewRefinementDestination::Existing);
    let after = fresh(&intent);
    let mut attempt = ViewRefinementSaveAttempt::new(intent);
    assert!(attempt.observe(&after).is_none());
    let permit = attempt
        .admit(&doc, Some("full"), Some(&personal()))
        .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.slack.com".into(),
        token: "fixture-token".into(),
    };
    let request = api::list_view_refinement_save(&SlackClient::default(), &session, permit);
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(
        fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        vec![
            "token",
            "list_id",
            "id",
            "type",
            "filters",
            "info_column_filters",
            "sorts",
            "grouping",
            "show_completed_items",
            "_x_reason"
        ]
    );
    assert!(attempt.finish(false));
    assert_eq!(attempt.phase(), ViewRefinementSavePhase::Unconfirmed);
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&personal()))
            .is_err()
    );
    assert!(attempt.observe(&doc).is_none());
    assert_eq!(attempt.observe(&after), Some("V1".into()));
    assert!(attempt.observe(&after).is_none());
}
#[test]
fn source_creation_copies_only_selected_column_members_and_requires_isolated_addition() {
    let doc = document();
    let draft = changed(&doc)
        .with_visibility(&doc, "C_STATUS", true)
        .unwrap();
    let intent = capture(
        &doc,
        &draft,
        ViewRefinementDestination::New {
            name: " Delivery ðŸ—‚ ".into(),
        },
    );
    assert_eq!(
        intent
            .fields
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        vec![
            "list_id",
            "name",
            "type",
            "grouping",
            "columns",
            "show_completed_items",
            "_x_reason"
        ]
    );
    assert_eq!(
        intent.expected_view["columns"][0],
        json!({"id":"C_NAME","width":240,"should_wrap_text":true,"visible":true,"position":"0000000001"})
    );
    let mut raw = doc.raw.clone();
    let mut added = intent.expected_view.clone();
    added["id"] = json!("SERVER_NEW");
    added["position"] = json!("0000000003");
    added["columns"][0]["key"] = json!("name");
    added["columns"][1]["key"] = json!("status");
    added["opaque_server"] = json!({"keep":true});
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .push(added);
    let after = Document::parse(raw.clone()).unwrap();
    assert_eq!(intent.observed_view_id(&after), Some("SERVER_NEW"));
    raw["list_metadata"]["views"][0]["opaque_view"] = json!(6);
    assert!(
        intent
            .observed_view_id(&Document::parse(raw).unwrap())
            .is_none()
    );
    for (key, value) in [
        (
            "info_column_filters",
            json!([{"key":"created_by","values":["U_OTHER"]}]),
        ),
        ("filter_set", json!({"operator":"or","filters":[]})),
    ] {
        let mut unexpected = after.raw.clone();
        unexpected["list_metadata"]["views"][2][key] = value;
        assert!(
            intent
                .observed_view_id(&Document::parse(unexpected).unwrap())
                .is_none()
        );
    }
    let mut wrong = after.raw.clone();
    wrong["list_metadata"]["views"][2]["columns"][0]["key"] = json!("status");
    assert!(
        intent
            .observed_view_id(&Document::parse(wrong).unwrap())
            .is_none()
    );
}
#[test]
fn unsupported_filters_and_foreign_column_ids_refuse_before_permit() {
    let doc = document();
    let row = json!({"key":"status","operator":"contains","values":["A"],"column_id":"C_STATUS"});
    let draft = changed(&doc).with_filters(&doc, vec![row.clone()]).unwrap();
    let accepted = capture(&doc, &draft, ViewRefinementDestination::Existing);
    assert_eq!(
        serde_json::from_str::<Value>(
            &accepted
                .fields
                .iter()
                .find(|(k, _)| k == "filters")
                .unwrap()
                .1
        )
        .unwrap(),
        json!([row])
    );
    for filter in [
        json!({"key":"status","operator":"contains","values":["A"],"column_id":"FOREIGN"}),
        json!({"key":"status","operator":"contains","values":["A"],"typed_values":["A"]}),
        json!({"key":"status","operator":"contains","values":["A"],"unknown":true}),
    ] {
        let mut raw = document().raw;
        raw["list_metadata"]["views"][0]["filters"] = json!([filter]);
        let doc = Document::parse(raw).unwrap();
        assert!(
            ViewRefinementSave::capture(
                &doc,
                &changed(&doc),
                None,
                ViewRefinementDestination::Existing,
                Some("full"),
                Some(&personal())
            )
            .is_err()
        );
    }
    let mut raw = document().raw;
    raw["list_metadata"]["views"][0]["filter_set"] = json!({"operator":"or","filters":[]});
    let doc = Document::parse(raw).unwrap();
    assert!(
        ViewRefinementSave::capture(
            &doc,
            &changed(&doc),
            None,
            ViewRefinementDestination::Existing,
            Some("full"),
            Some(&personal())
        )
        .is_err()
    );
}
#[test]
fn freshness_policy_and_not_requested_are_nonreplayable() {
    let doc = document();
    let intent = capture(&doc, &changed(&doc), ViewRefinementDestination::Existing);
    let mut attempt = ViewRefinementSaveAttempt::new(intent.clone());
    assert!(attempt.admit(&doc, Some("full"), None).is_err());
    let mut raw = doc.raw.clone();
    raw["list_metadata"]["opaque_metadata"] = json!([7]);
    let changed = Document::parse(raw).unwrap();
    assert!(
        attempt
            .admit(&changed, Some("full"), Some(&personal()))
            .is_err()
    );
    let _permit = attempt
        .admit(&doc, Some("full"), Some(&personal()))
        .unwrap();
    assert!(attempt.finish(true));
    assert_eq!(attempt.phase(), ViewRefinementSavePhase::NotRequested);
    assert!(
        attempt
            .admit(&doc, Some("full"), Some(&personal()))
            .is_err()
    );
    assert!(attempt.observe(&fresh(&intent)).is_none());
}
