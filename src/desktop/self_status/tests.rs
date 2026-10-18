use super::*;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{Transport, client::RequestBody},
};
const NOW: i64 = 1_800_000_000;
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.self_account.user_id = user.clone();
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic".into(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team,
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
    open_shell(&mut shell, NOW);
    shell
}
fn edit(shell: &mut ShellState, text: &str, emoji: &str, expiration: Expiration) {
    let draft = shell.self_status.draft.as_mut().unwrap();
    draft.text = text.into();
    draft.emoji = emoji.into();
    draft.expiration = expiration;
}
fn response(status: &Status) -> UserProfile {
    let mut p = UserProfile::default();
    status.merge(&mut p);
    p
}

#[test]
fn presets_and_custom_expiration_use_future_local_instants() {
    for (choice, seconds) in [
        (Expiration::ThirtyMinutes, 1800),
        (Expiration::OneHour, 3600),
        (Expiration::FourHours, 14400),
    ] {
        assert_eq!(choice.timestamp("", NOW), Ok(NOW + seconds));
        assert_eq!(Expiration::from_token(choice.token()), Some(choice));
    }
    assert_eq!(Expiration::Never.timestamp("", NOW), Ok(0));
    let today = Expiration::Today.timestamp("", NOW).unwrap();
    let local = super_platinum_core::block_actions::form::datetime_local_value(Some(today));
    assert!(local.ends_with("T23:59"));
    assert!(today > NOW && today - NOW <= 90_000);
    let future = super_platinum_core::block_actions::form::datetime_local_value(Some(NOW + 7200));
    assert!(Expiration::Custom.timestamp(&future, NOW).is_ok());
    for invalid in ["", "not-a-date", "2024-02-30T12:00", "2000-01-01T00:00"] {
        assert!(Expiration::Custom.timestamp(invalid, NOW).is_err());
    }
}

#[test]
fn status_request_preserves_text_and_defaults_text_only_emoji() {
    let mut shell = shell();
    edit(&mut shell, "Shipping 日本語 🦊", "", Expiration::OneHour);
    let attempt = begin(&mut shell, NOW).unwrap();
    assert_eq!(attempt.profile["status_text"], "Shipping 日本語 🦊");
    assert_eq!(attempt.profile["status_emoji"], ":speech_balloon:");
    assert_eq!(attempt.profile["status_expiration"], NOW + 3600);
    let context = attempt.context.as_ref().unwrap().request();
    let request = prepare(&attempt, &context.client, &context.session);
    assert!(!request.retry_safe());
    let RequestBody::Form(fields) = request.body else {
        panic!("form");
    };
    assert_eq!(
        serde_json::from_str::<Value>(&fields.iter().find(|(key, _)| key == "profile").unwrap().1)
            .unwrap(),
        attempt.profile
    );
    assert!(begin(&mut shell, NOW).is_err());
}

#[test]
fn validation_keeps_unicode_draft_and_exact_existing_expiry() {
    let mut shell = shell();
    edit(&mut shell, &"🦊".repeat(51), ":ship:", Expiration::Never);
    assert!(begin(&mut shell, NOW).is_err());
    assert_eq!(
        shell.self_status.draft.as_ref().unwrap().text,
        "🦊".repeat(51)
    );
    let draft = shell.self_status.draft.as_mut().unwrap();
    draft.text = "Current status".into();
    draft.expiration = Expiration::Custom;
    draft.original_expiration = Some(NOW + 3677);
    draft.custom_datetime =
        super_platinum_core::block_actions::form::datetime_local_value(draft.original_expiration);
    assert_eq!(draft.profile(NOW).unwrap().1.expiration, NOW + 3677);
    draft.text.clear();
    draft.emoji.clear();
    draft.custom_datetime = "invalid".into();
    assert_eq!(draft.profile(NOW).unwrap().1, Status::default());
}

#[test]
fn stale_account_session_and_transport_cannot_apply_a_status_or_feedback() {
    let changes: [fn(&mut ShellState); 5] = [
        |s| s.core.account_epoch += 1,
        |s| s.core.active_team = Some("OTHER".into()),
        |s| s.core.transport = Some(Arc::new(Transport::new("replacement").unwrap())),
        |s| s.self_account.user_id = "OTHER".into(),
        |s| {
            let team = s.core.active_team.clone().unwrap();
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token = "replacement".into();
        },
    ];
    for change in changes {
        let mut shell = shell();
        edit(&mut shell, "New status", ":ship:", Expiration::Never);
        let attempt = begin(&mut shell, NOW).unwrap();
        let before = attempt.scope.status(&shell);
        change(&mut shell);
        finish(&mut shell, &attempt, Ok(response(&attempt.desired)));
        assert_eq!(attempt.scope.status(&shell), before);
        assert!(poll(&mut shell));
        assert!(shell.self_status.draft.is_none());
        assert_ne!(shell.overlay, Some(Overlay::SelfStatus));
    }
}

