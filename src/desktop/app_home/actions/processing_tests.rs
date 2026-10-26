//! Real attempt identities and local time policy; native clicks are fixture-only proof.
use super::*;
fn setup() -> (ShellState, Key) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    let h = s.app_home.surface.as_ref().unwrap();
    let key = Key {
        visit: h.id,
        revision: h.revision,
    };
    (s, key)
}
fn attempt(s: &ShellState, id: u64, until: Instant) -> Attempt {
    let h = s.app_home.surface.as_ref().unwrap();
    Attempt {
        id,
        owner: h.owner.clone(),
        button: h
            .view
            .as_ref()
            .unwrap()
            .button("entry", "review", &h.owner.ctx.session.user_id)
            .unwrap(),
        route: context::Witness::capture(s).unwrap(),
        phase: Phase::Pending,
        repeatable: true,
        processing_until: Some(until),
    }
}
#[test]
fn one_second_window_is_independent_from_pending_and_uncertain_response() {
    let (s, _) = setup();
    let start = Instant::now();
    let mut a = attempt(&s, 1, start + Duration::from_secs(1));
    assert!(a.blocks_click(start + Duration::from_millis(999)));
    assert!(!a.blocks_click(start + Duration::from_millis(1000)));
    a.phase = Phase::Uncertain;
    assert!(a.blocks_click(start + Duration::from_millis(999)));
    assert!(!a.blocks_click(start + Duration::from_secs(1)));
}
#[test]
fn fast_success_keeps_click_gate_until_finite_expiry() {
    let (mut s, _) = setup();
    let until = Instant::now() + Duration::from_secs(1);
    s.app_home.attempts.push(attempt(&s, 1, until));
    complete(&mut s, 1);
    assert_eq!(s.app_home.attempts[0].phase, Phase::Completed);
    assert!(s.app_home.attempts[0].blocks_click(Instant::now()));
    expire_processing(&mut s, 1, until);
    assert!(s.app_home.attempts.is_empty());
}
#[test]
fn expiry_does_not_retire_pending_request_or_replace_its_identity() {
    let (mut s, key) = setup();
    let until = Instant::now() + Duration::from_secs(1);
    s.app_home.attempts.push(attempt(&s, 1, until));
    expire_processing(&mut s, 1, until);
    assert_eq!(s.app_home.attempts.len(), 1);
    assert!(dispatch_current(&s, 1, key));
    assert!(!s.app_home.attempts[0].blocks_click(until));
    s.app_home
        .attempts
        .push(attempt(&s, 2, until + Duration::from_secs(1)));
    assert!(dispatch_current(&s, 2, key));
    complete(&mut s, 1);
    assert_eq!(s.app_home.attempts.len(), 1);
    assert_eq!(s.app_home.attempts[0].id, 2);
    assert!(dispatch_current(&s, 2, key));
}
#[test]
fn uncertain_identity_cannot_dispatch_but_does_not_block_fresh_after_deadline() {
    let (mut s, key) = setup();
    let until = Instant::now();
    let mut a = attempt(&s, 1, until);
    a.phase = Phase::Uncertain;
    s.app_home.attempts.push(a);
    expire_processing(&mut s, 1, until);
    assert!(!dispatch_current(&s, 1, key));
    assert!(!s.app_home.attempts[0].blocks_click(until));
    s.app_home
        .attempts
        .push(attempt(&s, 2, until + Duration::from_secs(1)));
    assert!(dispatch_current(&s, 2, key));
    assert_eq!(s.app_home.attempts[0].phase, Phase::Uncertain);
}
#[test]
fn legacy_special_button_keeps_existing_pending_policy() {
    let (s, _) = setup();
    let now = Instant::now();
    let mut a = attempt(&s, 1, now);
    a.repeatable = false;
    a.processing_until = None;
    assert!(a.blocks_click(now + Duration::from_secs(5)));
    a.phase = Phase::Uncertain;
    assert!(a.blocks_click(now + Duration::from_secs(5)));
}

#[test]
fn older_timer_only_retires_its_own_completed_id() {
    let (mut s, key) = setup();
    let first = Instant::now() + Duration::from_secs(1);
    let second = first + Duration::from_secs(1);
    s.app_home.attempts.push(attempt(&s, 1, first));
    s.app_home.attempts.push(attempt(&s, 2, second));
    complete(&mut s, 1);
    expire_processing(&mut s, 1, first);
    assert_eq!(s.app_home.attempts.len(), 1);
    assert_eq!(s.app_home.attempts[0].id, 2);
    assert_eq!(s.app_home.attempts[0].processing_until, Some(second));
    assert!(s.app_home.attempts[0].blocks_click(first));
    assert!(dispatch_current(&s, 2, key));
    expire_processing(&mut s, 1, second);
    assert_eq!(s.app_home.attempts[0].processing_until, Some(second));
}
