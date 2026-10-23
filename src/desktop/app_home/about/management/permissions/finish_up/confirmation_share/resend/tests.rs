use super::*;
use std::time::Duration;
fn rate(delay: Option<u64>) -> SendAttempt {
    SendAttempt {
        number: 1,
        status: Some(429),
        outcome: Outcome::RateLimited,
        raw: None,
        response_text: None,
        retry_after_millis: delay,
        may_have_applied: false,
    }
}
#[test]
fn retry_after_defaults_to_sixty_seconds_and_shorter_observation_never_releases_early() {
    let now = tokio::time::Instant::now();
    let mut c = Cooldown::default();
    c.observe_at(&rate(None), now);
    c.observe_at(&rate(Some(1)), now + Duration::from_secs(5));
    assert!(c.blocked_at(now + Duration::from_secs(59)));
    assert!(!c.blocked_at(now + Duration::from_secs(60)));
    c.observe_at(&rate(Some(30_000)), now + Duration::from_secs(50));
    assert!(c.blocked_at(now + Duration::from_secs(79)));
    assert!(!c.blocked_at(now + Duration::from_secs(80)));
}
#[test]
fn non_rate_failure_without_retry_delay_does_not_create_or_clear_a_deadline() {
    let now = tokio::time::Instant::now();
    let mut failure = rate(None);
    failure.outcome = Outcome::Unknown;
    failure.status = None;
    let mut c = Cooldown::default();
    c.observe_at(&failure, now);
    assert!(!c.blocked_at(now));
    c.observe_at(&rate(Some(12_000)), now);
    c.observe_at(&failure, now + Duration::from_secs(1));
    assert!(c.blocked_at(now + Duration::from_secs(11)));
    assert!(!c.blocked_at(now + Duration::from_secs(12)));
}
