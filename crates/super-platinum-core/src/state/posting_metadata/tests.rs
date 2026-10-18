use super::*;
use crate::config::WorkspaceSession;
use crate::slack::client::{RequestBody, SlackClient};
use serde_json::json;

fn session() -> Session {
    Session {
        d_cookie: "fixture-only".into(),
        unified_user_client_enabled: BTreeMap::from([("T1".into(), true)]),
        workspaces: BTreeMap::from([(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: Some("E1".into()),
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-only".into(),
            },
        )]),
    }
}
fn boot() -> BootData {
    serde_json::from_value(json!({
        "team":{"id":"T1","plan":"plus","enterprise_id":"E1","token":"never-retained"},
        "self":{"id":"U1","team_id":"T1","enterprise_id":"E1","is_admin":true,
            "enterprise_user":{"id":"U1","enterprise_id":"E1","is_admin":false,"teams":["T1","T2"]},
            "token":"never-retained","profile":{"real_name":"private"}},
        "account_types":{"is_admin":["T1"],"is_owner":["T2"],"is_primary_owner":[]},
        "workspaces":[{"id":"T1","enterprise_id":"E1"},{"id":"T2","enterprise_id":"E1"},
            {"id":"T3","enterprise_id":"E2","allow_external_users":true}],
        "read_only_channels":["C1"],"thread_only_channels":[],"token":"never-retained"
    }))
    .unwrap()
}
fn metadata() -> (Metadata, Configuration) {
    (
        Metadata::from_boot("T1", "U1", &boot()),
        Configuration::from_session(&session(), "T1").unwrap(),
    )
}
fn local(team: &str) -> Value {
    json!({"id":"C1","context_team_id":team,"is_general":true})
}

#[test]
fn missing_role_query_returns_only_unresolved_relevant_modern_targets_without_mutation() {
    let (mut metadata, config) = metadata();
    metadata.workspace_admin.insert("T1".into(), false);
    metadata.workspace_admin.remove("T2");
    let shared = json!({"id":"C1","is_org_shared":true,"internal_team_ids":["T1","T2","T3","T2"]});
    assert_eq!(
        metadata.missing_workspace_roles(&config, &shared),
        Ok(vec!["T2".into()])
    );
    let pending = metadata
        .begin_workspace_role_read(&config, &shared, "T1")
        .unwrap();
    let before = format!("{metadata:?}");
    assert_eq!(
        metadata.missing_workspace_roles(&config, &shared),
        Ok(vec!["T1".into(), "T2".into()])
    );
    assert_eq!(format!("{metadata:?}"), before);
    assert!(pending.current(&metadata, &config));
    assert!(
        pending
            .apply(&mut metadata, &config, &json!({"ok":false}))
            .is_err()
    );
    assert_eq!(metadata.workspace_admin.get("T1"), Some(&false));
    assert_eq!(
        metadata.missing_workspace_roles(&config, &shared),
        Ok(vec!["T1".into(), "T2".into()])
    );
    let retry = metadata
        .begin_workspace_role_read(&config, &shared, "T1")
        .unwrap();
    retry
        .apply(
            &mut metadata,
            &config,
            &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":false}}}),
        )
        .unwrap();
    assert_eq!(
        metadata.missing_workspace_roles(&config, &shared),
        Ok(vec!["T2".into()])
    );
    let read = metadata
        .begin_workspace_role_read(&config, &shared, "T2")
        .unwrap();
    read.apply(
        &mut metadata,
        &config,
        &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":true}}}),
    )
    .unwrap();
    assert_eq!(metadata.channel_admin(&config, &shared), Ok(true));
    assert_eq!(
        metadata.missing_workspace_roles(&config, &shared),
        Ok(Vec::new())
    );
}

#[test]
fn missing_role_query_preserves_resolved_results_and_unsupported_modes() {
    let (mut metadata, mut config) = metadata();
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T1")),
        Ok(Vec::new())
    );
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T2")),
        Ok(Vec::new())
    );
    metadata.workspace_admin.remove("T2");
    metadata.member["enterprise_user"]["is_admin"] = json!(true);
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T2")),
        Ok(Vec::new())
    );
    metadata.member["enterprise_user"]["is_admin"] = json!(false);
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T9")),
        Err(Unknown::Membership)
    );
    config.unified = Some(false);
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T2")),
        Err(Unknown::Role)
    );
    config.unified = None;
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T2")),
        Err(Unknown::Configuration)
    );
    config.unified = Some(true);
    config.enterprise = None;
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T2")),
        Err(Unknown::Role)
    );
}

