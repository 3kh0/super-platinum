use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
fn edit(s: &mut ShellState, value: &str) -> u64 {
    let d = s.profile_title.draft.as_mut().unwrap();
    d.value = profile_text::normalize(value);
    d.visit
}
fn start(s: &mut ShellState, value: &str) -> Attempt {
    let v = edit(s, value);
    let a = begin(s, v).unwrap();
    fixture::save(s, a.clone());
    a
}
fn reopen(s: &mut ShellState) {
    if let Some((t, v)) = open_shell(s) {
        fixture::load(s, &t, v);
    }
}
#[test]
fn title_save_clear_and_cancel_preserve_siblings_and_single_dispatch() {
    let mut s = shell();
    let o = Owner::capture(&s).unwrap();
    let before = s.profile_title.fixture.as_ref().unwrap().raw.clone();
    let v = edit(&mut s, "Cancelled");
    close_visit(&mut s, Some(v));
    assert!(
        s.profile_title
            .fixture
            .as_ref()
            .unwrap()
            .attempts
            .is_empty()
    );
    reopen(&mut s);
    let a = start(&mut s, "Jose\u{301}");
    assert!(begin(&mut s, a.visit).is_none());
    assert_eq!(o.title(&s), "Original role");
    fixture::complete(&mut s);
    assert_eq!(o.title(&s), "José");
    assert!(!blocks_profile_writes(&s));
    let f = s.profile_title.fixture.as_ref().unwrap();
    assert!(f.valid);
    assert_eq!(
        f.raw["result"]["data"]["user"]["profileSections"][1],
        before["result"]["data"]["user"]["profileSections"][1]
    );
    reopen(&mut s);
    start(&mut s, "");
    fixture::complete(&mut s);
    assert_eq!(o.title(&s), "");
    let p = s.core.workspaces[&o.team].users[&o.user]
        .profile
        .as_ref()
        .unwrap();
    assert_eq!(p.status_text.as_deref(), Some("Reading"));
    assert_eq!(p.extra["title_fixture"], json!({"keep":true}));
}
#[test]
fn fresh_permission_or_sibling_drift_rejects_before_send() {
    for permission in [false, true] {
        let mut s = shell();
        let v = edit(&mut s, "New");
        let a = begin(&mut s, v).unwrap();
        let f = s.profile_title.fixture.as_mut().unwrap();
        if permission {
            f.raw["result"]["data"]["user"]["profileSections"][0]["profileElements"][0]["permissions"]
                ["ui"] = json!(false);
        } else {
            f.raw["result"]["data"]["user"]["profileSections"][0]["profileElements"][1]["text"] =
                json!("Changed sibling");
        }
        fixture::save(&mut s, a);
        assert!(
            s.profile_title
                .fixture
                .as_ref()
                .unwrap()
                .attempts
                .is_empty()
        );
        assert!(!blocks_profile_writes(&s));
        assert_eq!(s.profile_title.draft.as_ref().unwrap().value, "New");
    }
}
#[test]
fn uncertain_reopen_auth_rotation_and_readback_never_replay() {
    let mut s = shell();
    s.profile_title.fixture.as_mut().unwrap().uncertain = true;
    let a = start(&mut s, "New");
    fixture::complete(&mut s);
    close_visit(&mut s, Some(a.visit));
    s.core.account_epoch += 1;
    poll(&mut s);
    assert!(blocks_profile_writes(&s));
    reopen(&mut s);
    let v = s.profile_title.draft.as_ref().unwrap().visit;
    assert!(begin(&mut s, v).is_none());
    let (t, id, r) = begin_check(&mut s, v).unwrap();
    fixture::check(&mut s, t, id, r);
    assert!(!blocks_profile_writes(&s));
    assert_eq!(s.profile_title.fixture.as_ref().unwrap().attempts.len(), 1);
}
#[test]
fn stale_load_and_late_read_do_not_touch_new_owner_or_newer_title() {
    let mut s = shell();
    let a = start(&mut s, "New");
    assert!(finish_write(&mut s, &a, Ok(())));
    let (t, id, r) = begin_check(&mut s, a.visit).unwrap();
    let canonical = fixture::snapshot(&s, &t.owner);
    s.core.account_epoch += 1;
    poll(&mut s);
    finish_check(&mut s, &t, id, r, Ok(canonical));
    assert!(blocks_profile_writes(&s));
    assert_eq!(t.owner.title(&s), "Original role");
    reopen(&mut s);
    let d = s.profile_title.draft.as_ref().unwrap();
    let token = d.token.clone();
    let visit = d.visit;
    let cap = fixture::snapshot(&s, &token.owner);
    close_visit(&mut s, Some(visit));
    finish_load(&mut s, &token, visit, Ok(cap));
    assert!(s.profile_title.draft.is_none());
}
#[test]
fn three_profile_controllers_serialize_without_recursive_locks() {
    let mut s = shell();
    s.core.active_account = Some("00000000-0000-4000-8000-000000000077".into());
    reopen(&mut s);
    let _a = start(&mut s, "New");
    crate::self_status::open_shell(&mut s, now());
    assert!(crate::self_status::begin(&mut s, now()).is_err());
    assert!(!crate::self_status::prepare_clear(&mut s, now()));
    crate::profile_edit::fixture::install(&mut s);
    assert!(
        crate::profile_edit::presentation(&s)
            .unwrap()
            .status_conflict
    );
    let mut s = shell();
    let o = Owner::capture(&s).unwrap();
    s.core.session = Some(super_platinum_core::config::Session {
        d_cookie: "fixture-cookie".into(),
        workspaces: [(o.team.clone(), fixture::session(&o))].into(),
    });
    s.core.transport = Some(Arc::new(
        super_platinum_core::slack::Transport::new("fixture-cookie").unwrap(),
    ));
    crate::self_status::open_shell(&mut s, now());
    let _a = crate::self_status::begin(&mut s, now()).unwrap();
    let (t, v) = open_shell(&mut s).unwrap();
    let cap = fixture::snapshot(&s, &t.owner);
    finish_load(&mut s, &t, v, Ok(cap));
    edit(&mut s, "New");
    assert!(!presentation(&s).unwrap().can_save);
}
#[test]
fn overlength_and_cooldown_block_admission_without_clearing_draft() {
    let mut s = shell();
    let v = edit(&mut s, &"x".repeat(250));
    assert!(begin(&mut s, v).is_none());
    edit(&mut s, "New");
    let a = begin(&mut s, v).unwrap();
    preflight_failed(
        &mut s,
        &a,
        Some(&Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(!presentation(&s).unwrap().can_save);
    close_visit(&mut s, Some(v));
    assert!(open_shell(&mut s).is_none());
    assert!(!presentation(&s).unwrap().can_reload);
}
