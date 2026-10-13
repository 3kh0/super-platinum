use super::*;
use std::sync::Arc;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{Transport, models::UserProfile},
};

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let user = shell.core.workspaces["T1"].self_user_id.clone();
    shell.self_account.user_id = user.clone();
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: user,
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: "synthetic".into(),
            },
        )]
        .into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    shell
}
fn response() -> UserProfile {
    serde_json::from_value(serde_json::json!({"status_text":"","status_emoji":"","status_expiration":0,"real_name":"Captured account"})).unwrap()
}

#[test]
fn until_morning_uses_the_next_8am_in_the_local_offset() {
    assert_eq!(minutes_until_local_hour(10 * 3_600, 0, 8), 22 * 60);
    assert_eq!(minutes_until_local_hour(7 * 3_600, 0, 8), 60);
}

#[test]
fn self_menu_completion_rejects_replaced_account_viewer_session_or_transport() {
    let changes: [fn(&mut ShellState); 8] = [
        |s| s.core.account_epoch += 1,
        |s| s.core.active_team = Some("T_OTHER".into()),
        |s| s.core.transport = Some(Arc::new(Transport::new("replacement").unwrap())),
        |s| s.self_account.user_id = "U_OTHER".into(),
        |s| s.core.workspaces.get_mut("T1").unwrap().self_user_id = "U_OTHER".into(),
        |s| {
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token = "replacement".into()
        },
        |s| {
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .user_id = "U_OTHER".into()
        },
        |s| s.core.session = None,
    ];
    for change in changes {
        let mut shell = shell();
        let context = SelfMenuContext::begin(&mut shell, MutationKind::Status).unwrap();
        let user = context.0.session.user_id.clone();
        let before =
            serde_json::to_value(&shell.core.workspaces["T1"].users[&user].profile).unwrap();
        change(&mut shell);
        assert!(!context.current(&shell)); // Same guard suppresses errors and DND results.
        context.apply_profile(&mut shell, response());
        assert_eq!(
            serde_json::to_value(&shell.core.workspaces["T1"].users[&user].profile).unwrap(),
            before
        );
        assert!(shell.toast.is_none());
    }
}

#[test]
fn pause_resume_and_newer_presence_status_intents_retire_old_completions() {
    for kind in [
        MutationKind::Dnd,
        MutationKind::Presence,
        MutationKind::Status,
    ] {
        let mut shell = shell();
        let old = SelfMenuContext::begin(&mut shell, kind).unwrap();
        let newest = SelfMenuContext::begin(&mut shell, kind).unwrap();
        assert!(!old.current(&shell));
        assert!(newest.current(&shell));
    }
    let mut shell = shell();
    let dnd = SelfMenuContext::begin(&mut shell, MutationKind::Dnd).unwrap();
    let status = SelfMenuContext::begin(&mut shell, MutationKind::Status).unwrap();
    let presence = SelfMenuContext::begin(&mut shell, MutationKind::Presence).unwrap();
    assert!(dnd.current(&shell));
    assert!(status.current(&shell));
    assert!(presence.current(&shell));
    shell.clear_self_status();
    let user = &status.0.session.user_id;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut(user)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .real_name = Some("New realtime name".into());
    status.apply_profile(&mut shell, response());
    let profile = shell.core.workspaces["T1"].users[user]
        .profile
        .as_ref()
        .unwrap();
    assert_eq!(profile.status_text.as_deref(), Some(""));
    assert_eq!(profile.real_name.as_deref(), Some("New realtime name"));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut(user)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .status_text = Some("New realtime status".into());
    status.apply_profile(&mut shell, response());
    assert_eq!(
        shell.core.workspaces["T1"].users[user]
            .profile
            .as_ref()
            .unwrap()
            .status_text
            .as_deref(),
        Some("New realtime status")
    );
}
