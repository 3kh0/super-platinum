use super::*;

#[tokio::test]
async fn cancellation_before_or_after_callback_owns_exact_late_stream() {
    for callback_first in [false, true] {
        let f = Box::pin(Fixture::new()).await;
        let job = f.reserve();
        let expected = f.read(&job).await;
        let open = f.open(&job, expected);
        let (entered, at_callback) = oneshot::channel();
        let (release, held) = std::sync::mpsc::channel();
        let retained = job.clone();
        let retained_account = f.account.clone();
        open.start(&tokio::runtime::Handle::current(), move |result| {
            let stream = result.unwrap();
            if callback_first {
                retained.issued(stream.clone());
            }
            entered.send(()).unwrap();
            held.recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            if !callback_first {
                retained.issued(stream);
            }
            drop(retained_account);
        })
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), at_callback)
            .await
            .unwrap()
            .unwrap();
        assert!(
            open.result().unwrap().is_none(),
            "cached worker result is not callback completion"
        );
        let stamp = f.account.source_jobs_inventory().unwrap();
        drop(RunGuard {
            job: job.clone(),
            finished: false,
        });
        let h = f.account.retained_source_openings().unwrap();
        assert_eq!(
            f.account.acknowledge_source_opening(&h[0]),
            Err(durable::Error::Pending)
        );
        release.send(()).unwrap();
        let stream = open.ready().await.unwrap();
        assert!(!stream.is_live());
        settled_gap(&job).await.unwrap();
        assert_eq!(job.gap.attempts().len(), 1);
        job.abandon(); // A second notification cannot enqueue a second cleanup.
        assert_eq!(job.gap.attempts().len(), 1);
        assert_eq!(stamp.validate(&f.account), Err(ownership::Error::Changed));
        f.account.acknowledge_source_opening(&h[0]).unwrap();
    }
}

#[tokio::test]
async fn guard_canceled_before_first_poll_does_not_issue_storage() {
    let f = Box::pin(Fixture::new()).await;
    let job = f.reserve();
    let guard = RunGuard {
        job: job.clone(),
        finished: false,
    };
    let never_polled = async move {
        let _guard = guard;
        std::future::pending::<()>().await;
    };
    drop(never_polled);
    let state = job.state.lock().unwrap();
    assert!(state.abandoned);
    assert!(state.read.is_none() && state.open.is_none() && state.issued.is_none());
    drop(state);
    assert!(job.gap.attempts().is_empty());
    let h = f.account.retained_source_openings().unwrap();
    f.account.acknowledge_source_opening(&h[0]).unwrap();
}

#[tokio::test]
async fn read_reply_outlives_waiter_and_stale_source_cannot_be_recovered() {
    let f = Box::pin(Fixture::new()).await;
    let job = f.reserve();
    let read = ReplyJob::new(f.account.client().unwrap().stream_state("T1".into()));
    job.state.lock().unwrap().read = Some(read.clone());
    read.start(&tokio::runtime::Handle::current(), |_| ())
        .unwrap();
    drop(read.ready()); // Async waiter never owns the sole actual Reply.
    let expected = read.ready().await.unwrap();
    assert_eq!(expected.account(), f.account.id());
    let open = f.open(&job, expected);
    let retained = job.clone();
    open.start(&tokio::runtime::Handle::current(), move |r| {
        retained.issued(r.unwrap())
    })
    .unwrap();
    let source = Source::captured_with_job(
        f.account.clone(),
        f.socket.clone(),
        9,
        open.ready().await.unwrap(),
        &f.shell,
        Some(job.clone()),
    )
    .unwrap();
    {
        let mut state = job.state.lock().unwrap();
        state.source = Some((Arc::downgrade(&source), source.ownership()));
        state.phase = Phase::Delivered;
    }
    let h = f.account.retained_source_openings().unwrap();
    assert!(Arc::ptr_eq(&h[0].source().unwrap(), &source));
    source.gap(durable::GapReason::Disconnect);
    settled_gap(&job).await.unwrap();
    assert!(
        matches!(h[0].source(), Err(durable::Error::Gap)),
        "strong but revoked is not live"
    );
    assert_eq!(
        f.account.acknowledge_source_opening(&h[0]),
        Err(durable::Error::Pending)
    );
    drop(source);
    f.account.acknowledge_source_opening(&h[0]).unwrap();
}
