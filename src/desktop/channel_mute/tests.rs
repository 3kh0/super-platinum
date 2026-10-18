use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000077".into());
    shell.core.active_team = Some("T1".into());
    shell.core.active_channel = Some("C2".into());
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    workspace
        .channels
        .get_mut("C2")
        .unwrap()
        .extra
        .insert("is_member".into(), json!(true));
    workspace.notifications.apply_change("all_notifications_prefs",json!({"global":{"sound":"keep-global"},"channels":{"C2":{"muted":false,"desktop":"mentions","unknown":{"keep":true}},"C3":{"muted":true}}}));
    shell
}
fn token(shell: &ShellState) -> Token {
    Token::capture(shell, "C2").unwrap()
}
fn prefs(muted: bool, expiry: Option<i64>) -> api::NotificationPreferences {
    api::parse_notification_preferences(&json!({"ok":true,"prefs":{"all_notifications_prefs":{"global":{"sound":"remote-other"},"channels":{"C2":{"muted":muted,"temp_mute_expiration_ts":expiry,"desktop":"remote-other"},"C3":{"muted":false}}}}})).unwrap()
}
fn started(shell: &mut ShellState) -> Attempt {
    let token = token(shell);
    let attempt = begin(shell, token, 100).unwrap();
    assert!(mark_sending(shell, &attempt));
    attempt
}
#[test]
fn duplicate_stale_visit_and_cross_account_actions_cannot_dispatch() {
    let mut shell = shell();
    let old = token(&shell);
    let attempt = started(&mut shell);
    assert!(begin(&mut shell, old.clone(), 100).is_none());
    assert!(!mark_sending(&mut shell, &attempt));
    shell.channel_generation += 1;
    assert!(!old.current(&shell));
    assert!(begin(&mut shell, old, 100).is_none());
    shell.core.active_account = Some("another-account".into());
    assert!(!attempt.token.current(&shell));
    assert!(!finish_write(&mut shell, &attempt, Ok(()), 100));
    assert!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .is_some()
    );
    shell.core.active_account = Some(attempt.token.owner.account.clone());
    let reopened = token(&shell);
    assert!(begin(&mut shell, reopened.clone(), 100).is_none());
    let read = begin_check(&mut shell, reopened, 100).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(prefs(true, None)), 100));
}
#[test]
fn canonical_readback_adopts_only_target_mute_and_preserves_other_preferences() {
    let mut shell = shell();
    let attempt = started(&mut shell);
    assert!(!shell.core.workspaces["T1"].notifications.muted("C2", 100));
    assert!(finish_write(&mut shell, &attempt, Ok(()), 100));
    let token = token(&shell);
    let read = begin_check(&mut shell, token, 100).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(prefs(true, None)), 100));
    let w = &shell.core.workspaces["T1"];
    assert!(w.notifications.muted("C2", 100));
    assert!(w.sidebar.muted.contains("C2"));
    assert_eq!(w.notifications.channels["C2"]["desktop"], "mentions");
    assert_eq!(
        w.notifications.channels["C2"]["unknown"],
        json!({"keep":true})
    );
    assert_eq!(w.notifications.global["sound"], "keep-global");
    assert!(w.notifications.muted("C3", 100));
}
#[test]
fn unknown_write_never_replays_and_readback_mismatch_retains_lock() {
    let mut shell = shell();
    let attempt = started(&mut shell);
    assert!(finish_write(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost response".into())),
        100
    ));
    let first = token(&shell);
    let read = begin_check(&mut shell, first.clone(), 100).unwrap();
    assert!(!finish_read(&mut shell, &read, Ok(prefs(false, None)), 100));
    let current = token(&shell);
    assert!(begin(&mut shell, current, 100).is_none());
    let read = begin_check(&mut shell, first, 100).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(prefs(true, None)), 100));
    assert!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .is_none()
    );
}
#[test]
fn explicit_offline_and_known_rejection_unlock_but_internal_error_does_not() {
    for error in [
        Error::Offline("offline".into()),
        Error::Api("not_in_channel".into()),
        Error::Api("internal_error".into()),
    ] {
        let mut shell = shell();
        let attempt = started(&mut shell);
        let known = known_rejection(&error);
        finish_write(&mut shell, &attempt, Err(error), 100);
        assert_eq!(
            shell.channel_mute.entries[&attempt.token.owner]
                .operation
                .is_none(),
            known
        );
    }
}
#[test]
fn late_read_cannot_adopt_after_rotation_or_override_newer_realtime_preferences() {
    for rotate in [false, true] {
        let mut shell = shell();
        let attempt = started(&mut shell);
        finish_write(&mut shell, &attempt, Ok(()), 100);
        let old = token(&shell);
        let read = begin_check(&mut shell, old, 100).unwrap();
        if rotate {
            shell.core.account_epoch += 1;
        } else {
            shell
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .notifications
                .channels
                .get_mut("C2")
                .unwrap()
                .insert("desktop".into(), json!("newer"));
        }
        assert!(!finish_read(&mut shell, &read, Ok(prefs(true, None)), 100));
        assert!(!shell.core.workspaces["T1"].notifications.muted("C2", 100));
        assert!(
            shell.channel_mute.entries[&attempt.token.owner]
                .operation
                .is_some()
        );
        let token = token(&shell);
        let fresh = begin_check(&mut shell, token, 100).unwrap();
        assert!(finish_read(&mut shell, &fresh, Ok(prefs(true, None)), 100));
    }
}
#[test]
fn temporary_unmute_requires_actual_effective_readback_and_respects_retry_after() {
    let mut shell = shell();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .channels
        .get_mut("C2")
        .unwrap()
        .insert("temp_mute_expiration_ts".into(), json!(500));
    let attempt = started(&mut shell);
    assert!(!attempt.desired);
    finish_write(
        &mut shell,
        &attempt,
        Err(Error::RateLimited {
            retry_after_secs: Some(20),
        }),
        100,
    );
    let token = token(&shell);
    assert!(begin_check(&mut shell, token.clone(), 119).is_none());
    let read = begin_check(&mut shell, token.clone(), 120).unwrap();
    assert!(!finish_read(
        &mut shell,
        &read,
        Ok(prefs(false, Some(500))),
        120
    ));
    let read = begin_check(&mut shell, token, 501).unwrap();
    assert!(finish_read(
        &mut shell,
        &read,
        Ok(prefs(false, Some(500))),
        501
    ));
    assert!(!shell.core.workspaces["T1"].sidebar.muted.contains("C2"));
}

