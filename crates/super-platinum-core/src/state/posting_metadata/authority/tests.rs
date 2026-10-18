use super::*;
use crate::{config::Session, slack::events::RtEvent, state::Workspace};
use serde_json::json;
mod groups;

fn core() -> CoreAppState {
    let credential = WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: Some("E1".into()),
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-token".into(),
    };
    let mut core = CoreAppState::new(Default::default());
    core.active_account = Some("fixture-account".into());
    core.transport = Some(Arc::new(Transport::disconnected_fixture("fixture-cookie")));
    core.workspaces
        .insert("T1".into(), Workspace::from_session(&credential));
    core.session = Some(Session {
        d_cookie: "fixture-cookie".into(),
        unified_user_client_enabled: BTreeMap::from([("T1".into(), true)]),
        workspaces: BTreeMap::from([("T1".into(), credential)]),
    });
    core
}
fn raw_boot() -> Value {
    json!({"ok":true,"team":{"id":"T1","enterprise_id":"E1","plan":"plus"},
        "self":{"id":"U1","team_id":"T1","enterprise_id":"E1"},
        "account_types":{"is_admin":["T1"]},
        "workspaces":[{"id":"T1","enterprise_id":"E1"},{"id":"T2","enterprise_id":"E1"},
            {"id":"T3","allow_external_users":true}],
        "read_only_channels":[],"thread_only_channels":[],
        "channels":[{"id":"C1","context_team_id":"T1","name":"private-fixture-name"}]})
}
fn begin(core: &mut CoreAppState) -> BootRead {
    let transport = core.transport.clone().unwrap();
    let session = core.session.as_ref().unwrap().workspaces["T1"].clone();
    BootRead::begin(core, &transport, &session).unwrap()
}

fn full_read(core: &mut CoreAppState) -> BootRead {
    let transport = core.transport.clone().unwrap();
    let session = core.session.as_ref().unwrap().workspaces["T1"].clone();
    BootRead::begin_full(core, &transport, &session).unwrap()
}

#[test]
fn normal_boot_seeds_raw_channel_history_but_picker_recovery_does_not() {
    let mut core = core();
    let mut raw = raw_boot();
    raw["users"] = json!([{ "id":"U00000001", "name":"actual-wire-name" }]);
    assert!(
        begin(&mut core)
            .apply(&mut core, raw.clone(), false)
            .unwrap()
    );
    assert!(!core.workspaces["T1"].directory_channels.has_pending());
    assert!(core.workspaces["T1"].directory_members.cached().is_none());
    assert!(full_read(&mut core).apply(&mut core, raw, true).unwrap());
    let workspace = &core.workspaces["T1"];
    assert!(workspace.directory_channels.has_pending());
    let cached = workspace.directory_members.cached().unwrap();
    assert!(cached.to_string().contains("actual-wire-name"));
    // A source observation is not an invented fully processed collection.
    let session = &core.session.as_ref().unwrap().workspaces["T1"];
    assert!(
        workspace
            .directory_channels
            .ready(
                "fixture-account",
                session,
                core.account_epoch,
                "Q",
                &workspace.channels
            )
            .is_err()
    );
}

#[test]
fn replacement_owner_cannot_seed_shared_raw_history() {
    let mut core = core();
    let stale = full_read(&mut core);
    core.transport = Some(Arc::new(Transport::disconnected_fixture("fixture-cookie")));
    assert!(!stale.apply(&mut core, raw_boot(), true).unwrap());
    assert!(core.workspaces["T1"].directory_channels.cached().is_none());
    assert!(core.workspaces["T1"].directory_members.cached().is_none());
    let stale = full_read(&mut core);
    core.active_account = Some("replacement-account".into());
    core.account_epoch += 1;
    assert!(!stale.apply(&mut core, raw_boot(), true).unwrap());
    assert!(core.workspaces["T1"].directory_channels.cached().is_none());
}
fn acquired() -> (CoreAppState, Authority) {
    let mut core = core();
    assert!(begin(&mut core).apply(&mut core, raw_boot(), true).unwrap());
    let authority = Authority::capture(&core, "T1").unwrap();
    (core, authority)
}
fn event(value: Value) -> RtEvent {
    RtEvent::Unknown(serde_json::from_value(value).unwrap())
}