#[test]
fn missing_role_query_does_not_refresh_known_role_to_cover_unknown_enterprise_role() {
    let (mut metadata, config) = metadata();
    metadata
        .member
        .as_object_mut()
        .unwrap()
        .remove("enterprise_user");
    assert_eq!(
        metadata.channel_admin(&config, &local("T2")),
        Err(Unknown::Role)
    );
    assert_eq!(
        metadata.missing_workspace_roles(&config, &local("T2")),
        Err(Unknown::Role)
    );
    assert_eq!(metadata.workspace_admin.get("T2"), Some(&false));
    assert!(metadata.reads.is_empty());
}

#[test]
fn complete_boot_retains_only_policy_fields_and_workspace_apply_is_wired() {
    let (metadata, config) = metadata();
    let text = format!("{metadata:?}");
    assert!(!text.contains("never-retained"));
    assert!(!text.contains("private"));
    assert_eq!(
        metadata.context(&config, &local("T1")).unwrap().member["is_admin"],
        true
    );
    assert!(metadata.effective(&config, &local("T1")).unwrap().read_only);
    let ordinary = json!({"id":"C2","context_team_id":"T1"});
    assert!(
        metadata
            .effective(&config, &ordinary)
            .unwrap()
            .permits_root_post()
    );
    let mut workspace = crate::state::Workspace::from_session(&session().workspaces["T1"]);
    assert!(
        workspace
            .posting_metadata
            .context(&config, &ordinary)
            .is_err()
    );
    workspace.apply_boot(boot());
    assert!(
        workspace
            .posting_metadata
            .effective(&config, &ordinary)
            .unwrap()
            .permits_root_post()
    );
}

#[test]
fn missing_or_malformed_inventories_do_not_supply_false() {
    for raw in [None, Some(json!(null)), Some(json!(["C1", 2]))] {
        let mut boot = boot();
        boot.extra.remove("read_only_channels");
        if let Some(raw) = raw {
            boot.extra.insert("read_only_channels".into(), raw);
        }
        let metadata = Metadata::from_boot("T1", "U1", &boot);
        let config = Configuration::from_session(&session(), "T1").unwrap();
        assert_eq!(
            metadata.effective(&config, &local("T1")),
            Err(Unknown::Policy(posting_policy::Unknown::EffectiveFlags))
        );
    }
}

#[test]
fn configuration_uses_any_retained_true_and_preserves_unknown_filtered_grid() {
    let mut source = session();
    source.unified_user_client_enabled.clear();
    assert_eq!(
        Configuration::from_session(&source, "T1")
            .unwrap()
            .unified(),
        None
    );
    source
        .unified_user_client_enabled
        .insert("T1".into(), false);
    assert_eq!(
        Configuration::from_session(&source, "T1")
            .unwrap()
            .unified(),
        Some(false)
    );
    let mut second = source.workspaces["T1"].clone();
    second.team_id = "E1".into();
    source.workspaces.insert("E1".into(), second);
    source
        .unified_user_client_enabled
        .insert("E1".into(), false);
    assert_eq!(
        Configuration::from_session(&source, "T1")
            .unwrap()
            .unified(),
        None
    );
    source.unified_user_client_enabled.insert("E1".into(), true);
    assert_eq!(
        Configuration::from_session(&source, "T1")
            .unwrap()
            .unified(),
        Some(true)
    );
}

#[test]
fn role_union_is_not_a_complete_negative_inventory_or_owner_implies_admin() {
    let (metadata, config) = metadata();
    assert_eq!(metadata.channel_admin(&config, &local("T1")), Ok(true));
    assert_eq!(metadata.channel_admin(&config, &local("T2")), Ok(false));
    assert_eq!(
        metadata.channel_admin(&config, &local("T4")),
        Err(Unknown::Role)
    );
    // The raw self is_admin=true may not grant another Grid workspace.
    assert_eq!(metadata.member["is_admin"], true);
    assert_eq!(
        metadata.channel_admin(&config, &local("T3")),
        Err(Unknown::Role)
    );
}