#[test]
fn retired_read_and_preparing_lease_cannot_unlock_a_later_attempt() {
    let mut shell = shell();
    let attempt = started(&mut shell);
    finish_write(&mut shell, &attempt, Ok(()), 100);
    let token = token(&shell);
    let old = begin_check(&mut shell, token.clone(), 100).unwrap();
    finish_read(
        &mut shell,
        &old,
        Err(Error::Transport("read failed".into())),
        100,
    );
    let new = begin_check(&mut shell, token, 100).unwrap();
    assert!(!finish_read(&mut shell, &old, Ok(prefs(true, None)), 100));
    assert_eq!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .as_ref()
            .unwrap()
            .read_id,
        Some(new.id)
    );
    assert!(finish_read(&mut shell, &new, Ok(prefs(true, None)), 100));
    assert!(!finish_write(&mut shell, &attempt, Ok(()), 100));
    let token = Token::capture(&shell, "C2").unwrap();
    let next = begin(&mut shell, token, 100).unwrap();
    shell.core.account_epoch += 1;
    assert!(!mark_sending(&mut shell, &next));
    assert!(
        shell.channel_mute.entries[&next.token.owner]
            .operation
            .is_none()
    );
}

#[test]
fn queued_action_rejects_changed_session_identity_even_with_same_token_and_epoch() {
    use super_platinum_core::config::{Session, WorkspaceSession};
    for change_user in [true, false] {
        let mut shell = shell();
        shell.core.session = Some(Session {
            unified_user_client_enabled: Default::default(),
            d_cookie: "synthetic".into(),
            workspaces: [(
                "T1".into(),
                WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "same-synthetic-token".into(),
                },
            )]
            .into(),
        });
        let old = token(&shell);
        let session = shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap();
        if change_user {
            session.user_id = "another-user".into();
        } else {
            session.team_id = "another-team".into();
        }
        assert!(!old.current(&shell));
        assert!(begin(&mut shell, old, 100).is_none());
        assert!(shell.channel_mute.entries.is_empty());
    }
}

#[test]
fn expiry_tick_updates_sidebar_once_without_writing_or_settling_operations() {
    let mut shell = shell();
    let attempt = started(&mut shell);
    assert!(finish_write(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost response".into())),
        100
    ));
    adopt(
        &mut shell,
        &attempt.token.owner,
        &prefs(false, Some(120)),
        100,
    );
    let original = serde_json::to_value(&shell.core.workspaces["T1"].notifications).unwrap();
    assert!(shell.core.workspaces["T1"].sidebar.is_muted("C2"));
    assert!(!poll_at(&mut shell, 119));
    assert!(poll_at(&mut shell, 120));
    assert!(!shell.core.workspaces["T1"].sidebar.is_muted("C2"));
    assert!(shell.core.workspaces["T1"].sidebar.is_muted("C3"));
    assert!(!poll_at(&mut shell, 121));
    assert_eq!(
        serde_json::to_value(&shell.core.workspaces["T1"].notifications).unwrap(),
        original
    );
    assert!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .is_some()
    );
}

