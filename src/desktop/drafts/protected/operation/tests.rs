use super::*;
use crate::drafts::protected::account::wait;
use std::sync::atomic::AtomicU64;
use super_platinum_core::ComposerState;
const ID: &str = "00000000-0000-4000-8000-000000000397";
pub(crate) struct Fixture {
    _isolated: durable::test_support::Isolated,
    pub(crate) account: Arc<Account>,
    pub(crate) owner: Owner,
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
impl Fixture {
    pub(crate) async fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "c3-renderer-operation-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let isolated = durable::test_support::Isolated::create(ID.to_owned(), directory).unwrap();
        let account = Account::isolated(ID.to_owned(), isolated.client());
        account.initialize().await.unwrap();
        let owner = account
            .owner(DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            })
            .unwrap();
        let inventory = owner.local_inventory().await.unwrap();
        let lease = wait(account.client().unwrap().renew_local(inventory.clone()))
            .await
            .unwrap();
        let serial = lease.initial_serial();
        let inventory = match wait(account.client().unwrap().write_local(
            lease,
            serial,
            serial + 1,
            inventory.local().clone(),
            local("Original"),
        ))
        .await
        .unwrap()
        {
            durable::LocalWriteResult::Applied(value) => value,
            _ => panic!("first isolated input was not canonical"),
        };
        wait(
            account
                .client()
                .unwrap()
                .enroll_local(owner.scope().clone(), inventory),
        )
        .await
        .unwrap();
        Self {
            _isolated: isolated,
            account,
            owner,
        }
    }
    /// Test fixture setup through the existing private Account admission path.
    /// Return the issued Stream, never expose a production Client accessor.
    pub(crate) async fn verified_stream(&self) -> durable::Stream {
        let client = self.account.client().unwrap();
        let stamp = wait(client.stream_state("T1".into())).await.unwrap();
        let stream = wait(client.open_stream("T1".into(), stamp)).await.unwrap();
        wait(client.verify_local(stream.clone(), self.owner.review().await.unwrap()))
            .await
            .unwrap();
        stream
    }
    pub(crate) async fn operation(&self) -> Operation {
        let editor = editor::Editor::open(self.owner.clone()).await.unwrap();
        Operation::reserve(
            &editor,
            Seed {
                request: Request::Send,
                local: Arc::new(local("Original")),
                book: None,
                serial: editor.snapshot().serial,
                epoch: 7,
            },
        )
        .unwrap_or_else(|_| panic!("bounded exact operation"))
    }
}
#[tokio::test]
async fn reservation_keeps_exact_input_and_unknown_is_monotonic() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    assert_eq!(fixture.account.retained_operations().unwrap().len(), 1);
    assert_eq!(operation.record().local.composer.text, "Original");
    let review = fixture.owner.review().await.unwrap();
    operation.review(&review).unwrap();
    operation.unknown();
    operation.finish(Ok(Status::Finished));
    assert_eq!(operation.ready().await.unwrap(), Status::Unknown);
    assert!(
        operation
            .record()
            .evidence
            .lock()
            .unwrap()
            .review
            .as_ref()
            .unwrap()
            .same(&review)
    );
    assert!(matches!(
        operation.acknowledge(),
        Err(durable::Error::Pending)
    ));
}
#[tokio::test]
async fn only_exact_finished_operation_releases_account_capacity() {
    let fixture = Fixture::new().await;
    let finished = fixture.operation().await;
    let pending = fixture.operation().await;
    finished.finish(Ok(Status::Finished));
    pending.finish(Ok(Status::ConsumedAwaitingDelivery));
    assert!(matches!(
        pending.acknowledge(),
        Err(durable::Error::Pending)
    ));
    finished.acknowledge().unwrap();
    assert!(matches!(finished.acknowledge(), Err(durable::Error::Stale)));
    let retained = fixture.account.retained_operations().unwrap();
    assert_eq!(retained.len(), 1);
    assert!(retained[0].same(&pending));
}
#[tokio::test]
async fn poisoned_inventory_never_looks_empty_or_admits_new_operation() {
    let fixture = Fixture::new().await;
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = fixture.account.operations.lock().unwrap();
        panic!("injected poison");
    }));
    assert!(matches!(
        fixture.account.retained_operations(),
        Err(durable::Error::Storage)
    ));
    let input = Arc::new(local("Refused input"));
    let refused = match Operation::reserve(
        &editor,
        Seed {
            request: Request::Send,
            local: input.clone(),
            book: None,
            serial: 3,
            epoch: 7,
        },
    ) {
        Ok(_) => panic!("poison must refuse"),
        Err(value) => value,
    };
    assert_eq!(refused.error, durable::Error::Storage);
    assert!(Arc::ptr_eq(&refused.seed.local, &input));
}
#[tokio::test]
async fn bounded_reservation_refusal_keeps_original_request() {
    let fixture = Fixture::new().await;
    for _ in 0..MAX_OPERATIONS {
        fixture.operation().await;
    }
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    let input = Arc::new(local("Still caller owned"));
    let refused = match Operation::reserve(
        &editor,
        Seed {
            request: Request::Send,
            local: input.clone(),
            book: None,
            serial: 1,
            epoch: 7,
        },
    ) {
        Ok(_) => panic!("capacity must refuse"),
        Err(value) => value,
    };
    assert_eq!(refused.error, durable::Error::Pending);
    assert!(Arc::ptr_eq(&refused.seed.local, &input));
    assert_eq!(
        fixture.account.retained_operations().unwrap().len(),
        MAX_OPERATIONS
    );
}
#[tokio::test]
async fn preservation_failure_retains_error_and_never_claims_durable_success() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    assert_eq!(
        operation.preservation(Err(durable::Error::Storage)),
        Err(durable::Error::Storage)
    );
    assert_eq!(
        operation.record().evidence.lock().unwrap().preservation,
        Some(Err(durable::Error::Storage))
    );
    operation.finish(Err(durable::Error::Storage));
    assert_eq!(
        operation.ready().await.unwrap(),
        Status::Failed(durable::Error::Storage)
    );
}

