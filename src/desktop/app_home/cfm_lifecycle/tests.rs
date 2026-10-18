use super::*;
use serde_json::json;
use super_platinum_core::{config::Session, slack::Transport, state::Workspace};
fn setup() -> ShellState {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let session: Session = serde_json::from_value(json!({
        "d_cookie":"synthetic-cookie", "workspaces":{"TOWNER":{
            "team_id":"TOWNER","user_id":"U00000009","name":"Fixture",
            "token":"synthetic-token","url":"https://fixture.slack.com"
        }}
    }))
    .unwrap();
    shell.core.workspaces.insert(
        "TOWNER".into(),
        Workspace::from_session(&session.workspaces["TOWNER"]),
    );
    shell.core.session = Some(session);
    shell.core.active_account = Some("A".into());
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture(
        "synthetic-cookie",
    )));
    shell
}
#[test]
fn accepted_sockets_are_counted_independently_of_failed_attempt_generations() {
    let mut shell = setup();
    let zero = capture(&mut shell, "TOWNER").unwrap();
    assert_eq!(zero.slow_connections, 0);
    assert_eq!(zero.major, 1);
    shell
        .core
        .workspaces
        .get_mut("TOWNER")
        .unwrap()
        .rt_generation = 4;
    // A failed attempt produces no connected hook.
    assert_eq!(capture(&mut shell, "TOWNER").unwrap().slow_connections, 0);
    connected(&mut shell, "TOWNER", 4);
    let first = capture(&mut shell, "TOWNER").unwrap();
    assert_eq!(first.slow_connections, 1);
    assert!(!zero.current(&shell));
    connected(&mut shell, "TOWNER", 4);
    connected(&mut shell, "TOWNER", 3);
    assert!(first.current(&shell));
    shell
        .core
        .workspaces
        .get_mut("TOWNER")
        .unwrap()
        .rt_generation = 9;
    connected(&mut shell, "TOWNER", 9);
    assert_eq!(capture(&mut shell, "TOWNER").unwrap().slow_connections, 2);
    assert!(!first.current(&shell));
}
#[test]
fn cfm_lifecycle_has_no_authority_across_credentials_transport_or_account_epoch() {
    for change in 0..3 {
        let mut shell = setup();
        shell
            .core
            .workspaces
            .get_mut("TOWNER")
            .unwrap()
            .rt_generation = 1;
        connected(&mut shell, "TOWNER", 1);
        let old = capture(&mut shell, "TOWNER").unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("TOWNER")
                .unwrap()
                .token
                .push('x'),
            _ => {
                shell.core.transport = Some(Arc::new(Transport::disconnected_fixture(
                    "synthetic-cookie",
                )))
            }
        }
        assert!(!old.current(&shell));
        assert_eq!(capture(&mut shell, "TOWNER").unwrap().slow_connections, 0);
    }
}

#[test]
fn original_boot_completion_cannot_clear_new_attempt_or_changed_owner() {
    let mut shell = setup();
    let old = boot_started(&mut shell, "TOWNER").unwrap();
    let key = old.context.key();
    let new = boot_started(&mut shell, "TOWNER").unwrap();
    boot_settled(&mut shell, Some(old));
    assert!(
        shell.app_home.cfm_lifecycle.entries[&key]
            .boot
            .as_ref()
            .is_some_and(|t| Arc::ptr_eq(t, &new.token))
    );
    boot_settled(&mut shell, Some(new));
    assert!(shell.app_home.cfm_lifecycle.entries[&key].boot.is_none());
    let stale = boot_started(&mut shell, "TOWNER").unwrap();
    shell.core.account_epoch += 1;
    let current = boot_started(&mut shell, "TOWNER").unwrap();
    boot_settled(&mut shell, Some(stale));
    assert!(
        shell.app_home.cfm_lifecycle.entries[&key]
            .boot
            .as_ref()
            .is_some_and(|t| Arc::ptr_eq(t, &current.token))
    );
    boot_settled(&mut shell, Some(current));
    assert!(shell.app_home.cfm_lifecycle.entries[&key].boot.is_none());
}