#[test]
fn temporary_intent_retains_exact_deadline_and_mismatched_readback_cannot_settle() {
    let mut shell = shell();
    let captured = token(&shell);
    let attempt = begin_temporary(
        &mut shell,
        captured.clone(),
        api::TemporaryMuteDuration::OneHour,
        100,
    )
    .unwrap();
    assert_eq!(attempt.change, api::ChannelMuteChange::TemporaryUntil(3700));
    assert!(
        begin_temporary(
            &mut shell,
            captured,
            api::TemporaryMuteDuration::OneHour,
            101
        )
        .is_none()
    );
    assert!(mark_sending(&mut shell, &attempt));
    assert!(!mark_sending(&mut shell, &attempt));
    assert!(finish_write(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost receipt".into())),
        101
    ));
    let current = token(&shell);
    let read = begin_check(&mut shell, current, 101).unwrap();
    assert!(!finish_read(
        &mut shell,
        &read,
        Ok(prefs(false, Some(3699))),
        101
    ));
    let current = token(&shell);
    let read = begin_check(&mut shell, current, 102).unwrap();
    assert!(finish_read(
        &mut shell,
        &read,
        Ok(prefs(false, Some(3700))),
        102
    ));
    assert!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .is_none()
    );
    assert_eq!(
        shell.core.workspaces["T1"].notifications.channels["C2"]["unknown"],
        json!({"keep":true})
    );
}

#[test]
fn tomorrow_requires_known_schedule_and_temporary_actions_reject_stale_state() {
    let mut shell = shell();
    let captured = token(&shell);
    assert!(
        begin_temporary(
            &mut shell,
            captured.clone(),
            api::TemporaryMuteDuration::UntilTomorrow,
            100
        )
        .is_none()
    );
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .apply_prefs(&Default::default());
    let attempt = begin_temporary(
        &mut shell,
        captured,
        api::TemporaryMuteDuration::UntilTomorrow,
        100,
    )
    .unwrap();
    assert!(matches!(attempt.change,api::ChannelMuteChange::TemporaryUntil(until) if until>100));
    assert!(attempt.schedule.is_some());
    shell.channel_generation += 1;
    assert!(!mark_sending(&mut shell, &attempt));
}

#[test]
fn temporary_mute_replaces_permanent_mute_with_only_expiry_write_and_exact_readback() {
    use super_platinum_core::{
        config::WorkspaceSession,
        slack::{SlackClient, client::RequestBody},
    };
    let mut shell = shell();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .channels
        .get_mut("C2")
        .unwrap()
        .insert("muted".into(), json!(true));
    assert!(presentation(&shell, "C2").unwrap().can_temporary);
    let captured = token(&shell);
    let attempt = begin_temporary(
        &mut shell,
        captured,
        api::TemporaryMuteDuration::OneHour,
        100,
    )
    .unwrap();
    let session = WorkspaceSession {
        team_id: "T1".into(),
        user_id: attempt.token.owner.user.clone(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-only".into(),
    };
    let request =
        api::channel_mute_change_request(&SlackClient::default(), &session, "C2", attempt.change)
            .unwrap();
    let RequestBody::Form(fields) = request.body else {
        panic!("expected form")
    };
    let field = |key: &str| {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(field("channel_ids"), Some("C2"));
    let wire: Value = serde_json::from_str(field("prefs").unwrap()).unwrap();
    assert_eq!(
        wire,
        json!([{"name":"temp_mute_expiration_ts","value":"3700"}])
    );
    assert!(mark_sending(&mut shell, &attempt));
    assert!(finish_write(&mut shell, &attempt, Ok(()), 101));
    let current = token(&shell);
    let read = begin_check(&mut shell, current, 101).unwrap();
    assert!(!finish_read(
        &mut shell,
        &read,
        Ok(prefs(true, Some(3700))),
        101
    ));
    let current = token(&shell);
    let read = begin_check(&mut shell, current, 102).unwrap();
    assert!(finish_read(
        &mut shell,
        &read,
        Ok(prefs(false, Some(3700))),
        102
    ));
    let config = &shell.core.workspaces["T1"].notifications;
    assert_eq!(config.channels["C2"]["muted"], json!(false));
    assert_eq!(config.temporary_mute_expiration("C2"), Some(3700));
    assert_eq!(config.channels["C2"]["unknown"], json!({"keep":true}));
    assert!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .is_none()
    );
}

#[test]
fn remote_mute_after_settled_unmute_hides_old_success_but_keeps_uncertain_guidance() {
    let mut shell = shell();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .channels
        .get_mut("C2")
        .unwrap()
        .insert("muted".into(), json!(true));
    let attempt = started(&mut shell);
    assert!(finish_write(&mut shell, &attempt, Ok(()), 100));
    let current = token(&shell);
    let read = begin_check(&mut shell, current, 100).unwrap();
    assert!(finish_read(&mut shell, &read, Ok(prefs(false, None)), 100));
    assert_eq!(
        presentation(&shell, "C2").unwrap().message.as_deref(),
        Some("Channel unmuted.")
    );
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .channels
        .get_mut("C2")
        .unwrap()
        .insert("temp_mute_expiration_ts".into(), json!(now() + 3600));
    let view = presentation(&shell, "C2").unwrap();
    assert_eq!(view.label, "Unmute channel");
    assert_eq!(view.message, None);
    let next = started(&mut shell);
    assert!(finish_write(
        &mut shell,
        &next,
        Err(Error::Transport("lost".into())),
        101
    ));
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .channels
        .get_mut("C2")
        .unwrap()
        .insert("muted".into(), json!(true));
    let view = presentation(&shell, "C2").unwrap();
    assert!(view.uncertain);
    assert!(view.message.is_some());
}

#[test]
fn preference_rate_limit_follows_authentication_across_channels_and_never_shortens() {
    let mut shell = shell();
    let now = now();
    poll_at(&mut shell, now);
    let attempt = started(&mut shell);
    finish_write(
        &mut shell,
        &attempt,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
        now,
    );
    shell.core.active_channel = Some("C1".into());
    shell.channel_generation += 1;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C1")
        .unwrap()
        .extra
        .insert("is_member".into(), json!(true));
    let other = Token::capture(&shell, "C1").unwrap();
    assert!(begin(&mut shell, other.clone(), now + 1).is_none());
    assert!(!presentation(&shell, "C1").unwrap().can_change);
    remember_cooldown(
        &mut shell,
        &other,
        &Error::RateLimited {
            retry_after_secs: Some(1),
        },
        now + 2,
    );
    assert_eq!(shared_retry_at(&shell, &other), now + 60);
    remember_cooldown(
        &mut shell,
        &other,
        &Error::HttpStatus {
            status: 429,
            retry_after_secs: Some(90),
        },
        now + 3,
    );
    assert_eq!(shared_retry_at(&shell, &attempt.token), now + 93);
    assert!(!poll_at(&mut shell, now + 59));
    assert!(poll_at(&mut shell, now + 93));
    assert_eq!(shared_retry_at(&shell, &other), 0);
    assert!(begin(&mut shell, other, now + 93).is_some());
    // The original possibly dispatched mutation remains unresolved after expiry.
    assert!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .is_some()
    );
}