#[test]
fn ordinary_ingress_capture_matches_the_actual_request_not_just_current_team() {
    let (mut core, authority) = acquired();
    let transport = core.transport.clone().unwrap();
    let credential = core.session.as_ref().unwrap().workspaces["T1"].clone();
    assert!(Authority::capture_for_request(&core, &transport, &credential).is_ok());
    let other = Transport::disconnected_fixture("fixture-cookie");
    assert!(Authority::capture_for_request(&core, &other, &credential).is_err());
    for field in 0..4 {
        let mut changed = credential.clone();
        match field {
            0 => changed.token.push_str("-different"),
            1 => changed.url.push_str("/different"),
            2 => changed.user_id = "U2".into(),
            _ => changed.enterprise_id = None,
        }
        assert!(Authority::capture_for_request(&core, &transport, &changed).is_err());
    }
    let channel = serde_json::from_value(json!({"id":"C2","connected_team_ids":["T9"]})).unwrap();
    core.account_epoch += 1;
    assert!(
        authority
            .observe_external_channel_connections(&mut core, &[channel])
            .is_err()
    );
}

#[test]
fn joined_socket_arrays_add_history_without_authenticating_a_posting_snapshot() {
    let (mut core, authority) = acquired();
    let before = authority.external_team(&core, "T9").unwrap();
    let joined =
        event(json!({"type":"mpim_joined","channel":{"id":"G9","connected_team_ids":["E1","T9"]}}));
    observe_event(&mut core, "T1", 1, &joined);
    assert!(before.current(&core));
    observe_event(&mut core, "T1", 0, &joined);
    assert!(!before.current(&core));
    assert!(authority.external_team(&core, "T9").unwrap().value());
    assert!(!authority.external_team(&core, "E1").unwrap().value());
    assert!(authority.raw_channel(&core, "G9").is_err());
}

#[test]
fn created_socket_ingress_respects_current_restriction_and_im_exception() {
    let mut core = core();
    let mut boot = raw_boot();
    boot["self"]["is_restricted"] = json!(true);
    begin(&mut core).apply(&mut core, boot, true).unwrap();
    let authority = Authority::capture(&core, "T1").unwrap();
    observe_event(
        &mut core,
        "T1",
        0,
        &event(json!({"type":"channel_created","channel":{"id":"C9","connected_team_ids":["T9"]}})),
    );
    assert!(!authority.external_team(&core, "T9").unwrap().value());
    observe_event(
        &mut core,
        "T1",
        0,
        &event(
            json!({"type":"im_created","channel":{"id":"D9","is_im":true,"connected_team_ids":["T9"]}}),
        ),
    );
    assert!(authority.external_team(&core, "T9").unwrap().value());
    let mut boot = raw_boot();
    boot["self"]["is_restricted"] = json!(false);
    begin(&mut core).apply(&mut core, boot, true).unwrap();
    let authority = Authority::capture(&core, "T1").unwrap();
    observe_event(
        &mut core,
        "T1",
        0,
        &event(json!({"type":"channel_created","channel":{"id":"C8","connected_team_ids":["T8"]}})),
    );
    assert!(authority.external_team(&core, "T8").unwrap().value());
}

#[test]
fn unknown_created_member_role_never_preserves_a_negative_lookup() {
    let (mut core, authority) = acquired();
    let before = authority.external_team(&core, "T9").unwrap();
    observe_event(
        &mut core,
        "T1",
        0,
        &event(json!({"type":"channel_created","channel":{"id":"C9","connected_team_ids":["T9"]}})),
    );
    assert!(!before.current(&core));
    assert!(authority.external_team(&core, "T9").is_err());
}