#[test]
fn relevant_workspace_selection_uses_internal_membership_and_no_fallback() {
    let (metadata, config) = metadata();
    let global = json!({"id":"C1","is_org_shared":true,"is_global_shared":true});
    assert_eq!(
        metadata.relevant_workspaces(&config, &global).unwrap(),
        vec!["T1", "T2"]
    );
    let shared = json!({"id":"C1","is_org_shared":true,"internal_team_ids":["T2","T3"]});
    assert_eq!(
        metadata.relevant_workspaces(&config, &shared).unwrap(),
        vec!["T2"]
    );
    let empty = json!({"id":"C1","is_org_shared":true,"internal_team_ids":["T3"]});
    assert_eq!(metadata.channel_admin(&config, &empty), Ok(false));
    assert_eq!(
        metadata.relevant_workspaces(&config, &local("E1")),
        Err(Unknown::Membership)
    );
    let mut missing = boot();
    missing.extra.remove("workspaces");
    assert_eq!(
        Metadata::from_boot("T1", "U1", &missing).relevant_workspaces(&config, &global),
        Err(Unknown::Membership)
    );
}

#[test]
fn enterprise_admin_is_scoped_to_actual_workspace_enterprise() {
    let mut boot = boot();
    boot.self_user.extra.get_mut("enterprise_user").unwrap()["is_admin"] = json!(true);
    let mut metadata = Metadata::from_boot("T1", "U1", &boot);
    let config = Configuration::from_session(&session(), "T1").unwrap();
    assert_eq!(metadata.channel_admin(&config, &local("T2")), Ok(true));
    metadata.workspace_admin.insert("T3".into(), false);
    assert_eq!(metadata.channel_admin(&config, &local("T3")), Ok(false));
    assert_eq!(
        metadata.channel_admin(&config, &local("T4")),
        Err(Unknown::Role)
    );
}

#[test]
fn raw_channel_ingress_preserves_omission_retires_malformed_and_binds_config() {
    let (mut metadata, config) = metadata();
    metadata
        .observe_channel(
            &config,
            &json!({"id":"C1","is_read_only":false,"is_thread_only":true}),
        )
        .unwrap();
    metadata
        .observe_channel(&config, &json!({"id":"C1"}))
        .unwrap();
    assert!(
        metadata
            .effective(&config, &local("T1"))
            .unwrap()
            .thread_only
    );
    assert!(
        metadata
            .observe_channel(&config, &json!({"id":"C1","is_read_only":"bad"}))
            .is_err()
    );
    assert!(metadata.effective(&config, &local("T1")).is_err());
    metadata
        .observe_channel(
            &config,
            &json!({"id":"C1","is_read_only":false,"is_thread_only":false}),
        )
        .unwrap();
    let mut changed = config.clone();
    changed.unified = Some(false);
    assert!(metadata.effective(&changed, &local("T1")).is_err());
}

#[test]
fn role_preferences_use_contextual_admin_not_global_self_flag() {
    let (mut metadata, config) = metadata();
    let mut channel = local("T2");
    channel["properties"] = json!({"posting_restricted_to":{"role":["admin"]},"threads_restricted_to":{"role":["admin"]}});
    metadata.observe_channel(&config, &channel).unwrap();
    assert!(metadata.effective(&config, &channel).unwrap().read_only);
    channel["context_team_id"] = json!("T1");
    metadata.observe_channel(&config, &channel).unwrap();
    assert!(
        metadata
            .effective(&config, &channel)
            .unwrap()
            .permits_root_post()
    );
}

#[test]
fn feature_read_is_one_use_boot_owned_and_shape_checked() {
    let (mut metadata, config) = metadata();
    let old = metadata.begin_feature_read(&config).unwrap();
    let newer = metadata.begin_feature_read(&config).unwrap();
    let response = json!({"ok":true,"feature_policies":{"manage_channel_posting__access":{"is_enabled":false}}});
    assert_eq!(
        old.apply(&mut metadata, &config, &response),
        Err(Unknown::Stale)
    );
    let clone = newer.clone();
    newer.apply(&mut metadata, &config, &response).unwrap();
    assert_eq!(metadata.manage_posting, Some(false));
    assert_eq!(
        clone.apply(&mut metadata, &config, &response),
        Err(Unknown::Stale)
    );
    let pending = metadata.begin_feature_read(&config).unwrap();
    let mut fresh = Metadata::from_boot("T1", "U1", &boot());
    assert_eq!(
        pending.clone().apply(&mut fresh, &config, &response),
        Err(Unknown::Stale)
    );
    assert_eq!(
        pending.apply(
            &mut metadata,
            &config,
            &json!({"ok":true,"feature_policies":{}})
        ),
        Err(Unknown::Shape)
    );
    assert_eq!(metadata.manage_posting, None);
}

