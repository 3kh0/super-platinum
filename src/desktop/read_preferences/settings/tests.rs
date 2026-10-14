use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    crate::channel_mute::fixture::install(&mut s);
    s.overlay = Some(Overlay::Settings);
    s.settings_section = SettingsSection::Reading;
    s.core.workspaces.get_mut("T1").unwrap().read_preferences =
        ReadPreferences::from_prefs(&ReadMode::LeftOffAndRead.patch());
    s
}
fn loaded(s: &mut ShellState) -> u64 {
    let visit = next_id();
    let r = open_shell(s, visit).unwrap();
    let fresh = s.core.workspaces["T1"].read_preferences.clone();
    assert!(finish_read(s, &r, Ok(fresh)));
    visit
}
fn started(s: &mut ShellState, visit: u64) -> Attempt {
    let a = begin(s, visit, ReadMode::NewestKeepUnseen).unwrap();
    let fresh = s.core.workspaces["T1"].read_preferences.clone();
    assert!(preflight(s, &a, &fresh));
    a
}
#[test]
fn opening_is_read_only_and_unchanged_choice_does_not_reserve() {
    let mut s = shell();
    let visit = loaded(&mut s);
    assert!(begin(&mut s, visit, ReadMode::LeftOffAndRead).is_none());
    assert!(s.read_settings.entries.is_empty());
    assert_eq!(
        presentation(&s, visit).unwrap().mode,
        Some(ReadMode::LeftOffAndRead)
    );
}
#[test]
fn local_realtime_change_blocks_mutation_and_late_read_does_not_erase_it() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let r = begin_read(&mut s, visit).unwrap();
    let old = s.core.workspaces["T1"].read_preferences.clone();
    let fresh = ReadPreferences::from_prefs(&ReadMode::NewestAndRead.patch());
    s.core.workspaces.get_mut("T1").unwrap().read_preferences = fresh.clone();
    assert!(!finish_read(&mut s, &r, Ok(old)));
    assert_eq!(s.core.workspaces["T1"].read_preferences, fresh);
    assert!(begin(&mut s, visit, ReadMode::NewestKeepUnseen).is_none());
}
#[test]
fn exact_raw_baseline_rejects_even_an_equivalent_mode() {
    let mut s = shell();
    s.core.workspaces.get_mut("T1").unwrap().read_preferences =
        ReadPreferences::from_prefs(&BTreeMap::new());
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, ReadMode::LeftOffAndRead).unwrap();
    let changed = ReadPreferences::from_prefs(&ReadMode::NewestKeepUnseen.patch());
    assert!(!preflight(&mut s, &a, &changed));
    assert!(s.read_settings.entries[&a.token.owner].pending.is_none());
}
#[test]
fn close_cancels_preflight_but_retains_a_dispatched_unknown_write() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, ReadMode::NewestKeepUnseen).unwrap();
    close_shell(&mut s, visit);
    assert!(s.read_settings.entries[&a.token.owner].pending.is_none());
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    close_shell(&mut s, visit);
    assert!(finish_write(&mut s, &a, Err(Error::Transport("lost response".into()))).is_none());
    let next = next_id();
    assert!(open_shell(&mut s, next).is_none());
    assert!(presentation(&s, next).unwrap().uncertain);
    assert!(begin(&mut s, next, ReadMode::NewestAndRead).is_none());
}
#[test]
fn positive_ack_requires_exact_readback_and_never_replays_on_mismatch() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    let r = finish_write(&mut s, &a, Ok(())).unwrap();
    let missing = ReadPreferences::from_prefs(&BTreeMap::new());
    assert_eq!(missing.mode().ok(), Some(ReadMode::NewestKeepUnseen));
    assert!(!finish_read(&mut s, &r, Ok(missing)));
    assert!(presentation(&s, visit).unwrap().uncertain);
    let r = begin_read(&mut s, visit).unwrap();
    let exact = ReadPreferences::from_prefs(&ReadMode::NewestKeepUnseen.patch());
    assert!(finish_read(&mut s, &r, Ok(exact)));
    assert!(s.read_settings.entries[&a.token.owner].pending.is_none());
}
#[test]
fn account_rotation_cannot_adopt_or_clear_another_operation() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    let r = finish_write(&mut s, &a, Ok(())).unwrap();
    let old = s.core.workspaces["T1"].read_preferences.clone();
    s.core.account_epoch += 1;
    assert!(!finish_read(
        &mut s,
        &r,
        Ok(ReadPreferences::from_prefs(
            &ReadMode::NewestKeepUnseen.patch()
        ))
    ));
    assert_eq!(s.core.workspaces["T1"].read_preferences, old);
    assert!(
        s.read_settings.entries[&a.token.owner]
            .pending
            .as_ref()
            .is_some_and(|p| p.phase == Phase::Uncertain)
    );
    assert!(poll(&mut s));
}
#[test]
fn unsupported_positive_read_revokes_old_immediate_policy() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let r = begin_read(&mut s, visit).unwrap();
    let malformed = api::parse_read_preferences(&json!({"ok":true,"prefs":{"mark_msgs_read_immediately":"yes","start_scroll_at_oldest":true}})).unwrap();
    assert!(!finish_read(&mut s, &r, Ok(malformed.clone())));
    assert_eq!(s.core.workspaces["T1"].read_preferences, malformed);
    assert!(s.core.workspaces["T1"].read_preferences.mode().is_err());
    assert!(!presentation(&s, visit).unwrap().can_change);
}
#[test]
fn malformed_preflight_revokes_authority_but_not_newer_realtime() {
    for drift in [false, true] {
        let mut s = shell();
        let visit = loaded(&mut s);
        let a = begin(&mut s, visit, ReadMode::NewestAndRead).unwrap();
        let malformed = api::parse_read_preferences(
            &json!({"ok":true,"prefs":{"mark_msgs_read_immediately":null}}),
        )
        .unwrap();
        let newest = ReadPreferences::from_prefs(&ReadMode::NewestKeepUnseen.patch());
        if drift {
            s.core.workspaces.get_mut("T1").unwrap().read_preferences = newest.clone();
        }
        rejected_fresh(&mut s, &a, &malformed);
        assert_eq!(
            s.core.workspaces["T1"].read_preferences,
            if drift { newest } else { malformed }
        );
        assert!(s.read_settings.entries[&a.token.owner].pending.is_none());
    }
}
#[test]
fn only_known_rejections_release_the_mutation_reservation() {
    for (error, retained) in [
        (Error::Transport("lost".into()), true),
        (Error::Api("fatal_error".into()), true),
        (Error::Api("new_unclassified_error".into()), true),
        (Error::Api("invalid_auth".into()), false),
        (Error::Offline("not sent".into()), false),
    ] {
        let mut s = shell();
        let visit = loaded(&mut s);
        let a = started(&mut s, visit);
        assert!(finish_write(&mut s, &a, Err(error)).is_none());
        assert_eq!(
            s.read_settings.entries[&a.token.owner].pending.is_some(),
            retained
        );
    }
}
#[test]
fn exact_credentials_rotation_invalidates_captured_reads() {
    for field in ["cookie", "team", "user", "enterprise", "token", "url"] {
        let mut s = shell();
        let t = Token::capture(&s).unwrap();
        let workspace = session(&s, &t).unwrap();
        s.core.session = Some(super_platinum_core::config::Session {
            d_cookie: "fake-cookie".into(),
            workspaces: [("T1".into(), workspace)].into(),
        });
        let token = Token::capture(&s).unwrap();
        let session = s.core.session.as_mut().unwrap();
        let w = session.workspaces.get_mut("T1").unwrap();
        match field {
            "cookie" => session.d_cookie = "rotated".into(),
            "team" => w.team_id = "T2".into(),
            "user" => w.user_id = "U2".into(),
            "enterprise" => w.enterprise_id = Some("E2".into()),
            "token" => w.token = "rotated".into(),
            "url" => w.url = "https://other.slack.com".into(),
            _ => unreachable!(),
        }
        assert!(!token.current(&s), "{field}");
    }
}
#[test]
fn rate_limit_blocks_reads_and_writes_for_only_the_captured_owner() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    finish_write(
        &mut s,
        &a,
        Err(Error::RateLimited {
            retry_after_secs: Some(40),
        }),
    );
    assert!(cooling(&s));
    assert!(begin_read(&mut s, visit).is_none());
    assert!(begin(&mut s, visit, ReadMode::NewestAndRead).is_none());
    s.core.active_account = Some("another-account".into());
    assert!(!cooling(&s));
}
fn replace_account(s: &mut ShellState, account: &str) {
    let mut replacement = shell();
    replacement.core.account_epoch = s.core.account_epoch + 1;
    replacement.core.active_account = Some(account.into());
    carry_pending(s, &mut replacement);
    *s = replacement;
}
#[test]
fn real_shell_replacement_preserves_uncertainty_without_copying_other_account_prefs() {
    let mut s = shell();
    let owner = s.core.active_account.clone().unwrap();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    finish_write(&mut s, &a, Err(Error::Transport("lost response".into())));
    replace_account(&mut s, "another-account");
    assert!(s.read_settings.view.is_none());
    let other_visit = loaded(&mut s);
    assert!(!presentation(&s, other_visit).unwrap().uncertain);
    assert_eq!(
        s.core.workspaces["T1"].read_preferences.mode().ok(),
        Some(ReadMode::LeftOffAndRead)
    );
    replace_account(&mut s, &owner);
    let visit = next_id();
    assert!(open_shell(&mut s, visit).is_none());
    assert!(presentation(&s, visit).unwrap().uncertain);
    assert!(begin(&mut s, visit, ReadMode::NewestAndRead).is_none());
    let read = begin_read(&mut s, visit).unwrap();
    assert!(finish_read(
        &mut s,
        &read,
        Ok(ReadPreferences::from_prefs(
            &ReadMode::NewestKeepUnseen.patch()
        ))
    ));
    assert!(!presentation(&s, visit).unwrap().uncertain);
}
#[test]
fn replacement_retains_inflight_write_and_check_until_their_owned_completion() {
    for checking in [false, true] {
        let mut s = shell();
        let owner = s.core.active_account.clone().unwrap();
        let visit = loaded(&mut s);
        let a = started(&mut s, visit);
        let read = checking.then(|| finish_write(&mut s, &a, Ok(())).unwrap());
        replace_account(&mut s, "another-account");
        if let Some(read) = read {
            assert!(!finish_read(
                &mut s,
                &read,
                Ok(ReadPreferences::from_prefs(
                    &ReadMode::NewestKeepUnseen.patch()
                ))
            ));
        } else {
            assert!(finish_write(&mut s, &a, Ok(())).is_none());
        }
        assert!(
            s.read_settings.entries[&a.token.owner]
                .pending
                .as_ref()
                .is_some_and(|p| p.phase == Phase::Uncertain)
        );
        assert_eq!(
            s.core.workspaces["T1"].read_preferences.mode().ok(),
            Some(ReadMode::LeftOffAndRead)
        );
        replace_account(&mut s, &owner);
        let visit = next_id();
        assert!(open_shell(&mut s, visit).is_none());
        assert!(presentation(&s, visit).unwrap().uncertain);
    }
}
#[test]
fn replacement_cancels_unsent_preflight_and_retains_retry_after_on_return() {
    let mut s = shell();
    let owner = s.core.active_account.clone().unwrap();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, ReadMode::NewestKeepUnseen).unwrap();
    let old = s.core.workspaces["T1"].read_preferences.clone();
    remember(
        &mut s,
        &a.token,
        &Error::RateLimited {
            retry_after_secs: Some(40),
        },
    );
    replace_account(&mut s, "another-account");
    assert!(!cooling(&s));
    assert!(!preflight(&mut s, &a, &old));
    assert!(!s.read_settings.entries.contains_key(&a.token.owner));
    replace_account(&mut s, &owner);
    assert!(cooling(&s));
    assert!(open_shell(&mut s, next_id()).is_none());
}
#[test]
fn successful_idle_reload_clears_old_rejection_but_malformed_read_keeps_its_error() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    finish_write(&mut s, &a, Err(Error::Api("invalid_auth".into())));
    let old_message = s.read_settings.entries[&a.token.owner].message.clone();
    assert!(
        old_message
            .as_deref()
            .is_some_and(|m| m.contains("Reload before trying again"))
    );
    let read = begin_read(&mut s, visit).unwrap();
    let malformed = api::parse_read_preferences(
        &json!({"ok":true,"prefs":{"mark_msgs_read_immediately":null}}),
    )
    .unwrap();
    assert!(!finish_read(&mut s, &read, Ok(malformed)));
    assert_eq!(s.read_settings.entries[&a.token.owner].message, old_message);
    assert!(
        presentation(&s, visit)
            .unwrap()
            .message
            .as_deref()
            .is_some_and(|m| m.contains("unsupported reading preference"))
    );
    let read = begin_read(&mut s, visit).unwrap();
    assert!(finish_read(
        &mut s,
        &read,
        Ok(ReadPreferences::from_prefs(
            &ReadMode::LeftOffAndRead.patch()
        ))
    ));
    let p = presentation(&s, visit).unwrap();
    assert!(p.can_change && p.message.is_none());
    assert!(s.read_settings.entries[&a.token.owner].message.is_none());
}
#[test]
fn successful_unknown_check_preserves_unconfirmed_message_until_exact_confirmation() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    finish_write(&mut s, &a, Err(Error::Transport("lost response".into())));
    let read = begin_read(&mut s, visit).unwrap();
    assert!(!finish_read(
        &mut s,
        &read,
        Ok(ReadPreferences::from_prefs(
            &ReadMode::LeftOffAndRead.patch()
        ))
    ));
    let pending_message = s.read_settings.entries[&a.token.owner].message.clone();
    assert!(
        pending_message
            .as_deref()
            .is_some_and(|m| m.contains("has not confirmed"))
    );
    assert!(presentation(&s, visit).unwrap().uncertain);
    let read = begin_read(&mut s, visit).unwrap();
    let malformed = api::parse_read_preferences(
        &json!({"ok":true,"prefs":{"mark_msgs_read_immediately":"unsupported"}}),
    )
    .unwrap();
    assert!(!finish_read(&mut s, &read, Ok(malformed)));
    assert_eq!(
        s.read_settings.entries[&a.token.owner].message,
        pending_message
    );
    assert!(
        presentation(&s, visit)
            .unwrap()
            .message
            .as_deref()
            .is_some_and(|m| m.contains("unsupported reading preference"))
    );
    assert!(presentation(&s, visit).unwrap().uncertain);
}
