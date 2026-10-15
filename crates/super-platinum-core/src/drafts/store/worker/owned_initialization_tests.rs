//! Actual owned worker and durable normalization, not a substitute initializer.
use super::*;
use crate::drafts::store::protected::{
    LoadMode,
    owned_store::{FactoryIdentity, OwnedRoute},
};

fn inspect(queue: &Queue) -> Arc<upgrade::Review> {
    let actual = queue
        .submit_upgrade(upgrade::Action::Inspect)
        .unwrap()
        .wait()
        .unwrap();
    let upgrade::Reply::Inspected(review) = &*actual else {
        panic!("inspection")
    };
    review.clone()
}
fn migrate(queue: &Queue) {
    let review = inspect(queue);
    queue
        .submit_upgrade(upgrade::Action::Commit(review))
        .unwrap()
        .wait()
        .unwrap();
    queue.open(inspect(queue)).unwrap();
}
fn public_opened() -> (
    OwnedRoute,
    crate::drafts::store::protected::owned_store::OpenedStore,
) {
    let route = OwnedRoute::fresh_fixture().unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    route.commit(&review).unwrap().wait().unwrap();
    let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
    (route, opened)
}

#[test]
fn cold_requires_actual_handoff_before_binding() {
    let queue = Queue::fresh().unwrap();
    let identity = Arc::new(());
    assert!(matches!(queue.cold(&identity), Err(Error::Stale)));
    assert!(queue.initialization.lock().unwrap().binding.is_none());
    migrate(&queue);
    assert!(queue.cold(&identity).unwrap().wait().is_ok());
}

#[test]
fn lost_job_handle_reattaches_exact_inventory_without_enqueuing_again() {
    let queue = Queue::fresh().unwrap();
    migrate(&queue);
    let identity = Arc::new(());
    let first = queue.cold(&identity).unwrap();
    let inventory = first.wait().unwrap();
    let generation = queue.worker.pending.lock().unwrap().upgrade_generation;
    drop(first);
    let again = queue.cold(&identity).unwrap();
    assert!(Arc::ptr_eq(&inventory, &again.wait().unwrap()));
    assert!(Arc::ptr_eq(&inventory, &again.poll().unwrap().unwrap()));
    assert_eq!(
        queue.worker.pending.lock().unwrap().upgrade_generation,
        generation
    );
    assert!(matches!(queue.cold(&Arc::new(())), Err(Error::Stale)));
}

#[test]
fn public_result_binds_actual_worker_and_refuses_unowned_restart_call() {
    let (route, opened) = public_opened();
    let root = FactoryIdentity::default();
    let first = opened.initialize_cold(&root).unwrap().wait().unwrap();
    assert_eq!(first.mode(), LoadMode::AfterRestart);
    assert_eq!(first.worker_id(), route.worker_id());
    assert_eq!(first.account(), route.account());
    assert!(first.inventory().managed().is_empty());
    assert!(matches!(
        first.client().load(LoadMode::AfterRestart).wait(),
        Err(Error::Stale)
    ));
    assert!(first.client().load(LoadMode::Current).wait().is_ok());
    assert!(opened.initialize_cold(&FactoryIdentity::default()).is_err());
    assert!(opened.initialize_cold(&root).unwrap().wait().is_ok());
}

#[test]
fn actual_preexisting_live_stream_refuses_cold_without_revoking_or_resetting() {
    use crate::drafts::store::protected::LocalWriteResult;
    let (route, opened) = public_opened();
    let client = opened.client();
    let scope = Scope {
        account_id: route.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let initial = client.review_local(scope.clone()).wait().unwrap();
    let lease = client.renew_local(initial.clone()).wait().unwrap();
    let serial = lease.initial_serial();
    let LocalWriteResult::Applied(current) = client
        .write_local(
            lease,
            serial,
            serial + 1,
            initial.local().clone(),
            crate::drafts::store::sync::tests::local("live before cold claim"),
        )
        .wait()
        .unwrap()
    else {
        panic!("canonical write")
    };
    client.enroll_local(scope, current).wait().unwrap();
    let state = client.stream_state("T1".into()).wait().unwrap();
    let stream = client
        .open_stream("T1".into(), state.clone())
        .wait()
        .unwrap();
    assert!(stream.is_live());
    assert!(matches!(
        opened.initialize_cold(&FactoryIdentity::default()),
        Err(Error::Stale)
    ));
    assert!(
        stream.is_live(),
        "refusal must precede actual restart revocation"
    );
    assert!(client.load(LoadMode::Current).wait().is_ok());
    assert!(stream.is_live());
}

#[test]
fn canceled_waiter_before_observing_load_reattaches_the_retained_job() {
    let queue = Queue::fresh().unwrap();
    migrate(&queue);
    let identity = Arc::new(());
    let first = queue.cold(&identity).unwrap();
    let retained = Arc::downgrade(&first);
    let generation = queue.worker.pending.lock().unwrap().upgrade_generation;
    drop(first); // no wait/poll: completion may legitimately win this race
    let again = queue.cold(&identity).unwrap();
    assert!(Arc::ptr_eq(&retained.upgrade().unwrap(), &again));
    assert_eq!(
        queue.worker.pending.lock().unwrap().upgrade_generation,
        generation
    );
    assert!(again.wait().is_ok());
}

#[test]
fn failed_open_is_retained_after_repair_without_retrying_normalization() {
    let queue = Queue::fresh().unwrap();
    migrate(&queue);
    let path = queue.owner.path().to_owned();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("DELETE FROM composer_draft_adapter_format", [])
        .unwrap();
    drop(conn);
    let identity = Arc::new(());
    let job = queue.cold(&identity).unwrap();
    assert!(job.wait().is_err());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO composer_draft_adapter_format(singleton,version) VALUES(1,1)",
        [],
    )
    .unwrap();
    drop(conn);
    let generation = queue.worker.pending.lock().unwrap().upgrade_generation;
    assert!(queue.cold(&identity).unwrap().wait().is_err());
    assert_eq!(
        queue.worker.pending.lock().unwrap().upgrade_generation,
        generation
    );
}