#[test]
fn prepared_reads_are_exact_and_refuse_foreign_credentials() {
    let (mut metadata, config) = metadata();
    let session = session();
    let workspace = &session.workspaces["T1"];
    let client = SlackClient::default();
    let read = metadata
        .begin_workspace_role_read(&config, &local("T2"), "T2")
        .unwrap();
    let request = read
        .request(&metadata, &config, &client, workspace)
        .unwrap();
    assert!(request.url.contains("/api/users.info?"));
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-only".into()),
            ("self".into(), "true".into()),
            ("team".into(), "T2".into())
        ]
    );
    let mut wrong = workspace.clone();
    wrong.user_id = "U2".into();
    assert!(matches!(
        read.request(&metadata, &config, &client, &wrong),
        Err(Unknown::Identity)
    ));
    assert!(
        metadata
            .begin_workspace_role_read(&config, &local("T1"), "T2")
            .is_err()
    );
    let read = metadata.begin_feature_read(&config).unwrap();
    assert!(
        read.request(&metadata, &config, &client, workspace)
            .unwrap()
            .url
            .contains("/api/features.access.policies.list?")
    );
}

#[test]
fn workspace_read_rejects_foreign_user_team_and_missing_team_user() {
    let (mut metadata, config) = metadata();
    for response in [
        json!({"ok":true,"user":{"id":"U2","team_user":{"is_admin":true}}}),
        json!({"ok":true,"user":{"id":"U1","team_user":{"team_id":"T3","is_admin":true}}}),
        json!({"ok":true,"user":{"id":"U1"}}),
    ] {
        let read = metadata
            .begin_workspace_role_read(&config, &local("T2"), "T2")
            .unwrap();
        assert!(read.apply(&mut metadata, &config, &response).is_err());
        assert_eq!(
            metadata.channel_admin(&config, &local("T2")),
            Err(Unknown::Role)
        );
    }
    let read = metadata
        .begin_workspace_role_read(&config, &local("T2"), "T2")
        .unwrap();
    read.apply(
        &mut metadata,
        &config,
        &json!({"ok":true,"user":{"id":"U1","team_user":{"team_id":"T2","is_admin":true}}}),
    )
    .unwrap();
    assert_eq!(metadata.channel_admin(&config, &local("T2")), Ok(true));
}

#[test]
fn foreign_and_empty_boot_cannot_acquire_policy_context() {
    let (_, config) = metadata();
    for boot in [BootData::default(), {
        let mut b = boot();
        b.self_user.id = "U2".into();
        b
    }] {
        assert!(
            Metadata::from_boot("T1", "U1", &boot)
                .context(&config, &local("T1"))
                .is_err()
        );
    }
}

#[test]
fn warm_cache_does_not_restore_posting_authority() {
    let session = session();
    let config = Configuration::from_session(&session, "T1").unwrap();
    let credentials = &session.workspaces["T1"];
    let mut workspace = crate::state::Workspace::from_session(credentials);
    workspace.apply_boot(boot());
    assert!(
        workspace
            .posting_metadata
            .context(&config, &local("T1"))
            .is_ok()
    );
    let cache = crate::cache::Cache::open(":memory:").unwrap();
    cache.save_workspace(&workspace).unwrap();
    let restored = cache
        .load_workspace_for_startup(credentials)
        .unwrap()
        .unwrap();
    assert!(
        restored
            .posting_metadata
            .context(&config, &local("T1"))
            .is_err()
    );
}

#[test]
fn role_refresh_retires_local_grants_and_invalidation_retires_reads() {
    let (mut metadata, config) = metadata();
    let channel = json!({"id":"C2","context_team_id":"T2"});
    assert!(
        metadata
            .effective(&config, &channel)
            .unwrap()
            .permits_root_post()
    );
    metadata
        .begin_workspace_role_read(&config, &channel, "T2")
        .unwrap();
    // Fetching an initially missing role is not evidence that server-provided
    // complete inventories changed. Ordinary omitted flags remain usable.
    assert!(
        metadata
            .effective(&config, &channel)
            .unwrap()
            .permits_root_post()
    );
    let read = metadata.begin_feature_read(&config).unwrap();
    metadata.invalidate();
    assert!(!read.current(&metadata, &config));
}