#[test]
fn external_boot_and_ordered_raw_updates_retire_selected_absence() {
    let (mut core, authority) = acquired();
    let absent = authority.external_team(&core, "T9").unwrap();
    assert!(!absent.value());
    assert!(absent.current(&core));
    let old = authority.begin_channel_read(&mut core, "C1").unwrap();
    authority
        .begin_channel_read(&mut core, "C1")
        .unwrap()
        .apply(
            &mut core,
            json!({"id":"C1","connected_team_ids":["E1","T2"]}),
        )
        .unwrap();
    assert!(
        old.apply(&mut core, json!({"id":"C1","connected_team_ids":["T9"]}))
            .is_err()
    );
    assert!(!absent.current(&core));
    assert!(authority.external_team(&core, "T2").unwrap().value());
    assert!(!authority.external_team(&core, "E1").unwrap().value());
    let absent = authority.external_team(&core, "T9").unwrap();
    observe_event(
        &mut core,
        "T1",
        0,
        &event(
            json!({"type":"channel_updated","channel":"C1","updates":{"connected_limited_team_ids":["T9"]}}),
        ),
    );
    assert!(!absent.current(&core));
    assert!(authority.external_team(&core, "T9").unwrap().value());
    assert!(authority.raw_channel(&core, "C1").is_err());
}

#[test]
fn external_history_survives_fresh_boot_and_never_mints_cached_authority() {
    let (mut core, authority) = acquired();
    authority
        .observe_external_search_objects(&mut core, &[json!({"id":"T9"})])
        .unwrap();
    let cached = authority.metadata(&core).unwrap().external_cache_snapshot();
    assert!(cached.external.contains("T9").is_err());
    retire(&mut core, "T1");
    assert!(!authority.current(&core));
    assert!(
        begin(&mut core)
            .apply(&mut core, raw_boot(), false)
            .unwrap()
    );
    let next = Authority::capture(&core, "T1").unwrap();
    assert!(next.external_team(&core, "T9").unwrap().value());
    core.workspaces.get_mut("T1").unwrap().posting_metadata = cached;
    assert!(Authority::capture(&core, "T1").is_err());
    assert!(begin(&mut core).apply(&mut core, raw_boot(), true).unwrap());
    assert!(
        Authority::capture(&core, "T1")
            .unwrap()
            .external_team(&core, "T9")
            .unwrap()
            .value()
    );
}

#[test]
fn free_validation_descriptor_requires_current_owner_and_success_before_absence() {
    let mut core = core();
    let mut raw = raw_boot();
    raw["team"] = json!({"id":"T1"});
    assert!(begin(&mut core).apply(&mut core, raw, true).unwrap());
    let authority = Authority::capture(&core, "T1").unwrap();
    assert_eq!(
        authority.external_validation(&core).unwrap(),
        ExternalValidation::Required
    );
    assert!(authority.external_team(&core, "T9").is_err());
    let read = authority.begin_external_validation(&mut core).unwrap();
    let request = read.request(&core).unwrap();
    assert!(request.url.contains("users.slackConnect.orgList"));
    let crate::slack::client::RequestBody::Form(form) = request.body else {
        panic!("form expected");
    };
    assert_eq!(
        form.iter()
            .filter(|(k, _)| k != "token")
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            ("connection_type".into(), "all".into()),
            ("limit".into(), "1".into()),
            ("sort".into(), "count".into())
        ]
    );
    read.apply(&mut core, &json!({"ok":true,"orgs":[]}))
        .unwrap();
    assert!(!authority.external_team(&core, "T9").unwrap().value());
    core.account_epoch += 1;
    assert!(authority.external_team(&core, "T9").is_err());
}

#[test]
fn external_direct_ingress_rejects_changed_credentials_and_validates_whole_page() {
    let (mut core, authority) = acquired();
    assert!(
        authority
            .observe_external_search_objects(&mut core, &[json!({"id":"T9"}), json!({"id":9})])
            .is_err()
    );
    assert!(!authority.external_team(&core, "T9").unwrap().value());
    core.session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .token = "replacement".into();
    assert!(
        authority
            .observe_external_search_objects(&mut core, &[json!({"id":"T9"})])
            .is_err()
    );
}

#[test]
fn external_typed_intake_uses_only_explicit_connections_and_original_owner() {
    let (mut core, authority) = acquired();
    let channel = serde_json::from_value(json!({"id":"C2","connected_team_ids":["T9"],
        "is_read_only":false,"properties":{"posting_restricted_to":{"role":["admin"]}}}))
    .unwrap();
    authority
        .observe_external_channel_connections(&mut core, &[channel])
        .unwrap();
    assert!(authority.external_team(&core, "T9").unwrap().value());
    assert!(authority.raw_channel(&core, "C2").is_err());
    let channel = serde_json::from_value(json!({"id":"C3","connected_team_ids":["T8"]})).unwrap();
    core.session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert("T1".into(), false);
    assert!(
        authority
            .observe_external_channel_connections(&mut core, &[channel])
            .is_err()
    );
}

