//! Faults are armed only on the core-owned isolated worker's final commit.
use super::super::{editor::Editor, journal};
use super::*;
use durable::test_support::FaultPoint;

#[tokio::test]
async fn checkpoint_failure_retains_exact_retry_and_newer_typing() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    wait(fixture._isolated.fail_next(FaultPoint::WriterCommit))
        .await
        .unwrap();
    editor.submit(&local("First input")).unwrap();
    assert!(matches!(editor.flush().await, Err(durable::Error::Storage)));
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Original"
    );
    editor.submit(&local("Newer input")).unwrap();
    let failed = editor.snapshot();
    assert_eq!(failed.failed_inputs[0].composer.text, "First input");
    assert_eq!(failed.pending.unwrap().composer.text, "Newer input");
    editor.retry_storage().unwrap();
    let saved = editor.flush().await.unwrap();
    assert!(saved.failed_inputs.is_empty());
    assert!(saved.pending.is_none());
    assert_eq!(saved.view.local().composer.text, "Newer input");
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Newer input"
    );
}

#[tokio::test]
async fn released_local_failure_retries_the_exact_predecessor_once() {
    let fixture = Fixture::new().await;
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    editor.release().await.unwrap();
    let lane = editor.state.lock().unwrap().released.clone().unwrap();
    let before = fixture.owner.local_inventory().await.unwrap();
    wait(fixture._isolated.fail_next(FaultPoint::LocalCommit))
        .await
        .unwrap();
    let serial = lane.submit(&local("Local only")).unwrap();
    assert!(matches!(lane.flush().await, Err(durable::Error::Storage)));
    assert!(fixture.owner.local_inventory().await.unwrap().same(&before));
    assert_eq!(lane.snapshot().failed_inputs.len(), 1);
    lane.retry_storage(serial).unwrap();
    let current = lane.flush().await.unwrap();
    assert!(current.failed_inputs.is_empty());
    assert!(current.pending.is_none());
    assert!(current.inventory.as_ref().unwrap().row().is_none());
    assert_eq!(
        current.inventory.unwrap().local().composer.text,
        "Local only"
    );
    assert!(matches!(
        lane.retry_storage(serial),
        Err(durable::Error::Stale)
    ));
}

#[tokio::test]
async fn mutation_completion_disk_retry_keeps_newer_input_and_never_has_transport() {
    use crate::drafts::{dispatch::Outcome, network::Attempt};
    use super_platinum_core::drafts::sync::RemoteDraft;
    let fixture = Fixture::new().await;
    let client = fixture.account.client().unwrap();
    let stream_state = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), stream_state))
        .await
        .unwrap();
    let before = fixture.owner.review().await.unwrap();
    let verified = wait(client.verify_local(stream.clone(), before))
        .await
        .unwrap();
    let editor = Editor::open(fixture.owner.clone()).await.unwrap();
    let prepared = wait(client.prepare(stream, verified)).await.unwrap();
    let claimed = wait(client.claim_dispatch(prepared)).await.unwrap();
    let (operation, receipt) = claimed.into_parts();
    let remote = RemoteDraft {
        scope: operation.scope.clone(),
        id: "D1".into(),
        client_msg_id: operation.client_msg_id.clone(),
        last_updated_ts: "10.000001".into(),
        snapshot: operation.snapshot.clone().unwrap(),
    };
    let completion = Arc::new(journal::Mutation {
        parent: None,
        receipt,
        attempt: Arc::new(Attempt {
            outcome: Outcome::Draft(remote),
            cooldown_until_ms: None,
            issue: None,
            dispatched: true,
            raw: Some(serde_json::json!({"ok":true})),
        }),
    });
    fixture.account.journal.lock().unwrap().mutations.insert(
        (fixture.owner.scope().key.clone(), operation.id.clone()),
        completion.clone(),
    );
    // Fill other-scope job slots: repairing retained evidence replaces its
    // existing reservation, rather than requiring another free admission.
    {
        let mut journal = fixture.account.journal.lock().unwrap();
        for id in 1..super::super::retention::MAX_JOBS as u64 {
            let mut scope = fixture.owner.scope().clone();
            scope.key.channel = format!("OTHER{id}");
            journal.active.insert(id, scope);
        }
    }
    assert!(matches!(
        journal::Active::begin(&fixture.owner),
        Err(durable::Error::Pending)
    ));
    let repair = journal::Repair::begin(
        &fixture.owner,
        &operation.id,
        journal::RetainedKind::MutationStorage,
    )
    .unwrap();
    assert!(matches!(
        journal::Repair::begin(
            &fixture.owner,
            &operation.id,
            journal::RetainedKind::MutationStorage
        ),
        Err(durable::Error::Pending)
    ));
    drop(repair);
    wait(fixture._isolated.fail_next(FaultPoint::MutationReceipt))
        .await
        .unwrap();
    assert!(matches!(
        fixture.owner.retry_mutation_storage(&operation.id).await,
        Err(durable::Error::Storage)
    ));
    assert_eq!(fixture.account.journal.lock().unwrap().mutations.len(), 1);
    editor
        .submit(&local("Typed while receipt was unavailable"))
        .unwrap();
    editor.flush().await.unwrap();
    let result = fixture
        .owner
        .retry_mutation_storage(&operation.id)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&result.attempt, &completion.attempt));
    assert_eq!(
        result.review.local().composer.text,
        "Typed while receipt was unavailable"
    );
    assert!(fixture.account.journal.lock().unwrap().mutations.is_empty());
    assert!(matches!(
        fixture.owner.retry_mutation_storage(&operation.id).await,
        Err(durable::Error::Pending)
    ));
}