#[test]
fn unknown_role_acquisition_does_not_force_an_ordinary_channel_boot_loop() {
    let mut boot = boot();
    boot.extra
        .insert("account_types".into(), json!({"is_admin":["T1"]}));
    let config = Configuration::from_session(&session(), "T1").unwrap();
    let mut metadata = Metadata::from_boot("T1", "U1", &boot);
    let channel = json!({"id":"C2","context_team_id":"T2"});
    for _ in 0..2 {
        let read = metadata
            .begin_workspace_role_read(&config, &channel, "T2")
            .unwrap();
        assert_eq!(
            metadata.channel_admin(&config, &channel),
            Err(Unknown::Role)
        );
        assert!(
            metadata
                .effective(&config, &channel)
                .unwrap()
                .permits_root_post()
        );
        read.apply(
            &mut metadata,
            &config,
            &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":false}}}),
        )
        .unwrap();
        metadata.observe_channel(&config, &channel).unwrap();
        assert_eq!(metadata.channel_admin(&config, &channel), Ok(false));
        assert!(
            metadata
                .effective(&config, &channel)
                .unwrap()
                .permits_root_post()
        );
    }
}

#[test]
fn role_read_retires_each_derived_companion_without_erasing_server_origin() {
    for server_read in [true, false] {
        let (mut metadata, config) = metadata();
        let mut channel = local("T1");
        channel["properties"] = json!({"posting_restricted_to":{"role":["admin"]},"threads_restricted_to":{"role":["admin"]}});
        metadata.observe_channel(&config, &channel).unwrap();
        // Source gives either direct effective companion precedence over prefs.
        let partial = if server_read {
            json!({"id":"C1","is_read_only":false})
        } else {
            json!({"id":"C1","is_thread_only":false})
        };
        metadata.observe_channel(&config, &partial).unwrap();
        let read = metadata
            .begin_workspace_role_read(&config, &local("T1"), "T1")
            .unwrap();
        let expected = if server_read {
            (Some(false), None)
        } else {
            (None, Some(false))
        };
        assert_eq!(metadata.observation(&config, "C1").values(), expected);
        assert!(metadata.effective(&config, &channel).is_err());
        assert_eq!(
            read.apply(
                &mut metadata,
                &config,
                &json!({"ok":false,"error":"ratelimited"})
            ),
            Err(Unknown::Shape)
        );
        assert_eq!(metadata.observation(&config, "C1").values(), expected);
        assert_eq!(
            metadata.channel_admin(&config, &local("T1")),
            Err(Unknown::Role)
        );
    }
}

#[test]
fn two_separate_server_companions_replace_both_local_dependencies() {
    let (mut metadata, config) = metadata();
    metadata.observe_channel(&config, &json!({"id":"C1","context_team_id":"T1","properties":{"posting_restricted_to":{"role":["admin"]}}})).unwrap();
    metadata
        .observe_channel(&config, &json!({"id":"C1","is_read_only":false}))
        .unwrap();
    metadata
        .observe_channel(&config, &json!({"id":"C1","is_thread_only":false}))
        .unwrap();
    metadata
        .begin_workspace_role_read(&config, &local("T1"), "T1")
        .unwrap();
    assert_eq!(
        metadata.observation(&config, "C1").values(),
        (Some(false), Some(false))
    );
}

#[test]
fn known_role_change_retires_server_flags_even_after_read_supersession() {
    let (mut metadata, config) = metadata();
    let ordinary = json!({"id":"C2","context_team_id":"T1"});
    let old = metadata
        .begin_workspace_role_read(&config, &ordinary, "T1")
        .unwrap();
    let latest = metadata
        .begin_workspace_role_read(&config, &ordinary, "T1")
        .unwrap();
    let response = json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":false}}});
    assert_eq!(
        old.apply(&mut metadata, &config, &response),
        Err(Unknown::Stale)
    );
    assert!(
        metadata
            .effective(&config, &ordinary)
            .unwrap()
            .permits_root_post()
    );
    latest.apply(&mut metadata, &config, &response).unwrap();
    assert!(metadata.effective(&config, &ordinary).is_err());
    assert_eq!(metadata.channel_admin(&config, &ordinary), Ok(false));
}

