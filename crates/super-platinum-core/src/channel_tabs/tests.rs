use super::*;
use serde_json::json;
const C: &str = "C00000001";
const A: &str = "Ct00000001";
const B: &str = "Ct00000002";
fn raw() -> Value {
    json!({"id":C,"is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"properties":{"who_can_manage_tabs":"everyone","opaque":{"x":[1,2]},"tabz":[{"id":A,"type":"canvas","label":"Plan","data":{"file_id":"F00000001","access_level":"write","unknown":7},"future":true},{"id":B,"type":"future_resource","label":"Unknown","data":{"arbitrary":[1,"x"]}}]}})
}
fn inv(v: &Value) -> Inventory {
    Inventory::from_channel(v).unwrap()
}
fn request_fields(intent: &Intent) -> (String, Vec<(String, String)>, bool) {
    use crate::slack::{SlackClient, client::RequestBody};
    let w = crate::config::WorkspaceSession {
        team_id: "T00000001".into(),
        enterprise_id: None,
        user_id: "U00000001".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let r = intent.request(&SlackClient::default(), &w);
    let safe = r.retry_safe();
    let RequestBody::Form(form) = r.body else {
        panic!("not form")
    };
    (r.url, form, safe)
}
#[test]
fn unknown_records_and_metadata_are_losslessly_retained() {
    let v = raw();
    let inventory = inv(&v);
    assert_eq!(
        inventory.rows(),
        v["properties"]["tabz"].as_array().unwrap()
    );
    assert_eq!(inventory.properties(), &v["properties"]);
    assert_eq!(inventory.canvas(B), Err(Fault::Unsupported));
}
#[test]
fn rename_changes_only_label_and_requires_exact_other_siblings() {
    let v = raw();
    let i = Intent::capture(
        inv(&v),
        Action::Rename {
            id: A.into(),
            label: "Release".into(),
        },
    )
    .unwrap();
    let mut after = v.clone();
    after["properties"]["tabz"][0]["label"] = "Release".into();
    assert_eq!(i.observe(&inv(&after)), Observation::Satisfied);
    after["properties"]["tabz"][1]["data"]["arbitrary"] = json!([2]);
    assert_eq!(i.observe(&inv(&after)), Observation::Changed);
    assert_eq!(i.revalidate(&inv(&after)), Err(Fault::Changed));
}
#[test]
fn move_uses_id_anchor_and_preserves_all_unknown_bytes() {
    let v = raw();
    let i = Intent::capture(
        inv(&v),
        Action::Move {
            id: A.into(),
            after: Some(B.into()),
        },
    )
    .unwrap();
    let mut after = v.clone();
    after["properties"]["tabz"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert_eq!(i.observe(&inv(&after)), Observation::Satisfied);
    let (url, form, safe) = request_fields(&i);
    assert!(url.contains("/conversations.moveTab?"));
    assert!(!safe);
    assert_eq!(form.iter().find(|(k, _)| k == "after").unwrap().1, B);
    assert!(
        form.iter()
            .all(|(k, _)| !matches!(k.as_str(), "tabz" | "reason" | "channel"))
    );
}
#[test]
fn remove_keeps_file_share_and_never_rewrites_other_properties() {
    let v = raw();
    let i = Intent::capture(inv(&v), Action::Remove { id: A.into() }).unwrap();
    let mut after = v.clone();
    after["properties"]["tabz"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert_eq!(i.observe(&inv(&after)), Observation::Satisfied);
    let (url, form, safe) = request_fields(&i);
    assert!(url.contains("/conversations.removeTab?"));
    assert!(!safe);
    assert!(form.contains(&("keep_file_share".into(), "true".into())));
    assert!(form.iter().all(|(k, _)| k != "file"));
}
#[test]
fn add_records_exact_snake_case_data_and_positive_ordered_membership() {
    let v = raw();
    let i = Intent::capture(
        inv(&v),
        Action::AddCanvas {
            file: "F00000002".into(),
            label: String::new(),
            access: Access::Read,
            after: None,
        },
    )
    .unwrap();
    let (url, form, safe) = request_fields(&i);
    assert!(url.contains("/conversations.addTab?"));
    assert!(!safe);
    assert_eq!(
        serde_json::from_str::<Value>(&form.iter().find(|(k, _)| k == "data").unwrap().1).unwrap(),
        json!({"file_id":"F00000002","access_level":"read"})
    );
    let mut after = v.clone();
    after["properties"]["tabz"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"Ct00000003","type":"canvas","label":"","data":{"file_id":"F00000002"}}));
    assert_eq!(i.observe(&inv(&after)), Observation::Satisfied);
    after["properties"]["tabz"]
        .as_array_mut()
        .unwrap()
        .swap(0, 2);
    assert_eq!(i.observe(&inv(&after)), Observation::Changed);
}
#[test]
fn absent_inventory_can_add_without_fabricated_prior_tabs() {
    let v = json!({"id":C});
    let i = Intent::capture(
        inv(&v),
        Action::AddCanvas {
            file: "F00000001".into(),
            label: "Canvas".into(),
            access: Access::Read,
            after: None,
        },
    )
    .unwrap();
    let after = json!({"id":C,"properties":{"tabz":[{"id":A,"type":"canvas","label":"Canvas","data":{"file_id":"F00000001"}}]}});
    assert_eq!(i.observe(&inv(&after)), Observation::Satisfied);
}
#[test]
fn unchanged_unknown_outcome_and_wrong_channel_never_prove_success() {
    let v = raw();
    let i = Intent::capture(inv(&v), Action::Remove { id: A.into() }).unwrap();
    assert_eq!(i.observe(&inv(&v)), Observation::Unchanged);
    let mut foreign = v;
    foreign["id"] = "C00000002".into();
    foreign["properties"]["tabz"] = json!([]);
    assert_eq!(i.observe(&inv(&foreign)), Observation::Changed);
}
#[test]
fn duplicated_ids_and_automatic_canvas_refuse_before_request() {
    let mut v = raw();
    v["properties"]["tabz"][1]["id"] = A.into();
    assert_eq!(Inventory::from_channel(&v), Err(Fault::Shape));
    let mut v = raw();
    v["properties"]["channel_knowledge_canvas"] = json!({"file_id":"F00000001","is_enabled":true});
    assert_eq!(
        Intent::capture(inv(&v), Action::Remove { id: A.into() }),
        Err(Fault::Unsupported)
    );
}
#[test]
fn invalid_anchors_and_duplicate_file_addition_are_refused() {
    let b = inv(&raw());
    assert!(
        Intent::capture(
            b.clone(),
            Action::Move {
                id: A.into(),
                after: Some(A.into())
            }
        )
        .is_err()
    );
    assert!(
        Intent::capture(
            b.clone(),
            Action::Move {
                id: A.into(),
                after: Some("Ct00000099".into())
            }
        )
        .is_err()
    );
    assert!(
        Intent::capture(
            b,
            Action::AddCanvas {
                file: "F00000001".into(),
                label: "Again".into(),
                access: Access::Read,
                after: None
            }
        )
        .is_err()
    );
}
#[test]
fn permission_uses_observed_posting_and_never_synthetic_manager_property() {
    use crate::slack::{file_share_policy::PostingFacts, models::User};
    let mut v = raw();
    let member = User {
        id: "U00000001".into(),
        ..Default::default()
    };
    let mut facts = PostingFacts::new(C.into());
    assert_eq!(
        permission(&v, &member, &facts, false, Some("paid")),
        Permission::Unavailable
    );
    facts.observe(&v, Some(&member));
    assert_eq!(
        permission(&v, &member, &facts, true, Some("paid")),
        Permission::Allowed
    );
    v["properties"]["who_can_manage_tabs"] = "managers".into();
    v["channel_managers"] = json!([member.id]);
    assert_eq!(
        permission(&v, &member, &facts, false, Some("paid")),
        Permission::Unavailable
    );
    v["properties"]
        .as_object_mut()
        .unwrap()
        .remove("who_can_manage_tabs");
    assert_eq!(
        permission(&v, &member, &facts, true, Some("paid")),
        Permission::Unavailable
    );
    assert_eq!(
        permission(&v, &member, &facts, false, Some("paid")),
        Permission::Allowed
    );
    v["is_member"] = false.into();
    assert_ne!(
        permission(&v, &member, &facts, false, Some("paid")),
        Permission::Allowed
    );
}
#[test]
fn system_tabs_do_not_spend_the_custom_fifteen_limit() {
    let mut v = raw();
    let rows = v["properties"]["tabz"].as_array_mut().unwrap();
    for n in 3..=15 {
        rows.push(
            json!({"id":format!("Ct{n:08}"),"type":"canvas","data":{"file_id":format!("F{n:08}")}}),
        );
    }
    rows.push(json!({"type":"files"}));
    assert_eq!(rows.len(), 16);
    assert_eq!(
        Intent::capture(
            inv(&v),
            Action::AddCanvas {
                file: "F00000099".into(),
                label: String::new(),
                access: Access::Read,
                after: None
            }
        ),
        Err(Fault::Limit)
    );
    v["properties"]["tabz"].as_array_mut().unwrap().remove(0);
    assert!(
        Intent::capture(
            inv(&v),
            Action::AddCanvas {
                file: "F00000099".into(),
                label: String::new(),
                access: Access::Read,
                after: None
            }
        )
        .is_ok()
    );
}

#[test]
fn unchanged_actions_and_static_wire_anchors_are_not_dispatched() {
    let base = inv(&raw());
    assert_eq!(
        Intent::capture(
            base.clone(),
            Action::Rename {
                id: A.into(),
                label: "Plan".into()
            }
        ),
        Err(Fault::Changed)
    );
    assert_eq!(
        Intent::capture(
            base.clone(),
            Action::Move {
                id: A.into(),
                after: None
            }
        ),
        Err(Fault::Changed)
    );
    let mut v = raw();
    v["properties"]["tabz"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"channel"}));
    assert_eq!(
        Intent::capture(
            inv(&v),
            Action::Move {
                id: A.into(),
                after: Some("channel".into())
            }
        ),
        Err(Fault::Unsupported)
    );
}

#[test]
fn null_id_uses_exact_type_identity_without_coercing_malformed_non_null_ids() {
    let mut v = raw();
    v["properties"]["tabz"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":null,"type":"files","opaque":[1,2]}));
    let before = inv(&v);
    assert_eq!(id(&before.rows()[2]), Some("files"));
    let action = Intent::capture(
        before,
        Action::Rename {
            id: A.into(),
            label: "New label".into(),
        },
    )
    .unwrap();
    let mut after = v.clone();
    after["properties"]["tabz"][0]["label"] = "New label".into();
    assert_eq!(action.observe(&inv(&after)), Observation::Satisfied);
    assert_eq!(after["properties"]["tabz"][2], v["properties"]["tabz"][2]);
    for malformed in [json!(false), json!(7), json!(""), json!([])] {
        let mut changed = v.clone();
        changed["properties"]["tabz"][2]["id"] = malformed;
        assert!(Inventory::from_channel(&changed).is_err());
    }
    v["properties"]["tabz"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"files"}));
    assert!(Inventory::from_channel(&v).is_err());
}
