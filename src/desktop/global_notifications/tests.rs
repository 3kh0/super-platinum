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
    let a = begin(s, visit, Action::DesktopPush(false)).unwrap();
    fixture::dispatch_shell(s, a.clone());
    a
}
#[test]
fn local_relink_and_normalized_keyword_noops_complete_without_request() {
    let mut s = shell();
    let visit = loaded(&mut s);
    s.global_notifications.view.as_mut().unwrap().linked = Some(false);
    assert!(begin(&mut s, visit, Action::RelinkMobile).is_none());
    assert!(
        s.global_notifications
            .view
            .as_ref()
            .unwrap()
            .linked
            .unwrap()
    );
    keywords::edit_shell(&mut s, visit, "alpha, beta".into()); // one space is stripped -> not same canonical
    let owner = Token::capture(&s).unwrap().owner;
    let d = s
        .global_notifications
        .entries
        .get_mut(&owner)
        .unwrap()
        .keywords
        .as_mut()
        .unwrap();
    d.baseline = "alpha,beta".into();
    s.global_notifications.view.as_mut().unwrap().snapshot=Some(api::parse_notification_preferences(&json!({"ok":true,"prefs":{"all_notifications_prefs":{"global":{"global_desktop":"mentions_dms","global_mobile":"mentions_dms","global_keywords":"alpha,beta"},"channels":{}}}})).unwrap());
    keywords::edit_shell(&mut s, visit, "alpha,beta".into());
    assert!(
        s.global_notifications.entries[&owner]
            .keywords
            .as_ref()
            .unwrap()
            .dirty
    );
    assert!(begin(&mut s, visit, Action::Keywords("alpha,beta".into())).is_none());
    assert!(
        !s.global_notifications.entries[&owner]
            .keywords
            .as_ref()
            .unwrap()
            .dirty
    );
    assert!(
        s.global_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .is_empty()
    );
}
#[test]
fn dirty_keywords_survive_navigation_and_owner_rotation_without_autosave() {
    let mut s = shell();
    let visit = loaded(&mut s);
    keywords::edit_shell(&mut s, visit, "draft,🦊".into());
    let owner = s.core.active_account.clone();
    close_shell(&mut s, visit);
    s.core.active_account = Some("other".into());
    let other = loaded(&mut s);
    assert_ne!(presentation(&s, other).unwrap().keywords, "draft,🦊");
    close_shell(&mut s, other);
    s.core.active_account = owner;
    let back = loaded(&mut s);
    assert_eq!(presentation(&s, back).unwrap().keywords, "draft,🦊");
    assert!(presentation(&s, back).unwrap().keyword_dirty);
    assert!(
        s.global_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .is_empty()
    );
}
#[test]
fn pending_duplicate_and_late_input_never_change_captured_keyword_patch() {
    let mut s = shell();
    let visit = loaded(&mut s);
    keywords::edit_shell(&mut s, visit, "new,  🦊".into());
    let a = begin(&mut s, visit, Action::Keywords("new,  🦊".into())).unwrap();
    fixture::dispatch_shell(&mut s, a.clone());
    keywords::edit_shell(&mut s, visit, "late".into());
    assert_eq!(presentation(&s, visit).unwrap().keywords, "new,  🦊");
    assert!(begin(&mut s, visit, Action::ThreadReplies(true)).is_none());
    assert_eq!(
        a.intent.desired_patch()["global_keywords"],
        json!("new, 🦊")
    );
    fixture::complete(&mut s, false);
    assert!(!presentation(&s, visit).unwrap().keyword_dirty);
}
#[test]
fn unknown_result_close_reopen_requires_exact_readback_without_replay() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
    close_shell(&mut s, visit);
    fixture::complete_without_apply(&mut s);
    let next = next_id();
    assert!(open_shell(&mut s, next).is_none());
    let r = begin_read(&mut s, next).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(!finish_read(&mut s, &r, Ok(fresh)));
    assert!(
        s.global_notifications.entries[&a.token.owner]
            .pending
            .is_some()
    );
    fixture::apply_unconfirmed(&mut s);
    let r = begin_read(&mut s, next).unwrap();
    let fresh = fixture::snapshot(&s);
    assert!(finish_read(&mut s, &r, Ok(fresh)));
    assert_eq!(
        s.global_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .len(),
        1
    );
}
#[test]
fn mounted_link_and_local_global_changes_reject_stale_preflight_but_channels_survive() {
    let mut s = shell();
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, Action::DesktopPush(false)).unwrap();
    s.global_notifications.view.as_mut().unwrap().linked = Some(false);
    let fresh = fixture::snapshot(&s);
    assert!(!preflight(&mut s, &a, &fresh));
    let visit = loaded(&mut s);
    let a = begin(&mut s, visit, Action::DesktopPush(false)).unwrap();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .notifications
        .global
        .insert("global_desktop".into(), json!("everything"));
    assert!(!preflight(&mut s, &a, &fresh));
    let visit = loaded(&mut s);
    let a = started(&mut s, visit);
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
    assert_eq!(
        s.core.workspaces["T1"].notifications.channels["C9"]["muted"],
        json!(true)
    );
    assert!(
        s.global_notifications.entries[&a.token.owner]
            .pending
            .is_none()
    );
}
#[test]
fn read_leases_and_authenticated_cooldown_survive_navigation_without_replay() {
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
        s.global_notifications.entries[&a.token.owner]
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
    assert_eq!(crate::channel_mute::prefs_retry_at(&s), deadline);
    s.core.active_account = Some("new-owner".into());
    assert!(!cooling(&s));
    assert_eq!(
        s.global_notifications
            .fixture
            .as_ref()
            .unwrap()
            .writes
            .len(),
        1
    );
}
#[test]
fn credential_rotation_retires_view_not_dirty_draft_or_uncertain_receipt() {
    let mut s = shell();
    let visit = loaded(&mut s);
    keywords::edit_shell(&mut s, visit, "retained".into());
    let a = started(&mut s, visit);
    fixture::complete(&mut s, true);
    let old = mount_key(&s);
    s.core.session = Some(super_platinum_core::config::Session {
        d_cookie: "rotated".into(),
        workspaces: [("T1".into(), session(&s, &a.token).unwrap())].into(),
    });
    assert_ne!(old, mount_key(&s));
    assert!(poll(&mut s));
    assert!(s.global_notifications.view.is_none());
    assert_eq!(
        s.global_notifications.entries[&a.token.owner]
            .keywords
            .as_ref()
            .unwrap()
            .text,
        "retained"
    );
    assert!(
        s.global_notifications.entries[&a.token.owner]
            .pending
            .is_some()
    );
}
