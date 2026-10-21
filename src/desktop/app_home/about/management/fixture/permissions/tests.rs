use super::*;
#[test]
fn exact_permissions_cases_do_not_admit_metadata_delete_or_suffix_variants() {
    for value in [
        "permissions-success",
        "permissions-rejected",
        "permissions-unknown",
        "permissions-retirement",
    ] {
        assert!(supported(value));
    }
    for value in [
        "",
        "editor",
        "delete-success",
        "permissions",
        "permissions-success-foreign",
        "permissions-malformed",
    ] {
        assert!(!supported(value));
    }
}
#[test]
fn exact_mutation_wire_refuses_credential_identity_reason_and_duplicate_corruption() {
    let s = crate::app_home::about::management::tests::shell();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let expected = h.owner.ctx.client.rest_form(
        &h.owner.ctx.session,
        "functions.workflows.permissions.copy.set",
        vec![
            ("workflow_id", "WF_REVIEW".into()),
            ("permission_type", "everyone".into()),
            ("_x_reason", "set-copy-permission".into()),
        ],
    );
    let fresh = h.owner.ctx.client.rest_form(
        &h.owner.ctx.session,
        "functions.workflows.permissions.copy.set",
        vec![
            ("workflow_id", "WF_REVIEW".into()),
            ("permission_type", "everyone".into()),
            ("_x_reason", "set-copy-permission".into()),
        ],
    );
    assert!(wire::same_request(&fresh, &expected).is_ok());
    for kind in 0..7 {
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
                5 => {
                    fields
                        .iter_mut()
                        .find(|(k, _)| k == "permission_type")
                        .unwrap()
                        .1 = "collaborators".into();
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
