//! Delivery evidence belongs to an account/team/person, not a transport handle.
use super::*;
use std::sync::Arc;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
};

fn credentials(shell: &mut ShellState, name: &str) {
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.core.transport = Some(Arc::new(Transport::new(name).unwrap()));
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: name.into(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team,
                user_id: user,
                enterprise_id: None,
                name: "Fixture".into(),
                url: "https://example.test".into(),
                token: name.into(),
            },
        )]
        .into(),
    });
}
fn prepared(duplicate: bool) -> (ShellState, Attempt, Loaded) {
    let mut shell = shell();
    fixture::add_duplicate_views(&mut shell).unwrap();
    credentials(&mut shell, "synthetic-original");
    shell.slack_lists.scope = Scope::capture(&shell);
    let attempt = if duplicate {
        let view = shell.slack_lists.view.as_ref().unwrap();
        let (visit, layout, instance) = (
            view.generation,
            view.layout.generation,
            view.rows[0].instance_id.clone(),
        );
        assert!(record_detail::open_shell(
            &mut shell, &instance, visit, layout
        ));
        let generation = record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
            .unwrap()
            .generation;
        duplicate::begin(&mut shell, generation).unwrap()
    } else {
        begin_current(&mut shell)
    };
    let data = loaded(&shell);
    (shell, attempt, data)
}
fn reopen(shell: &mut ShellState, data: Loaded) {
    close_shell(shell);
    let file = serde_json::from_value(data.document.raw.clone()).unwrap();
    assert!(open_shell(shell, file));
    read(shell, data);
}

#[test]
fn credential_rotation_retires_unsent_authority_for_add_and_duplicate() {
    for duplicate in [false, true] {
        let (mut shell, attempt, data) = prepared(duplicate);
        credentials(&mut shell, "synthetic-rotated");
        shell.core.account_epoch += 1;
        assert!(poll(&mut shell));
        assert!(validate(&mut shell, &attempt, &data).is_err());
        assert!(shell.slack_lists.create.receipts.is_empty());
    }
}

#[test]
fn sent_receipt_survives_rotation_and_late_ack_only_settles_original_delivery() {
    for duplicate in [false, true] {
        let (mut shell, attempt, data) = prepared(duplicate);
        validate(&mut shell, &attempt, &data).unwrap();
        credentials(&mut shell, "synthetic-rotated");
        shell.core.account_epoch += 1;
        assert!(poll(&mut shell));
        assert!(shell.slack_lists.create.receipts[&attempt.key()].stage == Stage::Uncertain);
        reopen(&mut shell, data.clone());
        assert!(blocks_mutation(&shell));
        assert!(!presentation(&shell).enabled);
        let revision = shell.slack_lists.view.as_ref().unwrap().revision;
        assert!(!finish(
            &mut shell,
            &attempt,
            Ok(json!({"ok":true,"record":created()}))
        ));
        assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, revision);
        assert!(receipt(&shell).unwrap().stage == Stage::AwaitingRead);
        assert!(validate(&mut shell, &attempt, &data).is_err());
        read(&mut shell, with_record(data, created()));
        assert!(!blocks_mutation(&shell));
        assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
        assert!(presentation(&shell).can_open);
    }
}

#[test]
fn unknown_delivery_stays_locked_after_rotation_and_rejects_second_response() {
    for duplicate in [false, true] {
        let (mut shell, attempt, data) = prepared(duplicate);
        validate(&mut shell, &attempt, &data).unwrap();
        assert!(finish(
            &mut shell,
            &attempt,
            Err(Error::Transport("fixture lost response".into()))
        ));
        credentials(&mut shell, "synthetic-rotated");
        shell.core.account_epoch += 1;
        poll(&mut shell);
        reopen(&mut shell, with_record(data.clone(), created()));
        assert!(blocks_mutation(&shell));
        assert!(presentation(&shell).operation.unwrap().can_retire);
        assert!(!presentation(&shell).can_open);
        assert!(!finish(
            &mut shell,
            &attempt,
            Ok(json!({"ok":true,"record":created()}))
        ));
        assert!(validate(&mut shell, &attempt, &data).is_err());
    }
}

#[test]
fn account_or_person_replacement_cannot_see_or_clear_original_delivery() {
    for replacement in ["account", "person"] {
        let (mut shell, attempt, data) = prepared(true);
        validate(&mut shell, &attempt, &data).unwrap();
        let account = shell.core.active_account.clone();
        let team = shell.core.active_team.clone().unwrap();
        let user = shell.self_account.user_id.clone();
        if replacement == "account" {
            shell.core.active_account = Some("other-account".into());
        } else {
            shell.self_account.user_id = "OTHER_USER".into();
            shell.core.workspaces.get_mut(&team).unwrap().self_user_id = "OTHER_USER".into();
        }
        credentials(&mut shell, "synthetic-replacement");
        shell.core.account_epoch += 1;
        poll(&mut shell);
        reopen(&mut shell, data.clone());
        assert!(presentation(&shell).operation.is_none());
        assert!(!retire(&mut shell, attempt.serial));
        assert!(!finish(
            &mut shell,
            &attempt,
            Ok(json!({"ok":true,"record":created()}))
        ));
        assert_eq!(shell.slack_lists.create.receipts.len(), 1);
        shell.core.active_account = account;
        shell.self_account.user_id = user.clone();
        shell.core.workspaces.get_mut(&team).unwrap().self_user_id = user;
        credentials(&mut shell, "synthetic-returned");
        shell.core.account_epoch += 1;
        reopen(&mut shell, data);
        assert!(blocks_mutation(&shell));
        assert!(receipt(&shell).unwrap().stage == Stage::AwaitingRead);
    }
}
