use super::*;
use crate::slack_lists::{ListDefaultViewPreference, ListDefaultViewPreferenceAttempt};
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

fn reply(intent: &ViewRefinementSave) -> Value {
    let mut raw = intent.document.raw.clone();
    let mut added = intent.expected_view.clone();
    added["id"] = json!("V_ACK_LAST");
    raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .push(added);
    json!({"ok":true,"list":raw})
}
fn admitted(destination: ViewRefinementDestination) -> ViewRefinementSaveAttempt {
    let doc = document();
    let intent = capture(&doc, &changed(&doc), destination);
    let mut attempt = ViewRefinementSaveAttempt::new(intent);
    let _consumed_by_worker = attempt
        .admit(&doc, Some("full"), Some(&personal()))
        .unwrap();
    attempt
}
#[test]
fn positive_new_ack_can_start_independent_preference_before_metadata_observation() {
    let mut a = admitted(ViewRefinementDestination::New {
        name: "Created".into(),
    });
    let mut ack = reply(a.intent());
    // This response cannot pass strict metadata observation, yet source child
    // dispatch uses its last ID. No ACK metadata is published by this handoff.
    ack["list"]["list_metadata"]["opaque_metadata"] = json!(["different"]);
    let returned = Document::parse(ack["list"].clone()).unwrap();
    assert!(a.intent().observed_view_id(&returned).is_none());
    let child = a.acknowledge(&ack).unwrap();
    assert_eq!(child.list_id(), "F1");
    assert_eq!(child.view_id(), "V_ACK_LAST");
    assert_eq!(a.phase(), ViewRefinementSavePhase::Unconfirmed);
    assert!(a.acknowledge(&ack).is_none());
    let before = json!({"ok":true,"prefs":{"lists_default_view":"{\"F_OTHER\":\"V_OTHER\"}"}});
    let preference = ListDefaultViewPreference::capture_acknowledged(&before, child).unwrap();
    assert_eq!(
        preference.expected(),
        &json!({"F_OTHER":"V_OTHER","F1":"V_ACK_LAST"})
    );
    assert!(preference.revalidate(&before, &document()).is_ok());
    assert!(!preference.observes_desired(&ack));
    let mut child_attempt = ListDefaultViewPreferenceAttempt::new(preference);
    let _permit = child_attempt.admit(&before, &document()).unwrap();
    assert!(child_attempt.finish(false));
    assert!(!child_attempt.observe(&before));
    assert_eq!(a.phase(), ViewRefinementSavePhase::Unconfirmed);
}
#[test]
fn source_existing_ack_triggers_child_without_requiring_reply_metadata() {
    let mut a = admitted(ViewRefinementDestination::Existing);
    let child = a.acknowledge(&json!({"ok":true})).unwrap();
    assert_eq!(child.view_id(), "V1");
    let p = ListDefaultViewPreference::capture_acknowledged(
        &json!({"ok":true,"prefs":{"lists_default_view":""}}),
        child,
    )
    .unwrap();
    assert_eq!(p.expected(), &json!({"F1":"V1"}));
    assert_eq!(a.phase(), ViewRefinementSavePhase::Unconfirmed);
}
#[test]
fn no_admission_lost_reply_or_pre_dispatch_failure_cannot_mint_child() {
    let doc = document();
    let mut a = ViewRefinementSaveAttempt::new(capture(
        &doc,
        &changed(&doc),
        ViewRefinementDestination::Existing,
    ));
    assert!(a.acknowledge(&json!({"ok":true})).is_none());
    assert_eq!(a.phase(), ViewRefinementSavePhase::Reviewing);
    for not_requested in [false, true] {
        let mut a = admitted(ViewRefinementDestination::Existing);
        assert!(a.finish(not_requested));
        assert!(a.acknowledge(&json!({"ok":true})).is_none());
    }
}
#[test]
fn malformed_or_foreign_last_new_identity_never_dispatches_preference() {
    for case in 0..7 {
        let mut a = admitted(ViewRefinementDestination::New {
            name: "Created".into(),
        });
        let mut ack = reply(a.intent());
        match case {
            0 => ack["ok"] = json!(false),
            1 => ack["error"] = json!("failed"),
            2 => ack["list"]["id"] = json!("F_OTHER"),
            3 => {
                ack["list"]["list_metadata"]["views"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            4 => ack["list"]["list_metadata"]["views"][2]["type"] = json!("record"),
            5 => ack["list"]["list_metadata"]["views"][2]["name"] = json!("Different"),
            _ => ack["list"]["list_metadata"]["views"] = json!([]),
        }
        assert!(a.acknowledge(&ack).is_none());
        assert_eq!(a.phase(), ViewRefinementSavePhase::Unconfirmed);
        assert!(a.acknowledge(&reply(a.intent())).is_none());
    }
}
