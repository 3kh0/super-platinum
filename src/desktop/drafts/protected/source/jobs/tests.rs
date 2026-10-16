use super::*;
#[path = "fixture.rs"]
mod fixture;
use fixture::*;
#[path = "race_tests.rs"]
mod races;
#[path = "renderer_tests.rs"]
mod renderer;

#[test]
fn only_storage_holders_cross_the_existing_blocking_boundary() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Account>();
    send_sync::<Source>();
    send_sync::<Job>();
    send_sync::<ReplyJob<durable::Stream>>();
    // No assertion/widening for renderer Signal, Delivery or execution Evidence.
}

#[tokio::test]
async fn gap_rollback_keeps_actual_result_and_retries_same_child() {
    let f = Box::pin(Fixture::new()).await;
    let job = f.reserve();
    let stream = f.issued(&job).await;
    f.fault().await;
    job.abandon();
    assert_eq!(settled_gap(&job).await, Err(durable::Error::Storage));
    assert!(!stream.is_live());
    assert!(f.account.team_blocked("T1"));
    let first = job.gap.attempts()[0].clone();
    let stamp = f.account.source_jobs_inventory().unwrap();
    let generation = job.gap.generation("T1").unwrap();
    // Actual public Account repair must select this holder, not legacy enqueue.
    f.account.retry_gap("T1").unwrap();
    settled_gap(&job).await.unwrap();
    assert_eq!(job.gap.generation("T1"), Some(generation));
    assert_eq!(job.gap.attempts().len(), 2);
    assert_eq!(first.result().unwrap(), Some(Err(durable::Error::Storage)));
    assert!(!f.account.team_blocked("T1"));
    assert_eq!(stamp.validate(&f.account), Err(ownership::Error::Changed));
    assert!(matches!(job.retry_gap(), Err(durable::Error::Stale)));
    let handles = f.account.retained_source_openings().unwrap();
    f.account.acknowledge_source_opening(&handles[0]).unwrap();
    assert!(f.account.retained_source_openings().unwrap().is_empty());
}

#[tokio::test]
async fn delivery_drop_retires_actual_source_without_account_cycle() {
    let f = Box::pin(Fixture::new()).await;
    let job = f.reserve();
    let stream = f.issued(&job).await;
    let source = Source::captured_with_job(
        f.account.clone(),
        f.socket.clone(),
        9,
        stream,
        &f.shell,
        Some(job.clone()),
    )
    .unwrap();
    {
        let mut s = job.state.lock().unwrap();
        s.source = Some((Arc::downgrade(&source), source.ownership()));
        s.phase = Phase::Delivered;
    }
    let handles = f.account.retained_source_openings().unwrap();
    assert!(Arc::ptr_eq(&handles[0].source().unwrap(), &source));
    assert!(matches!(
        f.account.acknowledge_source_opening(&handles[0]),
        Err(durable::Error::Pending)
    ));
    let weak = Arc::downgrade(&source);
    let snapshot = f.account.source_jobs_inventory().unwrap();
    let (send, receive) = oneshot::channel();
    assert!(send.send(source).is_ok());
    drop(receive); // Never polled: dropping actual result calls existing Source::drop.
    assert!(weak.upgrade().is_none());
    settled_gap(&job).await.unwrap();
    assert!(matches!(handles[0].source(), Err(durable::Error::Gap)));
    assert_eq!(
        snapshot.validate(&f.account),
        Err(ownership::Error::Changed)
    );
    f.account.acknowledge_source_opening(&handles[0]).unwrap();
}

#[tokio::test]
async fn old_completion_cannot_remove_newer_team_generation() {
    let f = Box::pin(Fixture::new()).await;
    let job = f.reserve();
    let stream = f.issued(&job).await;
    f.fault().await;
    job.abandon();
    assert_eq!(settled_gap(&job).await, Err(durable::Error::Storage));
    let old = job.gap.generation("T1").unwrap();
    let newer = f
        .account
        .block_team(&stream, durable::GapReason::ExplicitRecovery);
    assert!(newer > old);
    job.retry_gap().unwrap();
    settled_gap(&job).await.unwrap();
    assert!(f.account.team_blocked("T1"));
    assert_eq!(job.gap.attempts().len(), 2);
}

#[tokio::test]
async fn failed_cleanup_attempt_bound_preserves_all_actual_outcomes() {
    let f = Box::pin(Fixture::new()).await;
    let job = f.reserve();
    f.issued(&job).await;
    for n in 0..16 {
        f.fault().await;
        if n == 0 {
            job.abandon();
        } else {
            job.retry_gap().unwrap();
        }
        assert_eq!(settled_gap(&job).await, Err(durable::Error::Storage));
    }
    assert_eq!(job.gap.attempts().len(), 16);
    assert_eq!(job.retry_gap(), Err(durable::Error::Pending));
    let h = f.account.retained_source_openings().unwrap();
    assert_eq!(
        f.account.acknowledge_source_opening(&h[0]),
        Err(durable::Error::Pending)
    );
    assert!(f.account.team_blocked("T1"));
}

#[tokio::test]
async fn reservation_bound_and_owner_identity_never_drop_existing_entries() {
    let f = Box::pin(Fixture::new()).await;
    let other = Box::pin(Fixture::new()).await;
    for _ in 0..MAX_JOBS {
        f.reserve();
    }
    let before = f.account.source_jobs_inventory().unwrap();
    assert!(matches!(
        Job::reserve(
            &f.account,
            f.socket.clone(),
            9,
            "T1",
            tokio::runtime::Handle::current()
        ),
        Err(durable::Error::Pending)
    ));
    before.validate(&f.account).unwrap();
    let handles = f.account.retained_source_openings().unwrap();
    assert_eq!(handles.len(), MAX_JOBS);
    assert_eq!(
        other.account.acknowledge_source_opening(&handles[0]),
        Err(durable::Error::Stale)
    );
    assert_eq!(
        before.validate(&other.account),
        Err(ownership::Error::Replaced)
    );
    assert_eq!(
        f.account.acknowledge_source_opening(&handles[0]),
        Err(durable::Error::Pending)
    );
}

#[tokio::test]
async fn replacement_child_after_lookup_never_falls_through_legacy_retry() {
    let f = Box::pin(Fixture::new()).await;
    let job = f.reserve();
    let stream = f.issued(&job).await;
    // Existing unadapted barrier is deliberately outside this child's custody.
    f.account
        .block_team(&stream, durable::GapReason::Disconnect);
    f.fault().await;
    let result = f.account.retry_gap_interleaved("T1", || {
        job.retire(stream.clone(), durable::GapReason::InterruptedAdmission);
        let reply = job.gap.attempts()[0].clone();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(result) = reply.result().unwrap() {
                assert_eq!(result, Err(durable::Error::Storage));
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "actual gap callback finished"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    assert_eq!(result, Err(durable::Error::Pending));
    assert_eq!(job.gap.attempts().len(), 1);
    assert!(f.account.team_blocked("T1"));
    // The next explicit call discovers the new child, retains its second actual
    // attempt, and succeeds without a legacy write or a new Stream.
    f.account.retry_gap("T1").unwrap();
    settled_gap(&job).await.unwrap();
    assert_eq!(job.gap.attempts().len(), 2);
    assert!(!f.account.team_blocked("T1"));
}
