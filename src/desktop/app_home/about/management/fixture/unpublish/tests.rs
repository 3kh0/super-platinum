use super::*;
#[test]
fn exact_unpublish_cases_do_not_admit_metadata_delete_or_suffix_variants() {
    for value in [
        "unpublish-success",
        "unpublish-primary-rejected",
        "unpublish-primary-unknown",
        "unpublish-sync-rejected",
        "unpublish-sync-unknown",
        "unpublish-retirement",
    ] {
        assert!(supported(value));
    }
    for value in [
        "",
        "editor",
        "delete-success",
        "unpublish",
        "unpublish-success-foreign",
        "unpublish-malformed",
    ] {
        assert!(!supported(value));
    }
}
#[test]
fn actual_trigger_projection_retains_opaque_truthy_mapping_and_derives_changed_keys() {
    let mut s = crate::app_home::about::management::tests::shell();
    let m = s.app_home.about.management.fixture.as_mut().unwrap();
    raw::seed(&mut m.workflow, &mut m.trigger);
    let original = json!({"workflow":m.workflow,"trigger":m.trigger});
    let new = raw::unpublished(&original["workflow"]);
    let request = protocol::TriggerUpdate::synchronize(&new, &original["trigger"])
        .unwrap()
        .unwrap();
    let expected = raw::synchronized(&original["trigger"]);
    assert_eq!(request.method(), "workflows.triggers.update");
    assert_eq!(request.reason(), "publish-workflow");
    assert_eq!(request.arguments()["inputs"], expected["inputs"]);
    assert_eq!(
        request.arguments()["inputs"]["keep"],
        original["trigger"]["inputs"]["keep"]
    );
    assert!(request.arguments()["inputs"].get("removed").is_none());
    assert_eq!(new["title"], original["workflow"]["title"]);
    assert_eq!(new["retained_unknown"], json!(42));
    assert_eq!(raw::primary(&new)["expression_data"], raw::expression());
    assert_eq!(raw::sync(&expected)["trigger"], expected);
}
#[test]
fn exact_mutation_wire_refuses_credential_identity_reason_and_duplicate_corruption() {
    let s = crate::app_home::about::management::tests::shell();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let expected = h.owner.ctx.client.rest_form(
        &h.owner.ctx.session,
        "functions.workflows.unpublish",
        vec![
            ("workflow_id", "WF_REVIEW".into()),
            ("_x_reason", "unpublish-workflow".into()),
        ],
    );
    let fresh = h.owner.ctx.client.rest_form(
        &h.owner.ctx.session,
        "functions.workflows.unpublish",
        vec![
            ("workflow_id", "WF_REVIEW".into()),
            ("_x_reason", "unpublish-workflow".into()),
        ],
    );
    assert!(wire::same_request(&fresh, &expected).is_ok());
    for kind in 0..6 {
        let mut bad = fresh.clone();
        if kind == 0 {
            bad.method = "GET";
        } else if kind == 1 {
            bad.url.push_str("&fp=00000000000000000000000000000000");
        } else {
            let RequestBody::Form(fields) = &mut bad.body else {
                panic!("form")
            };
            match kind {
                2 => fields.push(fields[0].clone()),
                3 => fields.iter_mut().find(|(k, _)| k == "token").unwrap().1 = "foreign".into(),
                4 => {
                    fields
                        .iter_mut()
                        .find(|(k, _)| k == "workflow_id")
                        .unwrap()
                        .1 = "WF_FOREIGN".into()
                }
                _ => {
                    fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 =
                        "publish-workflow".into()
                }
            }
        }
        assert!(wire::same_request(&bad, &expected).is_err());
    }
}
#[test]
fn transport_loss_does_not_invent_acknowledgement_or_erase_original_raw() {
    let raw = json!({"ok":false,"error":"not_allowed","opaque":[1,true]});
    assert_eq!(receipt(&Ok(raw.clone())), json!({"raw":raw,"error":null}));
    assert_eq!(
        receipt(&Err(Error::Transport(
            "synthetic applied reply lost".into()
        ))),
        json!({"raw":null,"error":"transport"})
    );
}