#[tokio::test]
async fn counter_exhaustion_returns_exact_schedule_payload_without_record() {
    let fixture = Fixture::new().await;
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    fixture.account.operations.lock().unwrap().next = u64::MAX;
    let input = Arc::new(local("Still here"));
    let desired = fixture
        .owner
        .review()
        .await
        .unwrap()
        .row()
        .state()
        .desired()
        .unwrap()
        .clone();
    let refused = match Operation::reserve(
        &editor,
        Seed {
            request: Request::Schedule {
                snapshot: desired.clone(),
                post_at: 12345,
            },
            local: input.clone(),
            book: None,
            serial: 3,
            epoch: 7,
        },
    ) {
        Ok(_) => panic!("counter must refuse"),
        Err(value) => value,
    };
    assert_eq!(refused.error, durable::Error::Unsupported);
    assert!(Arc::ptr_eq(&refused.seed.local, &input));
    assert!(
        matches!(refused.seed.request, Request::Schedule { snapshot, post_at: 12345 } if snapshot == desired)
    );
    assert!(fixture.account.retained_operations().unwrap().is_empty());
}
#[tokio::test]
async fn actual_owned_consume_retains_exact_result_before_return() {
    let fixture = Fixture::new().await;
    let client = fixture.account.client().unwrap();
    let stamp = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), stamp)).await.unwrap();
    let before = fixture.owner.review().await.unwrap();
    wait(client.verify_local(stream.clone(), before))
        .await
        .unwrap();
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    let lease = editor
        .prepare_review(Visit::new(), &local("Original"))
        .await
        .unwrap();
    let operation = Operation::reserve(
        &editor,
        Seed {
            request: Request::Send,
            local: Arc::new(local("Original")),
            book: None,
            serial: 0,
            epoch: 7,
        },
    )
    .unwrap_or_else(|_| panic!("reserve"));
    let applied = editor
        .apply_review_owned(
            lease,
            Action::Consume(super_platinum_core::drafts::sync::ConsumeKind::Send),
            operation.clone(),
            move || Ok(Some(stream)),
        )
        .await
        .unwrap();
    let evidence = operation.record().evidence.lock().unwrap();
    let retained = evidence.consumed.as_ref().unwrap();
    assert!(retained.before.same(&applied.before));
    assert!(retained.after.same(&applied.after));
    assert!(applied.after.local().empty());
    assert!(applied.after.row().state().consumption().is_some());
    assert_eq!(retained.native_clear, NativeClear::NotAttempted);
    drop(evidence);
    assert!(fixture.owner.review().await.unwrap().same(&applied.after));
    assert!(!operation.native_cleared().unwrap());
}
#[tokio::test]
async fn publication_evidence_survives_unknown_during_release() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    operation
        .retain(|state| {
            state.publication.initial_inventory = Some(true);
            state.publication.send_helper_invoked = Some(true);
            state.publication.inventory = Some(true);
            state.publication.queue = Some(false);
        })
        .unwrap();
    operation.phase(Phase::Release).unwrap();
    operation.unknown();
    assert_eq!(operation.ready().await.unwrap(), Status::Unknown);
    let evidence = operation.record().evidence.lock().unwrap();
    assert_eq!(evidence.phase, Phase::Release);
    assert_eq!(evidence.publication.inventory, Some(true));
    assert_eq!(evidence.publication.send_helper_invoked, Some(true));
    assert_eq!(evidence.publication.after_release_inventory, None);
}

