use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    shell
}
fn edit(shell: &mut ShellState, field: Field, text: &str) -> u64 {
    let draft = shell.profile_edit.draft.as_mut().unwrap();
    draft.values.0[index(field)] = profile_text::normalize(text);
    draft.visit
}
fn start(shell: &mut ShellState, field: Field, text: &str) -> Attempt {
    let visit = edit(shell, field, text);
    let attempt = begin(shell, visit).unwrap();
    fixture::save(shell, attempt.clone());
    attempt
}
#[test]
fn changed_text_only_preserves_status_unknowns_and_cancel_writes_nothing() {
    let mut shell = shell();
    let visit = edit(&mut shell, Field::DisplayName, "Cancelled");
    close_visit(&mut shell, Some(visit));
    assert!(
        shell
            .profile_edit
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    let (token, visit) = open_shell(&mut shell).unwrap();
    fixture::load(&mut shell, &token, visit);
    let before =
        serde_json::to_value(&shell.profile_edit.fixture.as_ref().unwrap().profile).unwrap();
    let attempt = start(&mut shell, Field::DisplayName, "Jose\u{301}");
    assert_eq!(attempt.patch, json!({"display_name":"José"}));
    assert!(begin(&mut shell, visit).is_none());
    fixture::complete(&mut shell);
    assert!(shell.profile_edit.draft.is_none());
    let profile = token.owner.values(&shell);
    assert_eq!(profile.get(Field::DisplayName), "José");
    let actual = serde_json::to_value(
        shell.core.workspaces[&token.owner.team].users[&token.owner.user]
            .profile
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    for key in [
        "status_text",
        "status_emoji",
        "status_expiration",
        "unknown_fixture",
        "title",
    ] {
        assert_eq!(actual[key], before[key]);
    }
    assert!(shell.profile_edit.fixture.as_ref().unwrap().valid);
}
#[test]
fn fresh_permission_revocation_and_realtime_profile_drift_send_nothing() {
    let mut shell = shell();
    let visit = edit(&mut shell, Field::DisplayName, "Next");
    let attempt = begin(&mut shell, visit).unwrap();
    shell.profile_edit.fixture.as_mut().unwrap().allow_display = false;
    fixture::save(&mut shell, attempt);
    assert!(
        shell
            .profile_edit
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    let owner = Owner::capture(&shell).unwrap();
    shell
        .core
        .workspaces
        .get_mut(&owner.team)
        .unwrap()
        .users
        .get_mut(&owner.user)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .display_name = Some("Realtime".into());
    assert!(begin(&mut shell, visit).is_none());
}
#[test]
fn uncertain_close_reopen_and_new_epoch_retains_lock_without_replay() {
    let mut shell = shell();
    shell.profile_edit.fixture.as_mut().unwrap().uncertain = true;
    let attempt = start(&mut shell, Field::Pronouns, "she/her");
    fixture::complete(&mut shell);
    assert!(blocks_status(&shell));
    close_visit(&mut shell, Some(attempt.visit));
    assert!(shell.profile_edit.draft.is_none());
    shell.core.account_epoch += 1;
    assert!(open_shell(&mut shell).is_none());
    let visit = shell.profile_edit.draft.as_ref().unwrap().visit;
    assert!(begin(&mut shell, visit).is_none());
    let (token, id, read) = begin_check(&mut shell, visit).unwrap();
    fixture::check(&mut shell, token, id, read);
    assert!(!blocks_status(&shell));
    assert!(shell.profile_edit.draft.is_none());
    assert_eq!(
        shell.profile_edit.fixture.as_ref().unwrap().attempts.len(),
        1
    );
}
#[test]
fn stale_preflight_and_late_old_account_completion_never_modify_new_owner() {
    let mut shell = shell();
    let visit = edit(&mut shell, Field::FullName, "Next Person");
    let attempt = begin(&mut shell, visit).unwrap();
    shell.core.account_epoch += 1;
    let fresh = fixture::capabilities(&shell, &attempt.token.owner);
    assert!(!preflight(&mut shell, &attempt, &fresh));
    assert!(
        shell
            .profile_edit
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    poll(&mut shell);
    let (token, visit) = open_shell(&mut shell).unwrap();
    fixture::load(&mut shell, &token, visit);
    let attempt = start(&mut shell, Field::FullName, "Saved Person");
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
    poll(&mut shell);
    fixture::complete(&mut shell);
    assert_eq!(
        attempt.token.owner.values(&shell).get(Field::FullName),
        "Original Person"
    );
    assert!(
        shell.profile_edit.entries[&attempt.token.owner]
            .pending
            .is_some()
    );
}
#[test]
fn read_mismatch_never_authorizes_retry_and_old_read_cannot_settle_new_owner() {
    let mut shell = shell();
    let attempt = start(&mut shell, Field::Pronouns, "");
    assert!(finish_write(&mut shell, &attempt, Ok(())));
    let (token, id, read) = begin_check(&mut shell, attempt.visit).unwrap();
    let different = shell.profile_edit.fixture.as_ref().unwrap().profile.clone();
    finish_check(&mut shell, &token, id, read, Ok(different));
    let view = presentation(&shell).unwrap();
    assert!(view.can_retire);
    assert!(!view.can_save);
    assert!(begin(&mut shell, attempt.visit).is_none());
    let (token, id, read) = begin_check(&mut shell, attempt.visit).unwrap();
    shell.core.account_epoch += 1;
    poll(&mut shell);
    let mut desired = shell.profile_edit.fixture.as_ref().unwrap().profile.clone();
    desired.pronouns = Some(String::new());
    finish_check(&mut shell, &token, id, read, Ok(desired));
    assert!(shell.profile_edit.entries[&token.owner].pending.is_some());
    assert_eq!(token.owner.values(&shell).get(Field::Pronouns), "they/them");
}
#[test]
fn known_rejection_and_cooldown_preserve_draft_and_reopening_cannot_bypass() {
    let mut shell = shell();
    let attempt = start(&mut shell, Field::FullName, "New Person");
    finish_write(
        &mut shell,
        &attempt,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(!blocks_status(&shell));
    assert!(!presentation(&shell).unwrap().can_save);
    close_visit(&mut shell, Some(attempt.visit));
    assert!(open_shell(&mut shell).is_none());
    assert!(!shell.profile_edit.draft.as_ref().unwrap().loading);
    assert!(
        shell
            .profile_edit
            .draft
            .as_ref()
            .unwrap()
            .capabilities
            .is_none()
    );
}

#[test]
fn profile_and_status_mutations_serialize_in_both_directions() {
    let mut shell = shell();
    let attempt = start(&mut shell, Field::DisplayName, "Pending name");
    crate::self_status::open_shell(&mut shell, now());
    assert!(crate::self_status::begin(&mut shell, now()).is_err());
    assert!(!crate::self_status::prepare_clear(&mut shell, now()));
    assert!(
        shell.profile_edit.entries[&attempt.token.owner]
            .pending
            .is_some()
    );

    let mut shell = self::shell();
    let owner = Owner::capture(&shell).unwrap();
    let session = fixture::session(&owner);
    shell.core.session = Some(super_platinum_core::config::Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "fixture-cookie".into(),
        workspaces: [(owner.team.clone(), session)].into(),
    });
    shell.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("fixture-cookie").unwrap(),
    ));
    crate::self_status::open_shell(&mut shell, now());
    let _status = crate::self_status::begin(&mut shell, now()).unwrap();
    let (token, visit) = open_shell(&mut shell).unwrap();
    let cap = fixture::capabilities(&shell, &owner);
    finish_load(&mut shell, &token, visit, Ok(cap));
    edit(&mut shell, Field::DisplayName, "Blocked name");
    assert!(presentation(&shell).unwrap().status_conflict);
    assert!(!presentation(&shell).unwrap().can_save);
    assert!(begin(&mut shell, visit).is_none());
}
