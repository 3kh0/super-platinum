use super::*;

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("quip-title-fixture".into());
    shell.core.active_team = Some("T1".into());
    shell.core.account_epoch = 7;
    let user = shell.core.workspaces["T1"].self_user_id.clone();
    assert!(!user.is_empty());
    shell.self_account.user_id = user;
    shell
}

#[test]
fn explicit_fresh_runtime_observes_absence_but_default_does_not() {
    let mut shell = shell();
    let observed = Observation::capture(&shell, "FTITLE1").unwrap();
    assert!(observed.current(&shell));
    assert!(matches!(observed.value(), QuipTitle::Nullish));
    shell.canvas_titles = State::default();
    assert!(Observation::capture(&shell, "FTITLE1").is_none());
    assert!(!observed.current(&shell));
}

#[test]
fn replacement_shell_or_runtime_retires_identical_owner_and_file() {
    let mut original = shell();
    let observed = Observation::capture(&original, "FTITLE1").unwrap();
    let replacement = shell();
    assert!(!observed.current(&replacement));
    original.canvas_titles = State::fresh();
    assert!(!observed.current(&original));
    assert!(
        Observation::capture(&original, "FTITLE1")
            .unwrap()
            .current(&original)
    );
}

#[test]
fn account_epoch_team_and_self_changes_retire_observation() {
    for change in 0..5 {
        let mut shell = shell();
        let observed = Observation::capture(&shell, "FTITLE1").unwrap();
        match change {
            0 => shell.core.active_account = Some("other-account".into()),
            1 => shell.core.account_epoch += 1,
            2 => {
                let session = super_platinum_core::config::WorkspaceSession {
                    team_id: "T2".into(),
                    user_id: shell.self_account.user_id.clone(),
                    enterprise_id: None,
                    name: "Second fixture workspace".into(),
                    url: "https://fixture.invalid".into(),
                    token: "synthetic".into(),
                };
                shell.core.workspaces.insert(
                    session.team_id.clone(),
                    super_platinum_core::state::Workspace::from_session(&session),
                );
                shell.core.active_team = Some("T2".into());
                shell.self_account.user_id = shell.core.workspaces["T2"].self_user_id.clone();
            }
            3 => {
                shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "UOTHER".into();
                shell.self_account.user_id = "UOTHER".into();
            }
            _ => shell.self_account.user_id = "inconsistent-self".into(),
        }
        assert!(!observed.current(&shell), "owner change {change}");
    }
}

#[test]
fn absent_owner_or_file_never_observes_nullish() {
    let mut shell = shell();
    assert!(Observation::capture(&shell, "").is_none());
    shell.core.active_account = None;
    assert!(Observation::capture(&shell, "FTITLE1").is_none());
    shell.core.active_account = Some("quip-title-fixture".into());
    shell.core.active_team = Some("missing-workspace".into());
    assert!(Observation::capture(&shell, "FTITLE1").is_none());
}

#[test]
fn file_observations_are_distinct_and_independent_of_profile_visits_and_socket() {
    let mut shell = shell();
    let first = Observation::capture(&shell, "FTITLE1").unwrap();
    let second = Observation::capture(&shell, "FTITLE2").unwrap();
    assert_eq!(first.file, "FTITLE1");
    assert_eq!(second.file, "FTITLE2");
    assert!(Arc::ptr_eq(&first.runtime, &second.runtime));
    for _ in 0..3 {
        shell.profile_generation = shell.profile_generation.wrapping_add(1);
        shell.profile_edit = Default::default();
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
        assert!(first.current(&shell));
        assert!(second.current(&shell));
        let reopened = Observation::capture(&shell, "FTITLE1").unwrap();
        assert!(Arc::ptr_eq(&first.runtime, &reopened.runtime));
        assert!(matches!(reopened.value(), QuipTitle::Nullish));
    }
}
