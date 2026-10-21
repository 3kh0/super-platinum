use super::*;
#[test]
fn delete_allowlist_excludes_old_metadata_cases_and_foreign_suffixes() {
    for value in [
        "delete-success",
        "delete-first-rejected",
        "delete-first-unknown",
        "delete-second-unknown",
        "delete-preflight-retirement",
        "delete-mutation-retirement",
    ] {
        assert!(supported(value));
    }
    for value in [
        "",
        "editor",
        "managed",
        "reply-loss",
        "delete-success-foreign",
        "delete-malformed",
    ] {
        assert!(!supported(value));
    }
}
#[test]
fn deletion_wire_accepts_original_core_plans_and_refuses_mutated_identity_credential_or_method() {
    let mut s = crate::app_home::about::management::tests::shell();
    let editor = crate::app_home::about::management::tests::editor(&mut s);
    let plan = MutationPlan::new(&editor, protocol::Command::Delete).unwrap();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    for slot in plan.slots() {
        let request = slot
            .plan()
            .request(&h.owner.ctx.client, &h.owner.ctx.session);
        let (method, fields) = super::super::decode(&request).unwrap();
        assert!(wire::validate_mutation(&s, &request, method, fields).is_ok());
        for n in 0..5 {
            let mut bad = request.clone();
            if n == 0 {
                bad.method = "GET";
            } else {
                let RequestBody::Form(fields) = &mut bad.body else {
                    panic!("form")
                };
                match n {
                    1 => fields.push(("extra".into(), "true".into())),
                    2 => fields.push(fields[0].clone()),
                    3 => {
                        fields.iter_mut().find(|(k, _)| k == "token").unwrap().1 = "foreign".into()
                    }
                    _ => {
                        fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 =
                            "foreign".into()
                    }
                }
            }
            assert!(wire::validate_mutation(&s, &bad, method, fields).is_err());
        }
    }
}
#[test]
fn raw_receipt_keeps_transport_loss_distinct_from_rejection_and_ack() {
    assert_eq!(
        receipt(&Ok(json!({"ok":true}))),
        json!({"raw":{"ok":true},"error":null})
    );
    assert_eq!(
        receipt(&Ok(json!({"ok":false,"error":"not_allowed"}))),
        json!({"raw":{"ok":false,"error":"not_allowed"},"error":null})
    );
    assert_eq!(
        receipt(&Err(Error::Transport("synthetic".into()))),
        json!({"raw":null,"error":"transport"})
    );
}