#[tokio::test]
async fn actual_accepted_send_and_local_release_cannot_hide_deferred_native_clear() {
    use crate::drafts::{dispatch::Outcome, network::Attempt};
    let fixture = Fixture::new().await;
    let client = fixture.account.client().unwrap();
    let stamp = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), stamp)).await.unwrap();
    wait(client.verify_local(stream.clone(), fixture.owner.review().await.unwrap()))
        .await
        .unwrap();
    let editor = editor::Editor::open(fixture.owner.clone()).await.unwrap();
    let lease = editor
        .prepare_review(Visit::new(), &local("Original"))
        .await
        .unwrap();
    let operation = Operation::reserve(
        &editor,
        Seed {
            request: Request::Send,
            local: Arc::new(local("Original")),
            book: None,
            serial: 0,
            epoch: 7,
        },
    )
    .unwrap_or_else(|_| panic!("reserve"));
    let captured = stream.clone();
    let mut applied = editor
        .apply_review_owned(
            lease,
            Action::Consume(super_platinum_core::drafts::sync::ConsumeKind::Send),
            operation.clone(),
            move || Ok(Some(captured)),
        )
        .await
        .unwrap();
    applied.native_clear = NativeClear::Deferred;
    operation.consumed(&applied).unwrap();
    let intent = applied.after.row().state().consumption().unwrap().clone();
    let prepared = wait(client.prepare(stream, applied.after)).await.unwrap();
    let claimed = wait(client.claim_dispatch(prepared)).await.unwrap();
    let (issued, receipt) = claimed.into_parts();
    let accepted = wait(client.record(
        receipt.clone(),
        durable::Outcome::Sent {
            client_msg_id: issued.client_msg_id.clone(),
            draft_id: issued.base.as_ref().map(|v| v.id.clone()),
            ts: "100.000001".into(),
        },
        0,
    ))
    .await
    .unwrap();
    editor.refresh_receipt().await.unwrap();
    let handoff = editor
        .release_completion_owned(accepted.clone(), intent, operation.clone())
        .await;
    assert_eq!(handoff, Ok(()));
    let result = operation
        .delivered(delivery::Finished {
            dispatched: Dispatched {
                review: accepted,
                receipt,
                attempt: Arc::new(Attempt {
                    outcome: Outcome::Sent {
                        client_msg_id: issued.client_msg_id,
                        draft_id: issued.base.map(|v| v.id),
                        ts: "100.000001".into(),
                    },
                    cooldown_until_ms: None,
                    issue: None,
                    dispatched: true,
                    raw: None,
                }),
            },
            inventory_published: true,
            queue_published: false,
            local_handoff: handoff,
        })
        .unwrap();
    assert_eq!(result, Status::AwaitingRecovery);
    operation.finish(Ok(result));
    assert!(matches!(
        operation.acknowledge(),
        Err(durable::Error::Pending)
    ));
    let evidence = operation.record().evidence.lock().unwrap();
    assert!(evidence.release.is_some());
    assert!(evidence.local_lease.is_some());
    assert_eq!(evidence.handoff, Some(Ok(())));
    assert!(evidence.finished.is_some());
    drop(evidence);
    assert!(
        fixture
            .owner
            .local_inventory()
            .await
            .unwrap()
            .row()
            .is_none()
    );
}

#[tokio::test]
async fn operation_request_cannot_be_rebound_to_different_action_or_owner() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    let snapshot = fixture
        .owner
        .review()
        .await
        .unwrap()
        .row()
        .state()
        .desired()
        .unwrap()
        .clone();
    assert!(operation.permits(&Action::Consume(
        super_platinum_core::drafts::sync::ConsumeKind::Send
    )));
    assert!(!operation.permits(&Action::Schedule {
        snapshot,
        post_at: 12345
    }));
    let other = Fixture::new().await;
    assert!(!operation.belongs_owner(&other.owner));
    assert!(operation.belongs_owner(&fixture.owner));
}
