use super::*;
use serde_json::json;
use super_platinum_core::{config::Session, slack::models::Channel, state::Workspace};
fn raw() -> Value {
    json!({"id":"C00000001","is_channel":true,"is_member":true,"is_ext_shared":false,
    "is_read_only":false,"is_thread_only":false,"future":{"retain":[3,1]}})
}
fn sample() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: "T00000001".into(),
        user_id: "U00000001".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.slack.com".into(),
        token: "synthetic-token".into(),
    };
    let mut workspace = Workspace::from_session(&session);
    workspace.channels.insert(
        "C00000001".into(),
        serde_json::from_value::<Channel>(raw()).unwrap(),
    );
    workspace.users.insert(
        "U00000001".into(),
        serde_json::from_value::<User>(json!({"id":"U00000001","is_bot":false})).unwrap(),
    );
    shell
        .core
        .workspaces
        .insert(session.team_id.clone(), workspace);
    shell.core.active_team = Some(session.team_id.clone());
    shell.self_account.user_id = session.user_id.clone();
    shell.core.active_account = Some("synthetic-account".into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic-cookie".into(),
        workspaces: [(session.team_id.clone(), session)].into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell
}
fn complete(owner: &mut Owner, shell: &ShellState, value: Value) -> Snapshot {
    let read = owner.begin(shell).unwrap();
    owner
        .finish(shell, read, json!({"ok":true,"channel":value}))
        .unwrap()
}
#[test]
fn exact_raw_positive_info_is_preserved_and_request_is_existing_read_route() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let read = owner.begin(&shell).unwrap();
    let request = read.request();
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/conversations.info"
    );
    let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic-token".into()),
            ("channel".into(), "C00000001".into()),
            ("include_num_members".into(), "false".into())
        ]
    );
    let snapshot = owner
        .finish(&shell, read, json!({"ok":true,"channel":raw()}))
        .unwrap();
    assert_eq!(snapshot.raw(), &raw());
    assert_eq!(snapshot.eligibility(), Eligibility::Allowed);
    assert!(owner.current(&shell, &snapshot));
}
#[test]
fn omission_retains_restriction_and_explicit_false_clears_it() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let mut restricted = raw();
    restricted["is_general"] = json!(true);
    restricted["is_read_only"] = json!(true);
    assert_eq!(
        complete(&mut owner, &shell, restricted).eligibility(),
        Eligibility::Denied(policy::Denial::ReadOnly)
    );
    let mut partial = raw();
    partial["is_general"] = json!(true);
    partial.as_object_mut().unwrap().remove("is_read_only");
    assert_eq!(
        complete(&mut owner, &shell, partial).eligibility(),
        Eligibility::Denied(policy::Denial::ReadOnly)
    );
    assert_eq!(
        complete(&mut owner, &shell, raw()).eligibility(),
        Eligibility::Allowed
    );
}
#[test]
fn contradictory_foreign_or_oversized_info_never_installs() {
    for response in [
        json!({"ok":true,"error":"denied","channel":raw()}),
        json!({"channel":raw()}),
        json!({"ok":true,"channel":{"id":"C00000002"}}),
        json!({"ok":true,"channel":raw(),"oversized":"x".repeat(RAW_LIMIT)}),
    ] {
        let shell = sample();
        let mut owner = Owner::capture(&shell, "C00000001").unwrap();
        let read = owner.begin(&shell).unwrap();
        assert!(owner.finish(&shell, read, response).is_err());
        assert!(owner.latest.is_none());
        assert_eq!(owner.facts.values(), (None, None));
    }
}
#[test]
fn raw_only_facts_never_come_from_default_filled_local_channel() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let mut response = raw();
    response.as_object_mut().unwrap().remove("is_read_only");
    response.as_object_mut().unwrap().remove("is_thread_only");
    let snapshot = complete(&mut owner, &shell, response);
    assert_eq!(
        snapshot.eligibility(),
        Eligibility::Unsupported(policy::Unsupported::PostingFacts)
    );
}
#[test]
fn transport_account_credentials_generation_and_canonical_changes_refuse_old_read() {
    for change in 0..9 {
        let mut shell = sample();
        let mut owner = Owner::capture(&shell, "C00000001").unwrap();
        let read = owner.begin(&shell).unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("another".into()),
            2 => shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            3 => shell.core.session.as_mut().unwrap().d_cookie = "rotated".into(),
            4 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .token = "rotated".into()
            }
            5 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .rt_generation += 1
            }
            6 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .channels
                    .get_mut("C00000001")
                    .unwrap()
                    .name = Some("changed".into())
            }
            7 => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .self_user_id = "OTHER".into()
            }
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut("T00000001")
                    .unwrap()
                    .channels
                    .get_mut("C00000001")
                    .unwrap()
                    .extra
                    .insert("is_member".into(), json!(false));
            }
        }
        assert!(
            owner
                .finish(&shell, read, json!({"ok":true,"channel":raw()}))
                .is_err()
        );
    }
}
#[test]
fn channel_updated_retires_inflight_read_and_old_grants_without_automatic_read() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let snapshot = complete(&mut owner, &shell, raw());
    let read = owner.begin(&shell).unwrap();
    let event = super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"channel_updated",
        "channel":"C00000001","updates":{"permissions":{"post":{},"thread":{}}}})
        .to_string(),
    )
    .unwrap();
    owner.event(&shell, "T00000001", 0, &event);
    assert!(!owner.current(&shell, &snapshot));
    assert!(owner.pending.is_none());
    assert_eq!(owner.facts.values(), (None, None));
    assert!(matches!(
        owner.finish(&shell, read, json!({"ok":true,"channel":raw()})),
        Err(Refusal::Changed)
    ));
    let mut missing = raw();
    missing.as_object_mut().unwrap().remove("is_read_only");
    missing.as_object_mut().unwrap().remove("is_thread_only");
    assert_eq!(
        complete(&mut owner, &shell, missing).eligibility(),
        Eligibility::Unsupported(policy::Unsupported::PostingFacts)
    );
}
#[test]
fn old_ticket_failed_completion_and_owner_replacement_cannot_retire_new_read() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let old = owner.begin(&shell).unwrap();
    assert!(matches!(owner.begin(&shell), Err(Refusal::Pending)));
    owner.invalidate(false);
    let new = owner.begin(&shell).unwrap();
    owner.failed(&old);
    assert_eq!(owner.pending, Some(new.serial));
    let snapshot = owner
        .finish(&shell, new, json!({"ok":true,"channel":raw()}))
        .unwrap();
    let replacement = Owner::capture(&shell, "C00000001").unwrap();
    assert!(!replacement.current(&shell, &snapshot));
}
#[test]
fn checked_clock_exhaustion_has_no_reuse() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    owner.serial = Some(u64::MAX);
    assert!(matches!(owner.begin(&shell), Err(Refusal::Exhausted)));
    assert!(owner.serial.is_none());
    assert!(matches!(owner.begin(&shell), Err(Refusal::Exhausted)));
}
#[test]
fn unrelated_event_preserves_ready_snapshot_but_disconnect_retires_it() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let snapshot = complete(&mut owner, &shell, raw());
    let event = super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"channel_updated",
        "channel":"C00000002","updates":{"permissions":{}}})
        .to_string(),
    )
    .unwrap();
    owner.event(&shell, "T00000001", 0, &event);
    assert!(owner.current(&shell, &snapshot));
    owner.invalidate(true);
    assert!(!owner.current(&shell, &snapshot));
}

