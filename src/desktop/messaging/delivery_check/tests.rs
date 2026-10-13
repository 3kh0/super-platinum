use super::*;
fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut s);
    s
}
#[test]
fn check_admission_is_one_inflight_and_retains_429_across_navigation() {
    let mut s = shell();
    let now = Instant::now();
    let action = fixture::action(&s, fixture::FIRST).unwrap();
    let attempt = begin(&mut s, action.clone(), now).unwrap();
    assert!(begin(&mut s, action.clone(), now).is_none());
    s.channel_generation += 1;
    assert_eq!(
        finish(
            &mut s,
            &attempt,
            Err(Error::RateLimited {
                retry_after_secs: Some(30)
            }),
            now
        ),
        Some(Duration::from_secs(30))
    );
    let reopened = fixture::action(&s, fixture::FIRST).unwrap();
    assert!(begin(&mut s, reopened.clone(), now + Duration::from_secs(29)).is_none());
    assert!(begin(&mut s, action, now + Duration::from_secs(31)).is_none());
    assert!(begin(&mut s, reopened, now + Duration::from_secs(30)).is_some());
}
#[test]
fn exact_readback_retires_only_requested_upload_and_missing_or_failed_reads_do_not() {
    let mut s = shell();
    for result in [
        Ok(HistoryPage::default()),
        Err(Error::Transport("page budget".into())),
        Err(Error::Offline("offline".into())),
    ] {
        let a = fixture::action(&s, fixture::FIRST).unwrap();
        let attempt = begin(&mut s, a, Instant::now()).unwrap();
        finish(&mut s, &attempt, result, Instant::now());
        assert_eq!(s.core.pending_file_messages.len(), 2);
    }
    let page = fixture::canonical(&s, fixture::FIRST);
    let a = fixture::action(&s, fixture::FIRST).unwrap();
    let attempt = begin(&mut s, a, Instant::now()).unwrap();
    finish(&mut s, &attempt, Ok(page), Instant::now());
    assert_eq!(s.core.pending_file_messages.len(), 1);
    assert_eq!(
        s.core.pending_file_messages[0].client_msg_id,
        fixture::SECOND
    );
}
#[test]
fn delayed_result_and_queued_click_do_not_cross_visit_account_or_transport() {
    for change in 0..4 {
        let mut s = shell();
        if change == 3 {
            s.core.transport = Some(Arc::new(Transport::new("same-cookie").unwrap()));
        }
        let a = fixture::action(&s, fixture::FIRST).unwrap();
        let page = fixture::canonical(&s, fixture::FIRST);
        let attempt = begin(&mut s, a.clone(), Instant::now()).unwrap();
        match change {
            0 => s.channel_generation += 1,
            1 => s.core.active_account = Some("other-owner".into()),
            _ => s.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap())),
        }
        assert!(begin(&mut s, a, Instant::now()).is_none());
        finish(&mut s, &attempt, Ok(page), Instant::now());
        assert_eq!(s.core.pending_file_messages.len(), 2);
        assert!(!s.upload_checks.entries[&attempt.action.key].busy);
    }
}
#[test]
fn wrong_sender_or_remote_deletion_never_confirms_delivery() {
    for deleted in [false, true] {
        let mut s = shell();
        let a = fixture::action(&s, fixture::FIRST).unwrap();
        let mut page = fixture::canonical(&s, fixture::FIRST);
        if deleted {
            s.core
                .workspaces
                .get_mut(&a.key.team)
                .unwrap()
                .messages
                .get_mut(&a.key.channel)
                .unwrap()
                .record_remote_deletion(page.messages[0].ts.as_deref().unwrap());
        } else {
            page.messages[0].user = Some("other-sender".into());
        }
        let attempt = begin(&mut s, a, Instant::now()).unwrap();
        finish(&mut s, &attempt, Ok(page), Instant::now());
        assert_eq!(s.core.pending_file_messages.len(), 2);
    }
}

#[test]
fn another_account_is_not_blocked_by_prior_cooldown_and_huge_deadlines_do_not_overflow() {
    let mut s = shell();
    let now = Instant::now();
    let old = fixture::action(&s, fixture::FIRST).unwrap();
    let attempt = begin(&mut s, old, now).unwrap();
    assert!(
        finish(
            &mut s,
            &attempt,
            Err(Error::RateLimited {
                retry_after_secs: Some(u64::MAX)
            }),
            now
        )
        .is_none()
    );
    assert!(s.upload_checks.cooldowns[&attempt.action.read_owner()].overflow);
    let same = fixture::action(&s, fixture::FIRST).unwrap();
    assert!(begin(&mut s, same, now).is_none());
    s.core.active_account = Some("another-account".into());
    for p in &mut s.core.pending_file_messages {
        p.finalized.as_mut().unwrap().account = s.core.active_account.clone();
    }
    let fresh = fixture::action(&s, fixture::FIRST).unwrap();
    assert!(begin(&mut s, fresh, now).is_some());
}

#[test]
fn shared_endpoint_deadline_blocks_other_receipts_and_late_shorter_errors() {
    let mut s = shell();
    let now = Instant::now();
    let first = fixture::action(&s, fixture::FIRST).unwrap();
    let second = fixture::action(&s, fixture::SECOND).unwrap();
    let a = begin(&mut s, first.clone(), now).unwrap();
    let b = begin(&mut s, second.clone(), now).unwrap();
    finish(
        &mut s,
        &a,
        Err(Error::RateLimited {
            retry_after_secs: Some(30),
        }),
        now,
    );
    // A parallel scan cannot send its next page after the first receives 429.
    assert!(!read_allowed(&s, &b, now + Duration::from_secs(1)));
    finish(
        &mut s,
        &b,
        Err(Error::RateLimited {
            retry_after_secs: Some(2),
        }),
        now + Duration::from_secs(1),
    );
    s.channel_generation += 1;
    let reopened = fixture::action(&s, fixture::SECOND).unwrap();
    assert!(begin(&mut s, reopened.clone(), now + Duration::from_secs(29)).is_none());
    assert!(begin(&mut s, reopened, now + Duration::from_secs(30)).is_some());
    // A genuinely different authenticated cookie has independent quota state.
    s.core.transport = Some(Arc::new(Transport::new("other-credential").unwrap()));
    let fresh = fixture::action(&s, fixture::FIRST).unwrap();
    assert!(begin(&mut s, fresh, now + Duration::from_secs(2)).is_some());
}

#[test]
fn api_rate_limit_without_http_429_also_blocks_other_receipts() {
    for code in ["ratelimited", "rate_limited"] {
        let mut s = shell();
        let now = Instant::now();
        let first = fixture::action(&s, fixture::FIRST).unwrap();
        let attempt = begin(&mut s, first, now).unwrap();
        finish(&mut s, &attempt, Err(Error::Api(code.into())), now);
        let second = fixture::action(&s, fixture::SECOND).unwrap();
        assert!(begin(&mut s, second.clone(), now + Duration::from_secs(29)).is_none());
        assert!(begin(&mut s, second, now + Duration::from_secs(30)).is_some());
    }
}
