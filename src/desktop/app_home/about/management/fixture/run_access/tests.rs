use super::*;
#[test]
fn exact_run_cases_refuse_copy_and_mixed_names() {
    for name in [
        "run-success",
        "run-named-combined",
        "run-rejected",
        "run-unknown",
        "run-followup-failure",
        "run-retirement",
    ] {
        assert!(supported(name));
    }
    for name in [
        "",
        "permissions-success",
        "run",
        "run-success-foreign",
        "delete-success",
        "unpublish-success",
    ] {
        assert!(!supported(name));
    }
}
#[test]
fn named_grants_keep_original_collaborator_and_all_target_categories() {
    let d = raw::desired_for("run-named-combined");
    assert_eq!(d["user_ids"], json!(["UWORKFLOW", "URUNADA01"]));
    assert_eq!(d["channel_ids"], json!(["CRUNTEAM1"]));
    assert_eq!(d["team_ids"], json!(["TRUNINNER", "TRUNEXTER"]));
    assert_eq!(d["org_ids"], json!([]));
    for case in ["run-success", "run-unknown", "run-retirement"] {
        let d = raw::desired_for(case);
        assert_eq!(d["permission_type"], json!("everyone"));
        for key in ["user_ids", "channel_ids", "team_ids", "org_ids"] {
            assert_eq!(d[key], json!([]));
        }
    }
}
#[test]
fn wire_comparison_refuses_target_and_reason_corruption() {
    let s = crate::app_home::about::management::tests::shell();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let build = || {
        h.owner.ctx.client.rest_form(
            &h.owner.ctx.session,
            "workflows.triggers.permissions.set",
            vec![
                ("trigger_id", "TR_REVIEW".into()),
                ("permission_type", "named_entities".into()),
                ("user_ids", "UWORKFLOW,URUNADA01".into()),
                ("_x_reason", "set-trigger-run-permission".into()),
            ],
        )
    };
    let expected = build();
    let fresh = build();
    assert!(wire::same_request(&fresh, &expected).is_ok());
    for key in ["trigger_id", "permission_type", "user_ids", "_x_reason"] {
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
#[test]
fn targets_use_original_raw_identity_and_affiliation() {
    let teams = seed::teams();
    assert_eq!(teams[0]["id"], "TWORKFLOW");
    assert_eq!(teams[0]["enterprise_id"], teams[1]["enterprise_id"]);
    assert_ne!(teams[0]["enterprise_id"], teams[2]["enterprise_id"]);
    let user: super_platinum_core::slack::models::User =
        serde_json::from_value(seed::person()).unwrap();
    assert_eq!(user.id, "URUNADA01");
    let channel: super_platinum_core::slack::models::Channel =
        serde_json::from_value(seed::channel()).unwrap();
    assert_eq!(channel.id, "CRUNTEAM1");
}

#[test]
fn picker_factory_compares_random_trace_fields_but_refuses_payload_or_route_change() {
    use protocol::settings_acquisition::{Read, SearchKind};
    use super_platinum_core::block_actions::directory_endpoint::DirectoryEndpoint;
    let s = crate::app_home::about::management::tests::shell();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let ctx = &h.owner.ctx;
    let now = std::time::Instant::now();
    let endpoint = DirectoryEndpoint::decode(&ctx.session, &picker::discovery_raw(), now).unwrap();
    for kind in [
        SearchKind::Members,
        SearchKind::Channels,
        SearchKind::InternalTeams,
        SearchKind::ExternalTeams,
        SearchKind::AvailableExternal,
    ] {
        let read = Read::new(
            &ctx.session,
            kind,
            if kind == SearchKind::AvailableExternal {
                ""
            } else {
                "run"
            },
            None,
        )
        .unwrap();
        let a = read
            .request(&ctx.client, &ctx.session, Some(&endpoint), now)
            .unwrap();
        let b = read
            .request(&ctx.client, &ctx.session, Some(&endpoint), now)
            .unwrap();
        assert!(wire::same_request(&a, &b).is_ok());
        let mut corrupt = a.clone();
        corrupt.url = corrupt
            .url
            .replace("slack.com", "foreign.test")
            .replace("example.test", "foreign.test");
        assert!(wire::same_request(&corrupt, &b).is_err());
        let mut corrupt = a;
        match &mut corrupt.body {
            RequestBody::Json(v) => v["query"] = json!("foreign"),
            RequestBody::Form(v) => v.push(("foreign".into(), "true".into())),
        }
        assert!(wire::same_request(&corrupt, &b).is_err());
    }
}

#[test]
fn prerequisite_original_grants_do_not_become_ready_seed_rows() {
    let raw = prerequisites::original_for("run-cold-targets").unwrap();
    assert_eq!(raw["user_ids"], json!(["UMISSING01"]));
    assert_ne!(seed::person()["id"], raw["user_ids"][0]);
    assert_ne!(seed::channel()["id"], raw["channel_ids"][0]);
    assert!(seed::teams().iter().all(|t| t["id"] != raw["team_ids"][0]));
    assert!(prerequisites::original_for("run-success").is_none());
    assert_eq!(
        raw::desired_for("run-everyone-external")["team_ids"],
        json!(["TWORKFLOW", "TRUNEXTER"])
    );
    assert_eq!(
        raw::desired_for("run-everyone-external")["user_ids"],
        json!([])
    );
}
#[test]
fn teams_identity_factory_retains_retry_classification_and_omitted_reason() {
    use protocol::settings_acquisition::Read;
    let s = crate::app_home::about::management::tests::shell();
    let ctx = &s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .owner
        .ctx;
    let read = Read::teams_by_ids(&ctx.session, &["TMISSING01".into()], None).unwrap();
    let a = read
        .request(&ctx.client, &ctx.session, None, std::time::Instant::now())
        .unwrap();
    let b = read
        .request(&ctx.client, &ctx.session, None, std::time::Instant::now())
        .unwrap();
    assert!(a.retry_safe());
    assert!(wire::same_request(&a, &b).is_ok());
    let RequestBody::Form(fields) = &a.body else {
        panic!("form")
    };
    assert!(!fields.iter().any(|(k, _)| k == "_x_reason"));
    let mut bad = a.clone();
    let RequestBody::Form(fields) = &mut bad.body else {
        panic!("form")
    };
    fields.push(("_x_reason".into(), "invented".into()));
    assert!(wire::same_request(&bad, &b).is_err());
}