#[test]
fn same_value_channel_refresh_changes_existing_revision_and_retires_snapshot() {
    let mut shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let snapshot = complete(&mut owner, &shell, raw());
    let page = serde_json::from_value(json!({"channels":[raw()]})).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T00000001")
        .unwrap()
        .apply_channel_refresh(page);
    assert!(!owner.current(&shell, &snapshot));
}
#[test]
fn unrelated_partial_event_preserves_facts_but_requires_another_current_read() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let snapshot = complete(&mut owner, &shell, raw());
    let event = super_platinum_core::slack::realtime::parse_event(
        &json!({"type":"channel_updated",
        "channel":"C00000001","updates":{"name":"renamed"}})
        .to_string(),
    )
    .unwrap();
    owner.event(&shell, "T00000001", 0, &event);
    assert!(!owner.current(&shell, &snapshot));
    assert_eq!(owner.facts.values(), (Some(false), Some(false)));
    let mut partial = raw();
    partial.as_object_mut().unwrap().remove("is_read_only");
    partial.as_object_mut().unwrap().remove("is_thread_only");
    assert_eq!(
        complete(&mut owner, &shell, partial).eligibility(),
        Eligibility::Allowed
    );
}

#[test]
fn canonical_restriction_refresh_then_partial_info_cannot_rebase_old_grant() {
    let mut shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let snapshot = complete(&mut owner, &shell, raw());
    assert_eq!(snapshot.eligibility(), Eligibility::Allowed);
    let mut restricted = raw();
    restricted["is_general"] = json!(true);
    restricted["is_read_only"] = json!(true);
    let page = serde_json::from_value(json!({"channels":[restricted]})).unwrap();
    shell
        .core
        .workspaces
        .get_mut("T00000001")
        .unwrap()
        .apply_channel_refresh(page);
    assert!(!owner.current(&shell, &snapshot));
    let mut partial = raw();
    partial["is_general"] = json!(true);
    partial.as_object_mut().unwrap().remove("is_read_only");
    partial.as_object_mut().unwrap().remove("is_thread_only");
    let next = complete(&mut owner, &shell, partial);
    assert_eq!(owner.facts.values(), (None, None));
    assert_eq!(
        next.eligibility(),
        Eligibility::Unsupported(policy::Unsupported::PostingFacts)
    );
    let mut explicit = raw();
    explicit["is_general"] = json!(true);
    explicit["is_read_only"] = json!(true);
    assert_eq!(
        complete(&mut owner, &shell, explicit).eligibility(),
        Eligibility::Denied(policy::Denial::ReadOnly)
    );
}
#[test]
fn member_revision_and_unstamped_canonical_changes_retire_fact_provenance() {
    for change in 0..3 {
        let mut shell = sample();
        let mut owner = Owner::capture(&shell, "C00000001").unwrap();
        complete(&mut owner, &shell, raw());
        let workspace = shell.core.workspaces.get_mut("T00000001").unwrap();
        match change {
            0 => workspace.users.get_mut("U00000001").unwrap().is_restricted = true,
            1 => workspace.apply_channel_refresh(
                serde_json::from_value(json!({"channels":[raw()]})).unwrap(),
            ),
            _ => {
                workspace
                    .channels
                    .get_mut("C00000001")
                    .unwrap()
                    .extra
                    .insert("is_read_only".into(), json!(true));
            }
        }
        let read = owner.begin(&shell).unwrap();
        assert_eq!(owner.facts.values(), (None, None));
        assert!(owner.fact_observation.is_none());
        owner.failed(&read);
        assert!(owner.fact_observation.is_none());
        let mut partial = raw();
        partial.as_object_mut().unwrap().remove("is_read_only");
        partial.as_object_mut().unwrap().remove("is_thread_only");
        assert_eq!(
            complete(&mut owner, &shell, partial).eligibility(),
            Eligibility::Unsupported(policy::Unsupported::PostingFacts)
        );
        assert_eq!(
            complete(&mut owner, &shell, raw()).eligibility(),
            Eligibility::Allowed
        );
    }
}