#[tokio::test]
async fn failed_gap_commit_keeps_process_barrier_until_explicit_disk_repair() {
    let fixture = Fixture::new().await;
    let client = fixture.account.client().unwrap();
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    wait(fixture._isolated.fail_next(FaultPoint::GapCommit))
        .await
        .unwrap();
    fixture.account.retire_stream(
        stream.clone(),
        durable::GapReason::Disconnect,
        Some(tokio::runtime::Handle::current()),
    );
    assert!(fixture.account.team_blocked("T1"));
    let review = fixture.owner.review().await.unwrap();
    assert!(matches!(
        wait(client.verify_local(stream, review)).await,
        Err(durable::Error::Gap)
    ));
    // Retry is explicit and disk-only. Waiting for the detached failure handoff
    // does not open a socket, stream or transport request.
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            match fixture.account.retry_gap("T1") {
                Ok(()) => break,
                Err(durable::Error::Pending) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected repair refusal: {error:?}"),
            }
        }
        while fixture.account.team_blocked("T1") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!fixture.account.team_blocked("T1"));
}

#[tokio::test]
async fn status_completion_failure_keeps_one_ticket_and_nonpositive_uncertainty() {
    use crate::drafts::network::{Block, Readback};
    use super_platinum_core::drafts::sync::{ConsumeKind, Workflow};
    let fixture = Fixture::new().await;
    let client = fixture.account.client().unwrap();
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    let verified = wait(client.verify_local(stream.clone(), fixture.owner.review().await.unwrap()))
        .await
        .unwrap();
    let consumed = wait(client.consume(stream.clone(), verified, ConsumeKind::Send))
        .await
        .unwrap();
    let ticket = wait(client.prepare(stream, consumed)).await.unwrap();
    let claimed = wait(client.claim_dispatch(ticket)).await.unwrap();
    let (operation, receipt) = claimed.into_parts();
    let uncertain = wait(client.record(receipt.clone(), durable::Outcome::Uncertain, 0))
        .await
        .unwrap();
    let recovery = fixture.owner.take_over().await.unwrap();
    let ticket = wait(client.reserve_check(recovery.clone(), uncertain, receipt, 1_000_000))
        .await
        .unwrap();
    let completion = match wait(client.claim_check(recovery, ticket, 1_000_000))
        .await
        .unwrap()
    {
        durable::CheckClaim::Ready {
            operation: claimed,
            completion,
        } => {
            assert_eq!(operation, claimed);
            completion
        }
        _ => panic!("new isolated check unexpectedly deferred"),
    };
    let read = Arc::new(Readback::blocked(Block::Session));
    let retained = Arc::new(journal::Check {
        completion,
        result: super::super::outcome::check(&read),
        read,
    });
    fixture.account.journal.lock().unwrap().checks.insert(
        (fixture.owner.scope().key.clone(), operation.id.clone()),
        retained.clone(),
    );
    wait(fixture._isolated.fail_next(FaultPoint::CheckCompletion))
        .await
        .unwrap();
    assert!(matches!(
        fixture.owner.retry_check_storage(&operation.id).await,
        Err(durable::Error::Storage)
    ));
    assert_eq!(fixture.account.journal.lock().unwrap().checks.len(), 1);
    let completed = fixture
        .owner
        .retry_check_storage(&operation.id)
        .await
        .unwrap();
    assert!(!completed.receipt.positive_accepted());
    assert!(Arc::ptr_eq(&completed.read, &retained.read));
    assert!(matches!(
        fixture
            .owner
            .review()
            .await
            .unwrap()
            .row()
            .state()
            .workflow(),
        Workflow::Uncertain(_)
    ));
    assert!(fixture.account.journal.lock().unwrap().checks.is_empty());
    assert!(matches!(
        fixture.owner.retry_check_storage(&operation.id).await,
        Err(durable::Error::Pending)
    ));
}
