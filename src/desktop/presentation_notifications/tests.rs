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
    let a = begin(s, visit, Action::DesktopSound("b2.mp3".into())).unwrap();
    fixture::dispatch_shell(s, a.clone());
    a
}
#[test]
fn opening_and_local_preview_have_no_mutation_and_saved_nonfirst_sound_is_retained() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let p = presentation(&s, visit).unwrap();
    assert_eq!(
        p.snapshot
            .unwrap()
            .settings()
            .unwrap()
            .desktop_sound
            .as_deref(),
        Some("been_tree.mp3")
    );
    assert!(p.has_vips && p.vip_available);
    assert!(
        s.presentation_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .is_empty()
    );
}
#[test]
fn vip_requires_known_paid_plan_and_membership_and_binds_both_at_preflight() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let team = s.core.workspaces.get_mut("T1").unwrap();
    team.team_plan = None;
    assert!(begin(&mut s, visit, Action::PrioritySound("b2.mp3".into())).is_none());
    s.core.workspaces.get_mut("T1").unwrap().team_plan = Some("paid".into());
    let a = begin(&mut s, visit, Action::PrioritySound("b2.mp3".into())).unwrap();
    s.core.workspaces.get_mut("T1").unwrap().vip_users.clear();
    let fresh = fixture::snapshot(&s);
    assert!(!preflight(&mut s, &a, &fresh));
    let visit = loaded(&mut s);
    assert!(begin(&mut s, visit, Action::PrioritySound("b2.mp3".into())).is_none());
}
#[test]
fn fresh_mute_or_legacy_nothing_retires_queued_sound_without_dispatch() {
    for key in ["mute", "nothing"] {
        let mut s = shell();
        let visit = loaded(&mut s);
        let a = begin(&mut s, visit, Action::DesktopSound("b2.mp3".into())).unwrap();
        let f = s.presentation_notifications.fixture.as_mut().unwrap();
        if key == "mute" {
            f.raw["mute_sounds"] = json!(true);
        } else {
            f.raw["all_notifications_prefs"]["global"]["global_desktop"] = json!("nothing");
        }
        let fresh = fixture::snapshot(&s);
        assert!(!preflight(&mut s, &a, &fresh));
        assert!(
            s.presentation_notifications
                .fixture
                .as_ref()
                .unwrap()
                .writes
                .is_empty()
        );
    }
}
#[test]
fn unknown_result_keeps_owner_lock_until_exact_named_read_and_no_replay() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    assert!(begin(&mut s, visit, Action::MuteSounds(true)).is_none());
    close_shell(&mut s, visit);
    fixture::complete_without_apply(&mut s);
    let next = next_id();
    assert!(open_shell(&mut s, next).is_none());
    let r = begin_read(&mut s, next).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(!finish_read(&mut s, &r, Ok(fresh)));
    assert!(
        s.presentation_notifications.entries[&a.token.owner]
            .pending
            .is_some()
    );
    fixture::apply_unconfirmed(&mut s);
    let r = begin_read(&mut s, next).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(finish_read(&mut s, &r, Ok(fresh)));
    assert_eq!(
        s.presentation_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .len(),
        1
    );
}
#[test]
fn canonical_adoption_preserves_channel_overrides_and_other_user_keys() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, Action::MuteSounds(true)).unwrap();
    fixture::dispatch_shell(&mut s, a);
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .channels
        .entry("C9".into())
        .or_default()
        .insert("muted".into(), json!(true));
    fixture::complete(&mut s, false);
    let prefs = &s.core.workspaces["T1"].notifications;
    assert_eq!(prefs.channels["C9"]["muted"], json!(true));
    assert_eq!(prefs.user["mute_sounds"], json!(true));
    assert_eq!(prefs.user["unknown_pref"], json!({"keep":7}));
    assert_eq!(prefs.global["unknown_global"], json!({"keep":9}));
    // The named selector is complete: absence retires a previously loaded mute value.
    s.presentation_notifications
        .fixture
        .as_mut()
        .unwrap()
        .raw
        .as_object_mut()
        .unwrap()
        .remove("mute_sounds");
    let r = begin_read(&mut s, visit).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(finish_read(&mut s, &r, Ok(fresh)));
    assert!(
        !s.core.workspaces["T1"]
            .notifications
            .user
            .contains_key("mute_sounds")
    );
}
#[test]
fn closed_read_releases_its_lease_and_rate_limit_remains_authenticated() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    fixture::complete(&mut s, true);
    let r = begin_read(&mut s, visit).unwrap();
    close_shell(&mut s, visit);
    assert!(!finish_read(
        &mut s,
        &r,
        Err(Error::RateLimited {
            retry_after_secs: Some(40)
        })
    ));
    assert!(
        s.presentation_notifications.entries[&a.token.owner]
            .pending
            .as_ref()
            .is_some_and(|p| p.phase == Phase::Uncertain && p.read.is_none())
    );
    assert!(cooling(&s));
    let deadline = crate::channel_mute::prefs_retry_at(&s);
    remember(
        &mut s,
        &a.token,
        &Error::RateLimited {
            retry_after_secs: Some(1),
        },
    );
    assert_eq!(deadline, crate::channel_mute::prefs_retry_at(&s));
    s.core.active_account = Some("other".into());
    assert!(!cooling(&s));
}
#[test]
fn credential_rotation_drops_view_but_retains_possible_sent_receipt() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    fixture::complete(&mut s, true);
    let old = mount_key(&s);
    s.core.session = Some(super_platinum_core::config::Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "new-cookie".into(),
        workspaces: [("T1".into(), session(&s, &a.token).unwrap())].into(),
    });
    assert_ne!(old, mount_key(&s));
    assert!(poll(&mut s));
    assert!(s.presentation_notifications.view.is_none());
    assert!(
        s.presentation_notifications.entries[&a.token.owner]
            .pending
            .is_some()
    );
}