#[test]
fn raw_enumeration_is_partial_current_evidence_not_an_absence_inventory() {
    let (mut core, authority) = acquired();
    let initial = authority.current_raw_channels(&core).unwrap();
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].value()["id"], "C1");
    authority.begin_channel_read(&mut core, "C2").unwrap().apply(&mut core,
        json!({"id":"C2","context_team_id":"T2","connected_team_ids":["T9"],"properties":{"posting_restricted_to":{"role":["admin"]}}})).unwrap();
    assert_eq!(authority.current_raw_channels(&core).unwrap().len(), 2);
    observe_event(
        &mut core,
        "T1",
        0,
        &event(json!({"type":"channel_updated","channel":"C1","updates":{}})),
    );
    assert!(!initial[0].current(&core));
    let current = authority.current_raw_channels(&core).unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].value()["connected_team_ids"], json!(["T9"]));
    // This list intentionally omits invalidated C1, despite its acquired history.
    // The omission cannot prove C1 or any prior positive external team absent.
    core.workspaces.get_mut("T1").unwrap().channels.insert(
        "C2".into(),
        serde_json::from_value(json!({"id":"C2"})).unwrap(),
    );
    assert!(authority.current_raw_channels(&core).unwrap().is_empty());
    assert!(!current[0].current(&core));
    retire(&mut core, "T1");
    assert!(authority.current_raw_channels(&core).is_err());
}

