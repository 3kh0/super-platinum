use super::*;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn token(s: &ShellState) -> Token {
    Token::capture(s, "C2").unwrap()
}
fn start(s: &mut ShellState, action: Action) -> Attempt {
    let t = token(s);
    let a = begin(s, t, action, now()).unwrap();
    fixture::dispatch_shell(s, a.clone());
    a
}
#[test]
fn mode_shares_mute_lock_and_adopts_only_confirmed_target_keys() {
    let mut s = shell();
    let old = s.core.workspaces["T1"].notifications.clone();
    let a = start(&mut s, Action::Mode(Mode::Everything));
    let t = token(&s);
    assert!(super::super::begin(&mut s, t.clone(), now()).is_none());
    assert!(begin(&mut s, t, Action::DesktopPush(false), now()).is_none());
    assert_eq!(
        s.core.workspaces["T1"].notifications.channels["C2"]["desktop"],
        "mentions_dms"
    );
    fixture::complete(&mut s, false);
    let p = &s.core.workspaces["T1"].notifications;
    assert_eq!(p.global, old.global);
    assert_eq!(p.channels["C1"], old.channels["C1"]);
    assert_eq!(p.channels["C2"]["unknown_target"], json!([1, 2]));
    assert_eq!(p.channels["C2"]["desktop"], "everything");
    assert_eq!(p.channels["C2"]["mobile"], "everything");
    assert!(s.channel_mute.entries[&a.token.owner].operation.is_none());
    assert!(s.channel_mute.settings_fixture.as_ref().unwrap().valid);
    let t = token(&s);
    let mute = super::super::begin(&mut s, t, now()).unwrap();
    let t = token(&s);
    assert!(begin(&mut s, t, Action::Mode(Mode::Mentions), now()).is_none());
    assert!(mute.settings.is_none());
}
#[test]
fn preflight_rejects_local_global_drift_and_stale_visit() {
    for local in [true, false] {
        let mut s = shell();
        let t = token(&s);
        let a = begin(&mut s, t, Action::DesktopPush(false), now()).unwrap();
        let fresh = fixture::snapshot(&s);
        if local {
            s.core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .notifications
                .global
                .insert("global_desktop_push_enabled".into(), json!(false));
        } else {
            s.channel_generation += 1;
        }
        assert!(!preflight(&mut s, &a, &fresh, now()));
        assert!(s.channel_mute.entries[&a.token.owner].operation.is_none());
    }
}
#[test]
fn partial_readback_and_late_owner_cannot_settle_or_erase_preferences() {
    let mut s = shell();
    let a = start(&mut s, Action::Mode(Mode::Everything));
    fixture::complete(&mut s, true);
    let t = token(&s);
    let r = begin_check(&mut s, t, now()).unwrap();
    let before = s.core.workspaces["T1"].notifications.channels["C2"].clone();
    let partial=api::parse_notification_preferences(&json!({"ok":true,"prefs":{"all_notifications_prefs":{"global":{},"channels":{"C2":{"desktop":"everything"}}}}})).unwrap();
    assert!(!finish_read(&mut s, &r, Ok(partial), now()));
    assert_eq!(s.core.workspaces["T1"].notifications.channels["C2"], before);
    let t = token(&s);
    let r = begin_check(&mut s, t, now()).unwrap();
    let good = fixture::snapshot(&s);
    s.core.account_epoch += 1;
    assert!(!finish_read(&mut s, &r, Ok(good), now()));
    assert_eq!(s.core.workspaces["T1"].notifications.channels["C2"], before);
    let t = token(&s);
    let r = begin_check(&mut s, t, now()).unwrap();
    let good = fixture::snapshot(&s);
    assert!(finish_read(&mut s, &r, Ok(good), now()));
    assert!(s.channel_mute.entries[&a.token.owner].operation.is_none());
    assert_eq!(
        s.channel_mute
            .settings_fixture
            .as_ref()
            .unwrap()
            .requests
            .len(),
        1
    );
}
#[test]
fn settings_rate_limit_blocks_mute_without_replay() {
    let mut s = shell();
    let a = start(&mut s, Action::DesktopPush(false));
    finish_write(
        &mut s,
        &a,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
        now(),
    );
    let t = token(&s);
    assert!(begin_check(&mut s, t.clone(), now()).is_none());
    assert!(super::super::begin(&mut s, t, now()).is_none());
    assert_eq!(
        s.channel_mute
            .settings_fixture
            .as_ref()
            .unwrap()
            .requests
            .len(),
        1
    );
}

#[test]
fn cookie_or_workspace_url_rotation_rejects_mute_and_settings_dispatch_and_adoption() {
    use super_platinum_core::config::Session;
    fn authenticated() -> ShellState {
        let mut s = shell();
        let w = synthetic_session(&token(&s));
        s.core.session = Some(Session {
            d_cookie: "synthetic-cookie".into(),
            workspaces: [("T1".into(), w)].into(),
        });
        s
    }
    fn rotate(s: &mut ShellState, cookie: bool) {
        let session = s.core.session.as_mut().unwrap();
        if cookie {
            session.d_cookie = "rotated-cookie".into();
        } else {
            session.workspaces.get_mut("T1").unwrap().url = "https://rotated.slack.com".into();
        }
    }
    fn capture(s: &mut ShellState, settings: bool) -> Attempt {
        let t = token(s);
        if settings {
            begin(s, t, Action::Mode(Mode::Everything), now()).unwrap()
        } else {
            super::super::begin(s, t, now()).unwrap()
        }
    }
    for cookie in [true, false] {
        for settings in [true, false] {
            let mut s = authenticated();
            let a = capture(&mut s, settings);
            let fresh = fixture::snapshot(&s);
            rotate(&mut s, cookie);
            assert!(!a.token.current(&s));
            assert!(!if settings {
                preflight(&mut s, &a, &fresh, now())
            } else {
                mark_sending(&mut s, &a)
            });
            assert!(s.channel_mute.entries[&a.token.owner].operation.is_none());
            let mut s = authenticated();
            let a = capture(&mut s, settings);
            assert!(mark_sending(&mut s, &a));
            assert!(finish_write(&mut s, &a, Ok(()), now()));
            let t = token(&s);
            let r = begin_check(&mut s, t, now()).unwrap();
            let before = s.core.workspaces["T1"].notifications.channels["C2"].clone();
            let mut raw = fixture::snapshot(&s).raw().clone();
            if let Some(intent) = a.settings.as_ref() {
                for (k, v) in intent.desired_patch() {
                    raw["channels"]["C2"][k] = v.clone();
                }
            } else {
                raw["channels"]["C2"]["muted"] = json!(true);
            }
            let canonical = api::parse_notification_preferences(
                &json!({"ok":true,"prefs":{"all_notifications_prefs":raw}}),
            )
            .unwrap();
            rotate(&mut s, cookie);
            assert!(!super::super::finish_read(
                &mut s,
                &r,
                Ok(canonical.clone()),
                now()
            ));
            assert_eq!(s.core.workspaces["T1"].notifications.channels["C2"], before);
            assert!(s.channel_mute.entries[&a.token.owner].operation.is_some());
            let t = token(&s);
            let r = begin_check(&mut s, t, now()).unwrap();
            assert!(super::super::finish_read(&mut s, &r, Ok(canonical), now()));
        }
    }
}
