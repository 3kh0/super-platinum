//! Final HTTP guards observe issuer replacement even after a claim returned.
use super::*;

#[test]
fn claimed_dispatch_loses_live_authority_on_replacement_and_cannot_revive() {
    let f = Fixture::new();
    let (old, review) = f.verified("frozen");
    let old_clone = old.clone();
    let ticket = f.client.prepare(old.clone(), review).wait().unwrap();
    let claimed = f.client.claim_dispatch(ticket).wait().unwrap();
    assert!(old.is_live());
    let expected = f.client.stream_state("T1".into()).wait().unwrap();
    let replacement = f
        .client
        .open_stream("T1".into(), expected.clone())
        .wait()
        .unwrap();
    assert!(!old.is_live());
    assert!(!old_clone.is_live());
    assert!(replacement.is_live());
    // Even a failed stale replacement keeps the previously returned issuer dead.
    assert!(matches!(
        f.client.open_stream("T1".into(), expected).wait(),
        Err(Error::Stale)
    ));
    assert!(!replacement.is_live());
    let next = f.open();
    assert!(next.is_live());
    assert!(!old.is_live());
    assert!(!replacement.is_live());
    let (_, receipt) = claimed.into_parts();
    assert!(
        f.client
            .record(receipt, Outcome::Uncertain, 0)
            .wait()
            .is_ok()
    );
}

#[test]
fn failed_takeover_open_revokes_previous_stream_before_any_disk_work() {
    let f = Fixture::new();
    let (old, review) = f.verified("frozen");
    let ticket = f.client.prepare(old.clone(), review).wait().unwrap();
    let claimed = f.client.claim_dispatch(ticket).wait().unwrap();
    let marker_path = f.path.join("fixture.json");
    let marker = std::fs::read(&marker_path).unwrap();
    std::fs::write(&marker_path, b"fixture route unavailable").unwrap();
    let result = f.client.take_over("T1".into(), None).wait();
    std::fs::write(&marker_path, marker).unwrap();
    assert!(matches!(result, Err(Error::Storage)));
    assert!(!old.is_live());
    let next = f.open();
    assert!(next.is_live());
    assert!(!old.is_live());
    let (_, receipt) = claimed.into_parts();
    f.client
        .record(receipt, Outcome::Uncertain, 0)
        .wait()
        .unwrap();
}

#[test]
fn claimed_status_checks_exact_recovery_latch_after_takeover_and_restart() {
    let f = Fixture::new();
    let (stream, review) = f.verified("frozen send");
    let consumed = f
        .client
        .consume(stream.clone(), review, ConsumeKind::Send)
        .wait()
        .unwrap();
    let ticket = f.client.prepare(stream.clone(), consumed).wait().unwrap();
    let (_, receipt) = f.client.claim_dispatch(ticket).wait().unwrap().into_parts();
    let uncertain = f
        .client
        .record(receipt.clone(), Outcome::Uncertain, 0)
        .wait()
        .unwrap();
    let recovery = f.client.take_over("T1".into(), None).wait().unwrap();
    assert!(!stream.is_live());
    let ticket = f
        .client
        .reserve_check(recovery.clone(), uncertain, receipt, 1000)
        .wait()
        .unwrap();
    let CheckClaim::Ready { completion, .. } = f
        .client
        .claim_check(recovery.clone(), ticket, 1000)
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert!(recovery.is_live());
    let state = f.client.recovery_state("T1".into()).wait().unwrap();
    let next = f.client.take_over("T1".into(), state).wait().unwrap();
    assert!(!recovery.is_live());
    assert!(next.is_live());
    // The final host guard refuses HTTP; its already-issued completion can still
    // record proven non-dispatch without borrowing the new issuer's authority.
    f.client
        .complete_check(
            completion,
            CheckResult::Nonpositive(KnownReadFailure::NotRequested),
            0,
        )
        .wait()
        .unwrap();
    f.client.load(LoadMode::AfterRestart).wait().unwrap();
    assert!(!next.is_live());
    assert!(!recovery.is_live());
}

#[test]
fn isolated_route_shutdown_revokes_its_stream_without_touching_same_account_route() {
    let f = Fixture::new();
    let (one, _) = f.verified("first");
    let path =
        std::env::temp_dir().join(format!("protected-adapter-test-{}", uuid::Uuid::new_v4()));
    let owned = test_support::Isolated::create(f.scope.account_id.clone(), path.clone()).unwrap();
    let other = Fixture {
        client: owned.client(),
        _owned: owned,
        scope: f.scope.clone(),
        path,
    };
    let (two, _) = other.verified("second");
    assert!(one.is_live());
    assert!(two.is_live());
    drop(f);
    assert!(!one.is_live());
    assert!(two.is_live());
    drop(other);
    assert!(!two.is_live());
}

#[test]
fn release_and_local_handoff_commit_faults_keep_exact_retry_authority() {
    let f = Fixture::new();
    let before = f.seed("retained local");
    let writer = f.client.register_writer(before.clone()).wait().unwrap();
    f._owned
        .fail_next(test_support::FaultPoint::ReleaseCommit)
        .wait()
        .unwrap();
    assert!(matches!(
        f.client.release(before.clone()).wait(),
        Err(Error::Storage)
    ));
    assert!(f.current().same(&before));
    let release = f.client.release(before).wait().unwrap();
    let local_before = f.client.review_local(f.scope.clone()).wait().unwrap();
    assert!(local_before.row().is_none());
    f._owned
        .fail_next(test_support::FaultPoint::StartLocalCommit)
        .wait()
        .unwrap();
    assert!(matches!(
        f.client.start_local(release.clone(), writer.clone()).wait(),
        Err(Error::Storage)
    ));
    let local_after = f.client.review_local(f.scope.clone()).wait().unwrap();
    assert!(local_before.same(&local_after));
    let lease = f
        .client
        .start_local(release.clone(), writer.clone())
        .wait()
        .unwrap();
    assert!(matches!(
        f.client.start_local(release, writer).wait(),
        Err(Error::Stale)
    ));
    let LocalWriteResult::Applied(written) = f
        .client
        .write_local(
            lease.clone(),
            lease.initial_serial(),
            lease.initial_serial() + 1,
            local("retained local"),
            local("continued"),
        )
        .wait()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(written.local().composer.text, "continued");
}
