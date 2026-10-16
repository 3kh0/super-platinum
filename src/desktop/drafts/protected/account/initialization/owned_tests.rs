use super::*;
use durable::owned_store as o;

fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: o::StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), o::JobState::Ready);
    job.result().unwrap().unwrap()
}
fn actual_owned() -> (durable::owned_store::Initialized, durable::Stream) {
    let route = o::OwnedRoute::fresh_fixture().unwrap();
    let before = route.inspect().unwrap().wait().unwrap();
    route.commit(&before).unwrap().wait().unwrap();
    let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
    let initialized = opened
        .initialize_cold(&o::FactoryIdentity::default())
        .unwrap()
        .wait()
        .unwrap();
    let ops = initialized.operations();
    let scope = Scope {
        account_id: ops.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let inventory = done(ops.local_inventory(scope.clone()).unwrap());
    done(ops.enroll(inventory).unwrap());
    let expected = done(
        ops.stream_state(o::TeamInput {
            scope: scope.clone(),
        })
        .unwrap(),
    );
    let stream = done(ops.open_stream(o::OpenInput { scope, expected }).unwrap());
    (initialized, stream)
}

#[tokio::test]
async fn both_supplied_owned_constructors_retain_actual_proof_and_never_submit_account_load() {
    let (initialized, stream) = tokio::task::spawn_blocking(actual_owned).await.unwrap();
    let factory = Account::from_initialized(initialized.clone());
    let retained = Account::from_retained(initialized.clone());
    for account in [factory, retained] {
        let snapshot = account.ownership_inventory().unwrap();
        assert_eq!(snapshot.initialization_status(), Status::Supplied);
        for _ in 0..3 {
            account.initialize().await.unwrap();
        }
        snapshot.validate(&account).unwrap();
        let state = account.initialization.state.lock().unwrap();
        assert!(state.task.is_none() && state.reply.is_none() && state.outcome.is_none());
        let proof = state.supplied.as_ref().unwrap();
        assert_eq!(proof.worker_id(), initialized.worker_id());
        assert_eq!(proof.account(), initialized.account());
        assert!(std::ptr::eq(proof.inventory(), initialized.inventory()));
        assert!(account.ready.load(Ordering::Acquire));
        assert!(
            stream.is_live(),
            "supplied success must not invoke another AfterRestart"
        );
    }
}

#[test]
fn supplied_owned_success_needs_no_executor_and_retains_historical_inventory() {
    let (initialized, stream) = actual_owned();
    let account = Account::from_retained(initialized.clone());
    let mut future = Box::pin(account.initialize());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert_eq!(
        std::future::Future::poll(future.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let state = account.initialization.state.lock().unwrap();
    assert!(state.task.is_none() && state.reply.is_none());
    assert!(std::ptr::eq(
        state.supplied.as_ref().unwrap().inventory(),
        initialized.inventory()
    ));
    assert!(stream.is_live());
}