#[test]
fn preflight_and_read_limits_block_other_channels_but_not_other_auth_owners() {
    let mut shell = shell();
    let now = now();
    let captured = token(&shell);
    let attempt = begin(&mut shell, captured, now).unwrap();
    preflight_failed(
        &mut shell,
        &attempt,
        Some(&Error::RateLimited {
            retry_after_secs: Some(30),
        }),
        now,
    );
    assert!(
        shell.channel_mute.entries[&attempt.token.owner]
            .operation
            .is_none()
    );
    shell.core.active_channel = Some("C1".into());
    shell.channel_generation += 1;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .get_mut("C1")
        .unwrap()
        .extra
        .insert("is_member".into(), json!(true));
    let other = Token::capture(&shell, "C1").unwrap();
    assert!(begin(&mut shell, other, now + 1).is_none());
    shell.core.active_account = Some("another-account".into());
    let fresh = Token::capture(&shell, "C1").unwrap();
    assert_eq!(shared_retry_at(&shell, &fresh), 0);
    let next = begin(&mut shell, fresh, now + 1).unwrap();
    assert!(mark_sending(&mut shell, &next));
    finish_write(&mut shell, &next, Ok(()), now + 1);
    let fresh = Token::capture(&shell, "C1").unwrap();
    let read = begin_check(&mut shell, fresh, now + 1).unwrap();
    finish_read(
        &mut shell,
        &read,
        Err(Error::RateLimited {
            retry_after_secs: Some(40),
        }),
        now + 1,
    );
    shell.core.active_channel = Some("C2".into());
    shell.channel_generation += 1;
    let fresh = token(&shell);
    assert!(begin(&mut shell, fresh, now + 2).is_none());
}

#[test]
fn queued_write_rechecks_shared_cooldown_before_marking_sent() {
    let mut shell = shell();
    let now = now();
    let captured = token(&shell);
    let attempt = begin(&mut shell, captured.clone(), now).unwrap();
    remember_cooldown(
        &mut shell,
        &captured,
        &Error::RateLimited {
            retry_after_secs: Some(60),
        },
        now,
    );
    assert!(!mark_sending(&mut shell, &attempt));
    assert!(
        shell.channel_mute.entries[&captured.owner]
            .operation
            .is_none()
    );
    assert!(shared_retry_at(&shell, &captured) > now);
}
