use super::*;
#[test]
fn exact_manager_cases_do_not_admit_other_settings() {
    for name in [
        "managers-success",
        "managers-combined",
        "managers-add-rejected",
        "managers-add-unknown",
        "managers-self-success",
        "managers-self-rejected-held",
        "managers-cold",
        "managers-retirement",
        "managers-no-trigger",
    ] {
        assert!(supported(name));
    }
    for name in [
        "",
        "managers",
        "permissions-success",
        "run-success",
        "managers-success-foreign",
        "delete-success",
    ] {
        assert!(!supported(name));
    }
}
#[test]
fn manager_ack_preserves_entire_original_workflow_and_opaque_data() {
    let workflow = json!({"id":"WF_REVIEW","app_id":"A_HOME","collaborators":["UWORKFLOW",raw::NEW],"steps":[],"unknown":{"keep":[1,"original"]}});
    let ack = raw::ack(&workflow);
    assert_eq!(ack["decorated_workflow"], workflow);
    assert_eq!(ack["available_data"], raw::available());
    assert_eq!(ack["expression_data"], raw::expression());
    assert_eq!(ack["opaque_receipt"], json!({"preserve":[3,"manager"]}));
}
#[test]
fn query_rows_preserve_actual_local_and_excluded_shapes() {
    let rows = raw::query_rows();
    assert_eq!(rows.len(), 10);
    let row = |id: &str| rows.iter().find(|r| r["id"] == id).unwrap();
    assert_eq!(row("UMANENTER")["team_id"], "TOTHER");
    assert_eq!(
        row("UMANENTER")["enterprise_user"]["teams"],
        json!(["TWORKFLOW"])
    );
    assert_eq!(row("UMANLOCAL")["is_stranger"], true);
    assert_eq!(row("UMANLOCAL")["team_id"], "TWORKFLOW");
    assert_eq!(row("UMANPROF1")["is_profile_only_user"], true);
    assert!(rows.iter().all(|r| r["id"] != raw::COLD));
}
#[test]
fn collaborator_factory_compares_full_request_not_only_method() {
    let s = crate::app_home::about::management::tests::shell();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let build = || {
        h.owner.ctx.client.rest_form(
            &h.owner.ctx.session,
            "functions.workflows.collaborators.add",
            vec![
                ("workflow_id", "WF_REVIEW".into()),
                ("collaborator_ids", raw::NEW.into()),
                ("_x_reason", "add-hermes-workflow-collaborators".into()),
            ],
        )
    };
    let expected = build();
    let fresh = build();
    assert!(wire::same_request(&fresh, &expected).is_ok());
    for key in ["workflow_id", "collaborator_ids", "_x_reason"] {
        let mut bad = fresh.clone();
        let RequestBody::Form(fields) = &mut bad.body else {
            panic!("form")
        };
        fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = "foreign".into();
        assert!(wire::same_request(&bad, &expected).is_err());
    }
    let mut bad = fresh;
    bad.url.push_str("&fp=00000000000000000000000000000000");
    assert!(wire::same_request(&bad, &expected).is_err());
}
