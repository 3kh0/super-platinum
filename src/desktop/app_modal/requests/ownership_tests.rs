use super::*;
use std::sync::Arc;
use super_platinum_core::{
    config::{Session as Credentials, WorkspaceSession},
    slack::Transport,
};

fn setup(submission: bool) -> (ShellState, Ticket, Value) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    users_fixture::install(&mut shell);
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell.core.session = Some(Credentials {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic-cookie".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U00000001".into(),
                name: "Fixture".into(),
                url: "https://example.test".into(),
                token: "synthetic-token".into(),
            },
        )]
        .into(),
    });
    let session = shell.app_modal.as_mut().unwrap();
    let form = session.current_form_mut().unwrap();
    form.set_value(
        &InputKey {
            block_id: "note".into(),
            action_id: "value".into(),
        },
        InputValue::Text("Retain this draft".into()),
    )
    .unwrap();
    let before = form.submission().unwrap();
    session.busy = submission;
    session.loading = !submission;
    let ticket = Ticket::capture(&shell).unwrap();
    assert!(ticket.current(&shell));
    (shell, ticket, before)
}
fn rotate(shell: &mut ShellState, token: bool) {
    if token {
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token = "rotated-synthetic-token".into();
    } else {
        shell.core.transport = Some(Arc::new(
            Transport::new("rotated-synthetic-cookie").unwrap(),
        ));
    }
}
#[test]
fn same_owner_token_or_transport_replacement_settles_submit_without_replay_or_draft_loss() {
    for token in [false, true] {
        let (mut shell, ticket, before) = setup(true);
        rotate(&mut shell, token);
        assert!(!ticket.current(&shell));
        assert!(ticket.settle_lost_authority(&mut shell, true));
        let s = shell.app_modal.as_ref().unwrap();
        assert!(!s.busy);
        assert!(s.uncertain && s.locked());
        assert!(!s.conflict);
        assert!(
            s.view_error
                .as_ref()
                .unwrap()
                .contains("will not be repeated")
        );
        assert_eq!(s.current_form().unwrap().submission().unwrap(), before);
        assert_eq!(s.fixture_attempts, 0);
        // Repeated/late completion cannot reopen or settle the uncertain attempt.
        assert!(!ticket.settle_lost_authority(&mut shell, true));
        assert!(shell.app_modal.as_ref().unwrap().uncertain);
    }
}
#[test]
fn same_owner_rotation_ends_loading_with_explicit_error_and_keeps_existing_fields() {
    for token in [false, true] {
        let (mut shell, ticket, before) = setup(false);
        rotate(&mut shell, token);
        assert!(ticket.settle_lost_authority(&mut shell, false));
        let s = shell.app_modal.as_ref().unwrap();
        assert!(!s.loading);
        assert!(s.conflict && s.locked());
        assert!(!s.uncertain);
        assert!(s.view_error.as_ref().unwrap().contains("Close it"));
        assert_eq!(s.current_form().unwrap().submission().unwrap(), before);
        assert!(!ticket.settle_lost_authority(&mut shell, false));
    }
}
#[test]
fn lost_authority_completion_cannot_change_new_view_account_person_or_overlay() {
    let changes: [fn(&mut ShellState); 7] = [
        |s| s.core.account_epoch += 1,
        |s| s.core.active_team = Some("other".into()),
        |s| s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
        |s| {
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .user_id = "U99999999".into()
        },
        |s| s.core.workspaces.get_mut("T1").unwrap().self_user_id = "U99999999".into(),
        |s| s.app_modal.as_mut().unwrap().generation = next_id(),
        |s| s.overlay = None,
    ];
    for submission in [false, true] {
        for change in changes {
            let (mut shell, ticket, before) = setup(submission);
            rotate(&mut shell, true);
            change(&mut shell);
            assert!(!ticket.current(&shell));
            assert!(!ticket.settle_lost_authority(&mut shell, submission));
            let s = shell.app_modal.as_ref().unwrap();
            assert_eq!(s.busy, submission);
            assert_eq!(s.loading, !submission);
            assert!(!s.uncertain && !s.conflict);
            assert!(s.view_error.is_none());
            assert_eq!(s.current_form().unwrap().submission().unwrap(), before);
        }
    }
}
#[test]
fn current_credentials_keep_normal_completion_authority() {
    for submission in [false, true] {
        let (mut shell, ticket, _) = setup(submission);
        assert!(!ticket.settle_lost_authority(&mut shell, submission));
        assert!(ticket.current(&shell));
    }
}
