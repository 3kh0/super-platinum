use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn loaded(s: &mut ShellState) -> u64 {
    let visit = next_id();
    let r = open_shell(s, visit).unwrap();
    let fresh = fixture::snapshot(s);
    assert!(finish_read(s, &r, Ok(fresh)));
    visit
}
fn started(s: &mut ShellState, visit: u64) -> Attempt {
    let a = begin(s, visit, Action::Mode(Mode::Weekdays)).unwrap();
    fixture::dispatch_shell(s, a.clone());
    a
}
#[test]
fn canonical_open_never_writes_and_replaces_only_requested_keys() {
    let mut s = shell();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .user
        .insert("tz".into(), json!("Old/Zone"));
    let visit = loaded(&mut s);
    let prefs = &s.core.workspaces["T1"].notifications;
    assert!(!prefs.user.contains_key("tz"));
    assert_eq!(prefs.user["unknown_pref"], json!({"keep":7}));
    assert!(prefs.schedule_loaded);
    assert!(presentation(&s, visit).unwrap().can_change);
    assert!(
        s.notification_schedule
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .is_empty()
    );
}
#[test]
fn pending_duplicate_close_and_unknown_readback_never_replay() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    assert!(begin(&mut s, visit, Action::Mode(Mode::Custom)).is_none());
    close_shell(&mut s, visit);
    fixture::complete(&mut s, true);
    assert!(
        s.notification_schedule.entries[&a.token.owner]
            .pending
            .is_some()
    );
    let next = next_id();
    assert!(open_shell(&mut s, next).is_none());
    let r = begin_read(&mut s, next).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(finish_read(&mut s, &r, Ok(fresh)));
    assert_eq!(
        s.notification_schedule
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .len(),
        1
    );
    assert!(presentation(&s, next).unwrap().can_change);
}
#[test]
fn queued_read_retired_after_close_or_cooldown_releases_only_its_lease() {
    for closed in [true, false] {
        let mut s = shell();
        let visit = loaded(&mut s);
        let a = started(&mut s, visit);
        fixture::complete(&mut s, true);
        let r = begin_read(&mut s, visit).unwrap();
        if closed {
            close_shell(&mut s, visit);
        } else {
            crate::channel_mute::remember_prefs_cooldown(
                &mut s,
                &Error::RateLimited {
                    retry_after_secs: Some(30),
                },
            );
        }
        assert!(!finish_read(
            &mut s,
            &r,
            Err(Error::Offline("admission changed".into()))
        ));
        assert!(
            s.notification_schedule.entries[&a.token.owner]
                .pending
                .as_ref()
                .is_some_and(|p| p.phase == Phase::Uncertain && p.read.is_none())
        );
        if !closed {
            assert!(!s.notification_schedule.view.as_ref().unwrap().loading);
            assert!(!presentation(&s, visit).unwrap().can_read);
        }
    }
}
#[test]
fn stale_selected_timezone_or_credentials_reject_preflight_and_preserve_newer_read() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, Action::Mode(Mode::Weekdays)).unwrap();
    let fresh = fixture::snapshot(&s);
    let user = a.token.owner.user.clone();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .users
        .get_mut(&user)
        .unwrap()
        .tz = Some("Europe/London".into());
    assert!(!preflight(&mut s, &a, &fresh));
    assert!(
        s.notification_schedule.entries[&a.token.owner]
            .pending
            .is_none()
    );
    let r = begin_read(&mut s, visit).unwrap();
    close_shell(&mut s, visit);
    let newer = next_id();
    let new = open_shell(&mut s, newer).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(!finish_read(&mut s, &r, Ok(fresh.clone())));
    assert!(s.notification_schedule.view.as_ref().unwrap().loading);
    assert!(finish_read(&mut s, &new, Ok(fresh)));
}
#[test]
fn credential_mount_key_rotates_without_erasing_retained_uncertain_operation() {
    use super_platinum_core::config::Session;
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    fixture::complete(&mut s, true);
    let old = mount_key(&s);
    let w = session(&s, &a.token).unwrap();
    s.core.session = Some(Session {
        d_cookie: "rotated".into(),
        workspaces: [("T1".into(), w)].into(),
    });
    assert_ne!(old, mount_key(&s));
    assert!(poll(&mut s));
    assert!(
        s.notification_schedule.entries[&a.token.owner]
            .pending
            .is_some()
    );
    assert!(s.notification_schedule.view.is_none());
}
#[test]
fn partial_readback_retains_receipt_and_unrelated_profile_timezone_can_settle() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    fixture::complete(&mut s, true);
    let r = begin_read(&mut s, visit).unwrap();
    let mut raw = fixture::snapshot(&s).raw().clone();
    raw.remove("dnd_days");
    let user = super_platinum_core::slack::models::User {
        id: a.token.owner.user.clone(),
        tz: Some("Europe/London".into()),
        ..Default::default()
    };
    let partial = api::NotificationSchedule::from_prefs(&raw)
        .unwrap()
        .with_user_timezone(&user)
        .unwrap();
    assert!(!finish_read(&mut s, &r, Ok(partial)));
    let r = begin_read(&mut s, visit).unwrap();
    let fresh = fixture::snapshot(&s).with_user_timezone(&user).unwrap();
    assert!(finish_read(&mut s, &r, Ok(fresh)));
    assert_eq!(
        s.notification_schedule
            .view
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .timezone(),
        Some("Europe/London")
    );
}
#[test]
fn late_rate_limit_survives_account_navigation_without_limiting_replacement_owner() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, Action::Mode(Mode::Weekdays)).unwrap();
    let owner = s.core.active_account.clone();
    s.core.active_account = Some("other-account".into());
    preflight_failed(
        &mut s,
        &a,
        Some(&Error::RateLimited {
            retry_after_secs: Some(80),
        }),
    );
    assert!(!cooling(&s));
    s.core.active_account = owner;
    assert!(cooling(&s));
    let deadline = crate::channel_mute::prefs_retry_at(&s);
    remember(
        &mut s,
        &a.token,
        &Error::RateLimited {
            retry_after_secs: Some(1),
        },
    );
    assert_eq!(crate::channel_mute::prefs_retry_at(&s), deadline);
}