#[test]
fn capture_requires_exact_joined_cg_id_and_map_identity() {
    for id in ["D00000001", "C_SHORT", "C00000001"] {
        let mut shell = sample();
        let workspace = shell.core.workspaces.get_mut("T00000001").unwrap();
        let mut value = raw();
        value["id"] = json!(if id == "C00000001" { "C00000002" } else { id });
        workspace
            .channels
            .insert(id.into(), serde_json::from_value(value).unwrap());
        assert!(Owner::capture(&shell, id).is_none());
    }
}

#[test]
fn map_scope_probe_is_read_only_and_does_not_replace_policy_validation() {
    let mut shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let snapshot = complete(&mut owner, &shell, raw());
    let serial = owner.serial;
    let facts = owner.facts.clone();
    assert!(owner.same_scope(&shell, "C00000001"));
    assert!(!owner.same_scope(&shell, "C00000002"));
    assert_eq!(owner.serial, serial);
    assert_eq!(owner.facts, facts);
    assert!(owner.current(&shell, &snapshot));
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut("T00000001")
        .unwrap()
        .token = "rotated".into();
    assert!(!owner.same_scope(&shell, "C00000001"));
    assert_eq!(owner.serial, serial);
}

#[test]
fn reclaimability_counts_internal_latest_and_every_external_snapshot_clone() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    assert!(owner.reclaimable());
    let snapshot = complete(&mut owner, &shell, raw());
    let clone = snapshot.clone();
    assert!(!owner.reclaimable());
    drop(snapshot);
    assert!(!owner.reclaimable());
    drop(clone);
    assert!(owner.latest.is_some());
    assert!(owner.reclaimable());
}
#[test]
fn invalidation_does_not_release_external_read_or_snapshot_custody() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let snapshot = complete(&mut owner, &shell, raw());
    let read = owner.begin(&shell).unwrap();
    assert!(!owner.reclaimable());
    owner.invalidate(true);
    assert!(owner.pending.is_none() && owner.latest.is_none());
    assert!(!owner.reclaimable());
    drop(snapshot);
    assert!(!owner.reclaimable());
    drop(read);
    assert!(owner.reclaimable());
}
#[test]
fn stale_credentials_and_realtime_scope_do_not_override_external_holders() {
    for realtime in [false, true] {
        let mut shell = sample();
        let mut owner = Owner::capture(&shell, "C00000001").unwrap();
        let snapshot = complete(&mut owner, &shell, raw());
        if realtime {
            shell
                .core
                .workspaces
                .get_mut("T00000001")
                .unwrap()
                .rt_generation += 1;
        } else {
            shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T00000001")
                .unwrap()
                .token = "replacement".into();
        }
        assert!(!owner.same_scope(&shell, "C00000001"));
        assert!(!owner.reclaimable());
        drop(snapshot);
        assert!(owner.reclaimable());
    }
}
#[test]
fn pending_even_without_remaining_ticket_is_not_reclaimed() {
    let shell = sample();
    let mut owner = Owner::capture(&shell, "C00000001").unwrap();
    let read = owner.begin(&shell).unwrap();
    drop(read);
    assert!(!owner.reclaimable());
    owner.invalidate(true);
    assert!(owner.reclaimable());
}