#[test]
fn rejection_allows_explicit_retry_but_uncertainty_requires_readback() {
    let mut shell = shell();
    edit(&mut shell, "New status", ":ship:", Expiration::Never);
    let rejected = begin(&mut shell, NOW).unwrap();
    finish(
        &mut shell,
        &rejected,
        Err(Error::Api("invalid_profile".into())),
    );
    assert_eq!(shell.self_status.draft.as_ref().unwrap().text, "New status");
    assert!(shell.self_status.pending.is_none());
    let uncertain = begin(&mut shell, NOW).unwrap();
    finish(
        &mut shell,
        &uncertain,
        Err(Error::Transport("synthetic disconnect".into())),
    );
    assert!(begin(&mut shell, NOW).is_err());
    close_shell(&mut shell);
    assert!(shell.self_status.draft.is_some());
    open_shell(&mut shell, NOW);
    let check = begin_check(&mut shell).unwrap();
    assert!(begin_check(&mut shell).is_none());
    finish_check(&mut shell, &check, Err(Error::Offline("synthetic".into())));
    assert!(begin(&mut shell, NOW).is_err());
    let check = begin_check(&mut shell).unwrap();
    finish_check(&mut shell, &check, Ok(response(&uncertain.desired)));
    assert!(shell.self_status.draft.is_none());
}

#[test]
fn confirmed_save_merges_only_status_and_preserves_newer_remote_status() {
    for changed_status in [false, true] {
        let mut shell = shell();
        edit(&mut shell, "New status", ":ship:", Expiration::Never);
        let attempt = begin(&mut shell, NOW).unwrap();
        let team = shell.core.active_team.clone().unwrap();
        let user = shell.self_account.user_id.clone();
        let profile = shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .users
            .get_mut(&user)
            .unwrap()
            .profile
            .get_or_insert_with(Default::default);
        profile.real_name = Some("New remote name".into());
        if changed_status {
            profile.status_text = Some("New remote status".into());
        }
        finish(&mut shell, &attempt, Ok(response(&attempt.desired)));
        let profile = shell.core.workspaces[&team].users[&user]
            .profile
            .as_ref()
            .unwrap();
        assert_eq!(profile.real_name.as_deref(), Some("New remote name"));
        assert_eq!(
            profile.status_text.as_deref(),
            Some(if changed_status {
                "New remote status"
            } else {
                "New status"
            })
        );
        assert_eq!(shell.self_status.draft.is_some(), changed_status);
    }
}

#[test]
fn newer_clear_intent_retires_pending_editor_write() {
    let mut shell = shell();
    edit(&mut shell, "Old editor intent", ":ship:", Expiration::Never);
    let attempt = begin(&mut shell, NOW).unwrap();
    let clear = SelfMenuContext::begin_status(&mut shell).unwrap();
    assert!(clear.current(&shell));
    shell.clear_self_status();
    finish(&mut shell, &attempt, Ok(response(&attempt.desired)));
    assert!(poll(&mut shell));
    assert!(attempt.scope.status(&shell).text.is_empty());
}

#[test]
fn missing_or_malformed_profile_is_never_confirmation_of_clear() {
    for value in [
        json!({}),
        json!({"ok":true}),
        json!({"ok":false,"profile":{}}),
        json!({"ok":true,"profile":{"status_text":"","status_emoji":""}}),
        json!({"ok":true,"profile":{"status_text":"","status_emoji":"","status_expiration":"0"}}),
    ] {
        assert!(decode_profile(value).is_err());
    }
    assert!(
        decode_profile(
            json!({"ok":true,"profile":{"status_text":"","status_emoji":"","status_expiration":0}})
        )
        .is_ok()
    );
}

#[test]
fn cooldown_expiry_requests_one_repaint_without_unrelated_input() {
    let mut shell = shell();
    edit(&mut shell, "Retained draft", ":ship:", Expiration::Never);
    let attempt = begin(&mut shell, NOW).unwrap();
    finish(
        &mut shell,
        &attempt,
        Err(Error::RateLimited {
            retry_after_secs: Some(30),
        }),
    );
    assert!(!presentation(&shell).unwrap().can_save);
    assert!(!poll(&mut shell));
    shell.self_status.draft.as_mut().unwrap().retry_at = now() - 1;
    assert!(poll(&mut shell));
    assert!(presentation(&shell).unwrap().can_save);
    assert_eq!(presentation(&shell).unwrap().text, "Retained draft");
    assert!(!poll(&mut shell));
}

#[test]
fn clear_status_uses_the_same_pending_receipt_and_never_replays_uncertain_writes() {
    let mut shell = shell();
    assert!(prepare_clear(&mut shell, NOW));
    let attempt = begin(&mut shell, NOW).unwrap();
    assert_eq!(attempt.desired, Status::default());
    assert!(blocks_profile_edit(&shell));
    assert!(!prepare_clear(&mut shell, NOW));
    finish(
        &mut shell,
        &attempt,
        Err(Error::Transport("lost response".into())),
    );
    assert!(blocks_profile_edit(&shell));
    assert!(!prepare_clear(&mut shell, NOW));
    assert!(begin(&mut shell, NOW).is_err());
    finish_check(&mut shell, &attempt, Ok(response(&Status::default())));
    assert!(!blocks_profile_edit(&shell));
}
