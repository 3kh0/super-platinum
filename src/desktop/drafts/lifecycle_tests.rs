use super::*;

#[tokio::test]
async fn canceled_view_waits_for_the_original_restart_normalization() {
    let mut initialization = Initialization::New;
    let (started, began) = tokio::sync::oneshot::channel();
    let (release, wait) = tokio::sync::oneshot::channel();
    let mut first = Box::pin(initialization.finish(|| async move {
        started.send(()).unwrap();
        wait.await.unwrap();
        Ok(())
    }));
    tokio::select! {
        result = &mut first => panic!("normalization completed before release: {result:?}"),
        _ = began => {},
    }
    // An unmounted view drops its waiter, not the shared initialization task.
    drop(first);
    assert!(matches!(initialization, Initialization::Running(_)));
    release.send(()).unwrap();
    initialization
        .finish(|| async { panic!("must not start a second normalization") })
        .await
        .unwrap();
    assert!(matches!(initialization, Initialization::Ready));
    initialization
        .finish(|| async { panic!("a new account visit must not normalize live operations") })
        .await
        .unwrap();
}

#[tokio::test]
async fn completed_storage_failure_allows_an_explicit_new_load() {
    let mut initialization = Initialization::New;
    assert_eq!(
        initialization
            .finish(|| async { Err(Fault::Storage) })
            .await,
        Err(Fault::Storage)
    );
    assert!(matches!(initialization, Initialization::New));
    initialization.finish(|| async { Ok(()) }).await.unwrap();
    assert!(matches!(initialization, Initialization::Ready));
}