#[test]
fn unbound_boot_cannot_be_authenticated_at_picker_open() {
    let mut core = core();
    core.workspaces
        .get_mut("T1")
        .unwrap()
        .apply_boot(serde_json::from_value(raw_boot()).unwrap());
    assert!(Authority::capture(&core, "T1").is_err());
    let read = begin(&mut core);
    assert!(read.request(&core).unwrap().url.contains("client.userBoot"));
    assert!(read.apply(&mut core, raw_boot(), false).unwrap());
    let authority = Authority::capture(&core, "T1").unwrap();
    let raw = authority.raw_channel(&core, "C1").unwrap();
    assert!(raw.value().get("is_read_only").is_none());
    assert!(
        authority
            .metadata(&core)
            .unwrap()
            .effective(authority.configuration(), raw.value())
            .unwrap()
            .permits_root_post()
    );
    assert_eq!(authority.internal_workspaces(&core).unwrap(), ["T1", "T2"]);
    let debug = format!(
        "{:?} {authority:?} {raw:?}",
        authority.metadata(&core).unwrap()
    );
    for secret in ["fixture-token", "fixture-cookie", "private-fixture-name"] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn request_start_owns_account_credentials_config_and_transport() {
    for change in 0..10 {
        let (mut core, authority) = acquired();
        let read = begin(&mut core);
        match change {
            0 => core.account_epoch += 1,
            1 => core.active_account = Some("other".into()),
            2 => core.session.as_mut().unwrap().d_cookie = "other".into(),
            3 => {
                core.session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .token = "other".into()
            }
            4 => {
                core.session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .url = "https://other.slack.com".into()
            }
            5 => {
                core.session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .enterprise_id = Some("E2".into())
            }
            6 => {
                core.session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .user_id = "U2".into()
            }
            7 => {
                core.session
                    .as_mut()
                    .unwrap()
                    .unified_user_client_enabled
                    .insert("T1".into(), false);
            }
            8 => core.transport = Some(Arc::new(Transport::disconnected_fixture("fixture-cookie"))),
            9 => {
                core.session
                    .as_mut()
                    .unwrap()
                    .unified_user_client_enabled
                    .insert("T2".into(), true);
            }
            _ => unreachable!(),
        }
        assert!(!authority.current(&core), "case {change}");
        assert!(!read.current(&core), "case {change}");
        assert!(read.request(&core).is_err());
        assert!(!read.apply(&mut core, raw_boot(), true).unwrap());
        assert!(Authority::capture(&core, "T1").is_err());
    }
}

#[test]
fn newest_boot_wins_and_stale_failure_cannot_release_it() {
    let mut core = core();
    let old = begin(&mut core);
    let latest = begin(&mut core);
    assert!(!old.failed(&mut core));
    assert!(latest.current(&core));
    assert!(latest.apply(&mut core, raw_boot(), true).unwrap());
    let authority = Authority::capture(&core, "T1").unwrap();
    assert!(begin(&mut core).failed(&mut core));
    assert!(authority.current(&core));
}

#[test]
fn malformed_foreign_and_missing_envelopes_never_mint_authority() {
    let mut foreign = raw_boot();
    foreign["team"]["id"] = json!("T2");
    let mut malformed = raw_boot();
    malformed["channels"] = json!("not-an-array");
    for raw in [
        json!({"ok":false}),
        json!({"team":{"id":"T1"},"self":{"id":"U1"}}),
        foreign,
        malformed,
    ] {
        let mut core = core();
        assert!(begin(&mut core).apply(&mut core, raw, true).is_err());
        assert!(Authority::capture(&core, "T1").is_err());
    }
    let mut core = core();
    let mut raw = raw_boot();
    raw["self"]["id"] = json!("U2");
    assert!(begin(&mut core).apply(&mut core, raw, true).is_err());
    assert!(Authority::capture(&core, "T1").is_err());
}

#[test]
fn typed_refresh_is_only_an_invalidation_witness_never_new_permission() {
    for direct in [false, true] {
        let (mut core, authority) = acquired();
        let selected = authority.raw_channel(&core, "C1").unwrap();
        let pending = authority.begin_channel_read(&mut core, "C1").unwrap();
        let selected = authority.raw_channel(&core, "C1").unwrap_or(selected);
        let workspace = core.workspaces.get_mut("T1").unwrap();
        if direct {
            workspace.channels.get_mut("C1").unwrap().extra.insert(
                "properties".into(),
                json!({"posting_restricted_to":{"role":["admin"]}}),
            );
        } else {
            workspace.apply_channel_refresh(
                serde_json::from_value(json!({"channels":[{"id":"C1","is_archived":true}]}))
                    .unwrap(),
            );
        }
        assert!(!selected.current(&core));
        assert!(!pending.current(&core));
        assert!(authority.raw_channel(&core, "C1").is_err());
        let fresh = authority
            .begin_channel_read(&mut core, "C1")
            .unwrap()
            .apply(&mut core, json!({"id":"C1","context_team_id":"T1"}))
            .unwrap();
        assert!(fresh.current(&core));
        // An omitted flag after invalidation cannot revive older boot grants.
        assert!(
            authority
                .metadata(&core)
                .unwrap()
                .effective(authority.configuration(), fresh.value())
                .is_err()
        );
    }
}

#[test]
fn raw_ingress_retires_older_boot_and_metadata_recovery_preserves_workspace() {
    let (mut core, authority) = acquired();
    let boot = begin(&mut core);
    authority
        .begin_channel_read(&mut core, "C1")
        .unwrap()
        .apply(&mut core, json!({"id":"C1","context_team_id":"T1"}))
        .unwrap();
    assert!(!boot.current(&core));
    assert!(!boot.apply(&mut core, raw_boot(), false).unwrap());
    let workspace = core.workspaces.get_mut("T1").unwrap();
    workspace.channels.get_mut("C1").unwrap().name = Some("keep-current-workspace-name".into());
    retire(&mut core, "T1");
    assert!(
        begin(&mut core)
            .apply(&mut core, raw_boot(), false)
            .unwrap()
    );
    assert_eq!(
        core.workspaces["T1"].channels["C1"].name.as_deref(),
        Some("keep-current-workspace-name")
    );
    assert!(
        Authority::capture(&core, "T1")
            .unwrap()
            .raw_channel(&core, "C1")
            .is_ok()
    );
}

#[test]
fn legacy_empty_user_keeps_normal_boot_without_creating_authority() {
    let mut core = core();
    core.session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T1")
        .unwrap()
        .user_id
        .clear();
    core.workspaces.get_mut("T1").unwrap().self_user_id.clear();
    assert!(begin(&mut core).apply(&mut core, raw_boot(), true).unwrap());
    assert_eq!(core.workspaces["T1"].self_user_id, "U1");
    assert!(core.workspaces["T1"].channels.contains_key("C1"));
    assert!(Authority::capture(&core, "T1").is_err());
}

#[test]
fn socket_partial_retires_raw_and_inventory_and_rejects_inflight_reads() {
    let (mut core, authority) = acquired();
    let selected = authority.raw_channel(&core, "C1").unwrap();
    let read = authority.begin_channel_read(&mut core, "C1").unwrap();
    let boot = begin(&mut core);
    observe_event(
        &mut core,
        "T1",
        0,
        &event(json!({"type":"channel_updated","channels":["C1"],"updates":{"is_archived":true}})),
    );
    assert!(!selected.current(&core));
    assert!(!read.current(&core));
    assert!(!boot.current(&core));
    assert!(authority.current(&core));
    assert!(authority.raw_channel(&core, "C1").is_err());
    assert!(
        authority
            .metadata(&core)
            .unwrap()
            .effective(
                authority.configuration(),
                &json!({"id":"C1","context_team_id":"T1"})
            )
            .is_err()
    );
    assert!(read.apply(&mut core, json!({"id":"C1"})).is_err());
    assert!(!boot.apply(&mut core, raw_boot(), true).unwrap());
    let fresh = authority
        .begin_channel_read(&mut core, "C1")
        .unwrap()
        .apply(
            &mut core,
            json!({"id":"C1","context_team_id":"T1","is_read_only":true,"is_thread_only":false}),
        )
        .unwrap();
    assert!(fresh.current(&core));
    assert!(
        authority
            .metadata(&core)
            .unwrap()
            .effective(authority.configuration(), fresh.value())
            .unwrap()
            .read_only
    );
}

#[test]
fn per_channel_read_order_and_stale_socket_generation_are_independent() {
    let (mut core, authority) = acquired();
    let old = authority.begin_channel_read(&mut core, "C1").unwrap();
    let latest = authority.begin_channel_read(&mut core, "C1").unwrap();
    let other = authority.begin_channel_read(&mut core, "C2").unwrap();
    observe_event(
        &mut core,
        "T1",
        99,
        &event(json!({"type":"channel_updated","channel":"C1","updates":{}})),
    );
    assert!(!old.current(&core));
    assert!(latest.current(&core));
    assert!(other.current(&core));
    let raw = latest
        .apply(&mut core, json!({"id":"C1","context_team_id":"T1"}))
        .unwrap();
    assert!(raw.current(&core));
    assert!(old.apply(&mut core, json!({"id":"C1"})).is_err());
    assert!(other.apply(&mut core, json!({"id":"C3"})).is_err());
    assert!(raw.current(&core));
}

#[test]
fn unknown_dependency_retains_raw_for_same_observation_recompute() {
    let (mut core, authority) = acquired();
    let raw = authority.begin_channel_read(&mut core, "C2").unwrap().apply(&mut core,
        json!({"id":"C2","context_team_id":"T2","properties":{"posting_restricted_to":{"role":["admin"]},"threads_restricted_to":{"role":["admin"]}}})).unwrap();
    assert!(raw.current(&core));
    assert!(
        authority
            .metadata(&core)
            .unwrap()
            .effective(authority.configuration(), raw.value())
            .is_err()
    );
    let read = authority
        .metadata_mut(&mut core)
        .unwrap()
        .begin_workspace_role_read(authority.configuration(), raw.value(), "T2")
        .unwrap();
    read.apply(
        authority.metadata_mut(&mut core).unwrap(),
        authority.configuration(),
        &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":true}}}),
    )
    .unwrap();
    assert!(raw.current(&core));
    raw.observe(&mut core).unwrap();
    assert!(
        authority
            .metadata(&core)
            .unwrap()
            .effective(authority.configuration(), raw.value())
            .unwrap()
            .permits_root_post()
    );
}

#[test]
fn known_role_revocation_cannot_replay_old_explicit_raw_grants() {
    let (mut core, authority) = acquired();
    let raw = authority
        .begin_channel_read(&mut core, "C1")
        .unwrap()
        .apply(
            &mut core,
            json!({"id":"C1","context_team_id":"T1","is_read_only":false,"is_thread_only":false}),
        )
        .unwrap();
    let read = authority
        .metadata_mut(&mut core)
        .unwrap()
        .begin_workspace_role_read(authority.configuration(), raw.value(), "T1")
        .unwrap();
    read.apply(
        authority.metadata_mut(&mut core).unwrap(),
        authority.configuration(),
        &json!({"ok":true,"user":{"id":"U1","team_user":{"is_admin":false}}}),
    )
    .unwrap();
    assert!(authority.current(&core));
    assert!(!raw.current(&core));
    assert!(raw.observe(&mut core).is_err());
    assert!(authority.raw_channel(&core, "C1").is_err());
}

#[test]
fn global_policy_and_membership_events_require_fresh_boot() {
    for kind in [
        "feature_access_policies_updated",
        "team_pref_change",
        "team_plan_change",
        "user_removed_from_team",
        "user_permissions_invalidated",
    ] {
        let (mut core, authority) = acquired();
        let boot = begin(&mut core);
        observe_event(&mut core, "T1", 0, &event(json!({"type":kind})));
        assert!(!authority.current(&core));
        assert!(!boot.current(&core));
        assert!(
            begin(&mut core)
                .apply(&mut core, raw_boot(), false)
                .unwrap()
        );
        assert!(Authority::capture(&core, "T1").is_ok());
    }
}

#[test]
fn explicit_order_loss_and_self_user_change_retire_before_callback() {
    for self_event in [false, true] {
        let (mut core, authority) = acquired();
        let read = authority.begin_channel_read(&mut core, "C1").unwrap();
        if self_event {
            observe_event(
                &mut core,
                "T1",
                0,
                &RtEvent::UserChanged(serde_json::from_value(json!({"id":"U1"})).unwrap()),
            );
        } else {
            retire(&mut core, "T1");
        }
        assert!(!authority.current(&core));
        assert!(!read.current(&core));
    }
}

#[test]
fn converted_socket_connections_retire_absence_and_partial_posting_evidence() {
    for (kind, field) in [
        ("channel_converted_to_shared", "channel"),
        ("group_converted_to_shared", "group"),
    ] {
        let (mut core, authority) = acquired();
        let absent = authority.external_team(&core, "T9").unwrap();
        let raw = authority.raw_channel(&core, "C1").unwrap();
        let mut payload = json!({"type":kind});
        payload[field] = json!({"id":"C1","is_org_shared":true,
            "connected_team_ids":["E1","T9"]});
        let converted = event(payload);
        observe_event(&mut core, "T1", 1, &converted);
        assert!(absent.current(&core));
        observe_event(&mut core, "T1", 0, &converted);
        assert!(!absent.current(&core));
        assert!(authority.external_team(&core, "T9").unwrap().value());
        assert!(!authority.external_team(&core, "E1").unwrap().value());
        assert!(!raw.current(&core));
        assert!(authority.raw_channel(&core, "C1").is_err());
    }
}

#[test]
fn converted_socket_ignores_unknown_archived_unshared_or_wrong_payload_channels() {
    for mode in 0..5 {
        let (mut core, authority) = acquired();
        let absent = authority.external_team(&core, "T9").unwrap();
        let mut payload = json!({"type":"channel_converted_to_shared", "channel":{
            "id":"C1","is_org_shared":true,"connected_team_ids":["T9"]}});
        match mode {
            0 => payload["channel"]["id"] = json!("C404"),
            1 => {
                core.workspaces
                    .get_mut("T1")
                    .unwrap()
                    .channels
                    .get_mut("C1")
                    .unwrap()
                    .is_archived = true
            }
            2 => {
                payload["channel"]["is_org_shared"] = json!(false);
                payload["channel"]["is_global_shared"] = json!(true);
            }
            3 => payload["channel"] = Value::Null,
            _ => payload["type"] = json!("group_converted_to_shared"),
        }
        observe_event(&mut core, "T1", 0, &event(payload));
        assert!(absent.current(&core));
        assert!(!authority.external_team(&core, "T9").unwrap().value());
    }
}