#[test]
fn known_role_baseline_survives_failed_completed_reads_before_retry_revocation() {
    for failure in [
        json!({"ok":false,"error":"ratelimited"}),
        json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":"bad"}}}),
        json!({"ok":true,"user":{"id":"U2","team_user":{"is_admin":false}}}),
        json!({"ok":true,"user":{"id":"U1","team_user":{"team_id":"T2","is_admin":false}}}),
        json!({"ok":true,"user":{"id":"U1"}}),
    ] {
        let (mut metadata, config) = metadata();
        let ordinary = json!({"id":"C2","context_team_id":"T1"});
        let mut derived = local("T1");
        derived["properties"] = json!({"posting_restricted_to":{"role":["admin"]},"threads_restricted_to":{"role":["admin"]}});
        metadata.observe_channel(&config, &derived).unwrap();
        assert!(
            metadata
                .effective(&config, &derived)
                .unwrap()
                .permits_root_post()
        );
        for _ in 0..2 {
            let read = metadata
                .begin_workspace_role_read(&config, &ordinary, "T1")
                .unwrap();
            assert!(read.apply(&mut metadata, &config, &failure).is_err());
            assert_eq!(metadata.workspace_admin.get("T1"), Some(&true));
            assert_eq!(
                metadata.channel_admin(&config, &ordinary),
                Err(Unknown::Role)
            );
            assert!(metadata.effective(&config, &derived).is_err());
        }
        assert!(
            metadata
                .effective(&config, &ordinary)
                .unwrap()
                .permits_root_post()
        );
        let read = metadata
            .begin_workspace_role_read(&config, &ordinary, "T1")
            .unwrap();
        read.apply(
            &mut metadata,
            &config,
            &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":false}}}),
        )
        .unwrap();
        assert_eq!(metadata.channel_admin(&config, &ordinary), Ok(false));
        assert!(!metadata.invalid_workspace_roles.contains("T1"));
        assert!(metadata.effective(&config, &ordinary).is_err());
        assert!(metadata.effective(&config, &derived).is_err());
    }
}

#[test]
fn failed_initial_role_acquisition_stays_unknown_then_recovers_without_erasing_server_flags() {
    let mut boot = boot();
    boot.extra
        .insert("account_types".into(), json!({"is_admin":["T1"]}));
    let config = Configuration::from_session(&session(), "T1").unwrap();
    let mut metadata = Metadata::from_boot("T1", "U1", &boot);
    let ordinary = json!({"id":"C2","context_team_id":"T2"});
    let failed = metadata
        .begin_workspace_role_read(&config, &ordinary, "T2")
        .unwrap();
    assert!(
        failed
            .apply(&mut metadata, &config, &json!({"ok":false}))
            .is_err()
    );
    assert_eq!(metadata.workspace_admin.get("T2"), None);
    assert_eq!(
        metadata.channel_admin(&config, &ordinary),
        Err(Unknown::Role)
    );
    let read = metadata
        .begin_workspace_role_read(&config, &ordinary, "T2")
        .unwrap();
    read.apply(
        &mut metadata,
        &config,
        &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":false}}}),
    )
    .unwrap();
    assert_eq!(metadata.channel_admin(&config, &ordinary), Ok(false));
    assert!(
        metadata
            .effective(&config, &ordinary)
            .unwrap()
            .permits_root_post()
    );
    assert!(!metadata.invalid_workspace_roles.contains("T2"));
    metadata.invalidate();
    assert!(metadata.workspace_admin.is_empty());
    assert!(metadata.invalid_workspace_roles.is_empty());
}

#[test]
fn local_boot_role_precedence_is_applied_once_and_later_reads_can_revoke_it() {
    let mut boot = boot();
    boot.self_user.extra.remove("enterprise_id");
    boot.self_user.extra.insert("is_admin".into(), json!(false));
    let config = Configuration::from_session(&session(), "T1").unwrap();
    let mut metadata = Metadata::from_boot("T1", "U1", &boot);
    // kzmv local self false replaces account_types.is_admin's T1 true.
    assert_eq!(metadata.channel_admin(&config, &local("T1")), Ok(false));
    for admin in [true, false] {
        let read = metadata
            .begin_workspace_role_read(&config, &local("T1"), "T1")
            .unwrap();
        assert_eq!(
            metadata.channel_admin(&config, &local("T1")),
            Err(Unknown::Role)
        );
        read.apply(
            &mut metadata,
            &config,
            &json!({"ok":true,"user":{"id":"U1","team_user":{"team_id":"T1","is_admin":admin}}}),
        )
        .unwrap();
        assert_eq!(metadata.channel_admin(&config, &local("T1")), Ok(admin));
    }
    boot.self_user.extra.insert("is_admin".into(), json!(true));
    let mut metadata = Metadata::from_boot("T1", "U1", &boot);
    assert_eq!(metadata.channel_admin(&config, &local("T1")), Ok(true));
    let read = metadata
        .begin_workspace_role_read(&config, &local("T1"), "T1")
        .unwrap();
    read.apply(
        &mut metadata,
        &config,
        &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":false}}}),
    )
    .unwrap();
    assert_eq!(metadata.channel_admin(&config, &local("T1")), Ok(false));
}