#[test]
fn bounce_duration_is_mounted_and_off_preserves_it_without_replaying_on_open() {
    let mut s = shell();
    let visit = loaded(&mut s);
    assert_eq!(presentation(&s, visit).unwrap().bounce_duration, "short");
    for value in ["short", "long", ""] {
        let a = begin(&mut s, visit, Action::MacBounce(value.into())).unwrap();
        fixture::dispatch_shell(&mut s, a);
        fixture::complete(&mut s, false);
    }
    assert_eq!(presentation(&s, visit).unwrap().bounce_duration, "long");
    assert_eq!(
        s.core.workspaces["T1"].notifications.user["mac_ssb_bounce"],
        json!("")
    );
    close_shell(&mut s, visit);
    let next = loaded(&mut s);
    assert_eq!(presentation(&s, next).unwrap().bounce_duration, "short");
    assert_eq!(
        s.presentation_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .len(),
        3
    );
}

#[test]
fn bounce_unknown_requires_exact_readback_and_credential_owner_before_adoption() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, Action::MacBounce("long".into())).unwrap();
    fixture::dispatch_shell(&mut s, a.clone());
    fixture::complete_without_apply(&mut s);
    let read = begin_read(&mut s, visit).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(!finish_read(&mut s, &read, Ok(fresh)));
    assert!(begin(&mut s, visit, Action::MacBounce("short".into())).is_none());
    fixture::apply_unconfirmed(&mut s);
    let old_read = begin_read(&mut s, visit).unwrap();
    s.core.account_epoch += 1;
    poll(&mut s);
    let fresh = fixture::snapshot(&s);
    assert!(!finish_read(&mut s, &old_read, Ok(fresh)));
    assert_eq!(
        s.core.workspaces["T1"].notifications.user["mac_ssb_bounce"],
        json!("")
    );
    let next = next_id();
    assert!(open_shell(&mut s, next).is_none());
    let read = begin_read(&mut s, next).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(finish_read(&mut s, &read, Ok(fresh)));
    assert_eq!(
        s.core.workspaces["T1"].notifications.user["mac_ssb_bounce"],
        json!("long")
    );
    assert_eq!(
        s.presentation_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .len(),
        1
    );
}

#[test]
fn bounce_disabled_dependency_and_missing_named_value_are_guarded() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, Action::MacBounce("short".into())).unwrap();
    s.presentation_notifications.fixture.as_mut().unwrap().raw["all_notifications_prefs"]["global"]
        ["global_desktop"] = json!("nothing");
    let fresh = fixture::snapshot(&s);
    assert!(!preflight(&mut s, &a, &fresh));
    let visit = loaded(&mut s);
    assert!(begin(&mut s, visit, Action::MacBounce("short".into())).is_none());
    s.presentation_notifications
        .fixture
        .as_mut()
        .unwrap()
        .raw
        .as_object_mut()
        .unwrap()
        .remove("mac_ssb_bounce");
    let read = begin_read(&mut s, visit).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(finish_read(&mut s, &read, Ok(fresh)));
    assert!(
        !s.core.workspaces["T1"]
            .notifications
            .user
            .contains_key("mac_ssb_bounce")
    );
}