#[test]
fn migration_and_cold_normalization_are_separate_exact_revisions() {
    use crate::drafts::store::sync::{Commit, Prepare};
    use crate::drafts::sync::{DraftSync, RemoteDraft, Snapshot, Workflow};
    let queue = Queue::fresh().unwrap();
    let scope = Scope {
        account_id: queue.account().into(),
        key: DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
    };
    let snapshot = Snapshot {
        blocks: vec![serde_json::json!({"type":"future", "extra":{"keep":true}})],
        file_ids: vec![],
        destinations: vec![serde_json::json!({"channel_id":scope.key.channel})],
    };
    let remote = RemoteDraft {
        scope: scope.clone(),
        id: "D1".into(),
        client_msg_id: uuid::Uuid::new_v4().to_string(),
        last_updated_ts: "10.000001".into(),
        snapshot: snapshot.clone(),
    };
    let mut state = DraftSync::adopt_remote(&scope, remote).unwrap();
    let mut changed = snapshot;
    changed.blocks.push(serde_json::json!({"future_input":1}));
    state.edit(&scope, Some(changed));
    let local = crate::drafts::store::sync::tests::local("retained local");
    let mut store = DraftStore::open(queue.owner.path()).unwrap();
    store.save([(scope.key.clone(), local.clone())]).unwrap();
    let Commit::Applied(stored) = store
        .commit_synced(&scope, None, &local, &state, None)
        .unwrap()
    else {
        panic!("initial durable state")
    };
    let Prepare::Prepared { stored, operation } =
        store.prepare_synced(&scope, stored.revision).unwrap()
    else {
        panic!("durable operation")
    };
    let before = stored.revision;
    drop(store);
    migrate(&queue);
    let (reply, recv) = mpsc::channel();
    queue.submit_protected(ProtectedCommand {
        account: queue.account().into(),
        action: super::super::super::protected::commands::Action::Review {
            scope: scope.clone(),
            reply,
        },
    });
    let migrated = recv.recv().unwrap().unwrap();
    assert_eq!(migrated.row().revision(), before + 1);
    assert_eq!(migrated.row().state(), &stored.state);
    let identity = Arc::new(());
    let actual = queue.cold(&identity).unwrap().wait().unwrap();
    let normalized = actual.managed().get(&scope.key).unwrap();
    assert_eq!(normalized.row().revision(), before + 2);
    let mut expected = stored.state;
    assert!(expected.mark_uncertain(&scope, &operation.id));
    assert_eq!(normalized.row().state(), &expected);
    assert!(matches!(
        normalized.row().state().workflow(),
        Workflow::Uncertain(_)
    ));
    assert_eq!(
        serde_json::to_value(normalized.local()).unwrap(),
        serde_json::to_value(local).unwrap()
    );
    let again = queue.cold(&identity).unwrap().wait().unwrap();
    assert!(Arc::ptr_eq(&actual, &again));
}

#[test]
fn a_true_fixture_reopen_creates_a_new_cold_binding_on_the_same_durable_store() {
    let (route, opened) = public_opened();
    let ticket = opened.fixture_ticket().unwrap();
    let original_worker = route.worker_id().to_owned();
    let identity = FactoryIdentity::default();
    let initialized = opened.initialize_cold(&identity).unwrap().wait().unwrap();
    drop((initialized, opened, route));
    let reopened = OwnedRoute::reopen_fixture(&ticket).unwrap();
    assert_ne!(original_worker, reopened.worker_id());
    let opened = reopened.inspect().unwrap().wait().unwrap().open().unwrap();
    assert!(
        opened
            .initialize_cold(&FactoryIdentity::default())
            .unwrap()
            .wait()
            .is_ok()
    );
}
