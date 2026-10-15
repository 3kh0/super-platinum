use super::*;
use std::time::Duration;
use super_platinum_core::ComposerState;

struct Fixture {
    _store: durable::test_support::Isolated,
    account: Arc<Account>,
    owner: Owner,
}
fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: ComposerState::with_text(text),
        attachments: vec![],
    }
}
impl Fixture {
    async fn new() -> Self {
        Self::seed(true).await
    }
    async fn seed(enroll: bool) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "c3-editor-open-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let id = "00000000-0000-4000-8000-000000000393";
        let store = durable::test_support::Isolated::create(id.into(), path).unwrap();
        let account = Account::isolated(id.into(), store.client());
        account.initialize().await.unwrap();
        let owner = account
            .owner(DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            })
            .unwrap();
        let initial = owner.local_inventory().await.unwrap();
        let lease = account::wait(account.client().unwrap().renew_local(initial.clone()))
            .await
            .unwrap();
        let serial = lease.initial_serial();
        let durable::LocalWriteResult::Applied(local) =
            account::wait(account.client().unwrap().write_local(
                lease,
                serial,
                serial + 1,
                initial.local().clone(),
                local("Original \u{1f98a}"),
            ))
            .await
            .unwrap()
        else {
            panic!("canonical local fixture");
        };
        if enroll {
            account::wait(
                account
                    .client()
                    .unwrap()
                    .enroll_local(owner.scope().clone(), local),
            )
            .await
            .unwrap();
        }
        Self {
            _store: store,
            account,
            owner,
        }
    }
    async fn open(&self) -> Open {
        let open = Request {
            owner: self.owner.clone(),
            expected: None,
        }
        .begin()
        .unwrap_or_else(|_| panic!("open admission"));
        ready(&open).await;
        open
    }
}

#[tokio::test]
async fn failed_and_unknown_started_openings_keep_actual_enrollment_blocked() {
    for unknown in [false, true] {
        let fixture = Fixture::seed(false).await;
        let before = fixture.owner.local_inventory().await.unwrap();
        let mut shell = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some(fixture.account.id().into());
        shell.core.account_epoch = 7;
        let mut discovery = super::super::discovery::Discovery::default();
        discovery.select(&shell, fixture.account.clone()).unwrap();
        let load = discovery.begin_load(&shell).unwrap();
        load.ready().await;
        discovery.finish_load(&shell, &load).unwrap();
        assert!(
            discovery
                .enrollment(&shell, &fixture.owner.scope().key, Visit::new())
                .is_ok()
        );
        let open = if unknown {
            let (owner, completion, _) = Request {
                owner: fixture.owner.clone(),
                expected: None,
            }
            .reserve()
            .unwrap_or_else(|_| panic!("reservation"));
            let open = Open(completion.job.clone());
            drop(owner);
            drop(completion);
            open
        } else {
            Request {
                owner: fixture.owner.clone(),
                expected: None,
            }
            .begin()
            .unwrap_or_else(|_| panic!("reservation"))
        };
        ready(&open).await;
        assert!(open.result().is_err());
        assert!(fixture.account.retained_editors().unwrap().is_empty());
        assert_eq!(fixture.account.editor_inventory().unwrap().len(), 1);
        assert!(matches!(
            discovery.enrollment(&shell, &fixture.owner.scope().key, Visit::new()),
            Err(durable::Error::Pending)
        ));
        assert!(fixture.owner.local_inventory().await.unwrap().same(&before));
    }
}
async fn ready(open: &Open) {
    tokio::time::timeout(Duration::from_secs(5), open.ready())
        .await
        .unwrap();
}
async fn waiting(account: &Arc<Account>, phase: Phase) -> Open {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(open) = account
                .retained_openings()
                .unwrap()
                .into_iter()
                .find(|open| open.0.evidence.lock().unwrap().phase == phase)
            {
                return open;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn public_waiter_cancellation_keeps_prejournal_reservation_and_same_editor() {
    let fixture = Fixture::new().await;
    let expected = fixture.owner.review().await.unwrap();
    let gate = fixture.owner.gate.clone().lock_owned().await;
    let owner = fixture.owner.clone();
    let task = tokio::spawn(async move { editor::Editor::open_expected(owner, expected).await });
    let retained = waiting(&fixture.account, Phase::WaitingOwner).await;
    let inventory = fixture.account.editor_inventory().unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].scope.as_ref(), Some(fixture.owner.scope()));
    assert_eq!(inventory[0].terminal, Some(Terminal::Running));
    assert!(!inventory[0].installed);
    assert!(fixture.account.retained_editors().unwrap().is_empty());
    assert!(fixture.account.journal.lock().unwrap().active.is_empty());
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    drop(gate);
    ready(&retained).await;
    let editor = retained.result().unwrap();
    assert!(Arc::ptr_eq(&editor, &retained.result().unwrap()));
    assert_eq!(fixture.account.retained_editors().unwrap().len(), 1);
    assert!(fixture.account.editor_inventory().unwrap()[0].installed);
    editor.submit(&local("After canceled waiter")).unwrap();
    editor.flush().await.unwrap();
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "After canceled waiter"
    );
    editor.retire_clean().await.unwrap();
}

#[tokio::test]
async fn open_none_retains_actual_read_and_writer_after_interrupted_install() {
    let fixture = Fixture::new().await;
    let (owner, completion, runtime) = Request {
        owner: fixture.owner.clone(),
        expected: None,
    }
    .reserve()
    .unwrap_or_else(|_| panic!("reserve"));
    let open = Open(completion.job.clone());
    // Exercise the real initialize/read/register chain. Simulate loss exactly
    // after the returned Writer is retained, before the installation transition.
    let (editor, owner_gate) = completion.run_inner(owner, runtime).await.unwrap();
    assert!(fixture.owner.gate.try_lock().is_err());
    drop(owner_gate);
    assert!(open.0.expected.is_none());
    let (review, writer) = {
        let evidence = open.0.evidence.lock().unwrap();
        assert_eq!(evidence.phase, Phase::Installing);
        (
            evidence.review.clone().unwrap(),
            evidence.writer.clone().unwrap(),
        )
    };
    assert!(review.same(&fixture.owner.review().await.unwrap()));
    drop(editor);
    drop(completion);
    ready(&open).await;
    assert!(matches!(open.result(), Err(durable::Error::Storage)));
    assert_eq!(
        fixture.account.editor_inventory().unwrap()[0].terminal,
        Some(Terminal::Unknown)
    );
    assert!(fixture.account.retained_editors().unwrap().is_empty());
    // Actual core writer proof is still usable for its exact original input;
    // this demonstrates real registration, not an invented job phase.
    let result = account::wait(
        fixture
            .owner
            .checkpoint(
                writer.clone(),
                writer.initial_serial() + 1,
                (*review).clone(),
                local("Kept after interrupted installation"),
                None,
            )
            .unwrap(),
    )
    .await
    .unwrap();
    assert!(matches!(result, durable::WriteResult::Applied(_)));
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Kept after interrupted installation"
    );
    assert_eq!(fixture.account.retained_openings().unwrap().len(), 1);
}

#[tokio::test]
async fn stale_expected_keeps_both_exact_proofs_without_registering_or_replaying() {
    let fixture = Fixture::new().await;
    let before = fixture.owner.review().await.unwrap();
    let active = fixture.open().await;
    let editor = active.result().unwrap();
    editor.submit(&local("New canonical input")).unwrap();
    editor.flush().await.unwrap();
    let after = fixture.owner.review().await.unwrap();
    let open = Request {
        owner: fixture.owner.clone(),
        expected: Some(before.clone()),
    }
    .begin()
    .unwrap_or_else(|_| panic!("reserve"));
    ready(&open).await;
    assert!(matches!(open.result(), Err(durable::Error::Stale)));
    let evidence = open.0.evidence.lock().unwrap();
    assert!(open.0.expected.as_ref().unwrap().same(&before));
    assert!(evidence.review.as_ref().unwrap().same(&after));
    assert!(evidence.writer.is_none());
    assert_eq!(evidence.phase, Phase::Reading);
    assert_eq!(evidence.terminal, Terminal::Failed(durable::Error::Stale));
    drop(evidence);
    assert!(fixture.owner.review().await.unwrap().same(&after));
    assert_eq!(fixture.account.retained_openings().unwrap().len(), 2);
    editor.retire_clean().await.unwrap();
    assert_eq!(fixture.account.editor_inventory().unwrap().len(), 1);
}

#[tokio::test]
async fn completion_exists_before_first_poll_and_failed_job_does_not_cycle_account() {
    let fixture = Fixture::new().await;
    let weak = Arc::downgrade(&fixture.account);
    let expected = fixture.owner.review().await.unwrap();
    let (owner, completion, _runtime) = Request {
        owner: fixture.owner.clone(),
        expected: Some(expected.clone()),
    }
    .reserve()
    .unwrap_or_else(|_| panic!("reserve"));
    let open = Open(completion.job.clone());
    assert_eq!(
        fixture.account.editor_inventory().unwrap()[0].phase,
        Some(Phase::Reserved)
    );
    drop(completion); // Equivalent to dropping a never-polled spawned future.
    drop(owner);
    ready(&open).await;
    assert!(open.0.expected.as_ref().unwrap().same(&expected));
    assert_eq!(open.0.evidence.lock().unwrap().terminal, Terminal::Unknown);
    drop(fixture);
    assert!(
        weak.upgrade().is_none(),
        "job must not hold Account strongly"
    );
    assert!(matches!(open.result(), Err(durable::Error::Storage)));
}

#[tokio::test]
async fn capacity_refusal_returns_original_payload_and_unused_reservations_rollback() {
    let fixture = Fixture::new().await;
    let slots: Vec<_> = (0..super::super::retention::MAX_EDITORS)
        .map(|_| fixture.account.reserve_editor().unwrap())
        .collect();
    let expected = fixture.owner.review().await.unwrap();
    let text_allocation = expected.local().composer.text.as_ptr();
    let request = Request {
        owner: fixture.owner.clone(),
        expected: Some(expected),
    };
    let refused = match request.begin() {
        Err(refused) => refused,
        Ok(_) => panic!("capacity admitted"),
    };
    assert_eq!(refused.error, durable::Error::Pending);
    assert_eq!(
        refused
            .request
            .expected
            .as_ref()
            .unwrap()
            .local()
            .composer
            .text
            .as_ptr(),
        text_allocation
    );
    assert!(Arc::ptr_eq(
        &refused.request.owner.account,
        &fixture.account
    ));
    assert!(fixture.account.retained_openings().unwrap().is_empty());
    assert_eq!(
        fixture.account.editor_inventory().unwrap().len(),
        super::super::retention::MAX_EDITORS
    );
    drop(slots);
    assert!(fixture.account.editor_inventory().unwrap().is_empty());
    let open = refused
        .request
        .begin()
        .unwrap_or_else(|_| panic!("same returned request"));
    ready(&open).await;
    open.result().unwrap().retire_clean().await.unwrap();
}

#[tokio::test]
async fn successful_retirement_drops_job_slot_without_second_editor_cycle() {
    let fixture = Fixture::new().await;
    let weak_account = Arc::downgrade(&fixture.account);
    let open = fixture.open().await;
    let editor = open.result().unwrap();
    let weak_editor = Arc::downgrade(&editor);
    editor.retire_clean().await.unwrap();
    assert!(fixture.account.retained_openings().unwrap().is_empty());
    assert!(fixture.account.editor_inventory().unwrap().is_empty());
    drop(editor);
    drop(fixture);
    assert!(weak_editor.upgrade().is_none());
    assert!(weak_account.upgrade().is_none());
    assert!(matches!(open.result(), Err(durable::Error::Stale)));
}

#[tokio::test]
async fn same_account_text_on_different_route_cannot_claim_open_identity() {
    let a = Fixture::new().await;
    let b = Fixture::new().await;
    assert_eq!(a.account.id(), b.account.id());
    let open = a.open().await;
    assert!(open.belongs_to(&a.account));
    assert!(!open.belongs_to(&b.account));
    assert!(b.account.retained_openings().unwrap().is_empty());
    open.result().unwrap().retire_clean().await.unwrap();
}

#[tokio::test]
async fn poisoned_outcome_is_explicit_unavailable_and_wait_never_hangs() {
    let fixture = Fixture::new().await;
    let (owner, completion, _runtime) = Request {
        owner: fixture.owner.clone(),
        expected: None,
    }
    .reserve()
    .unwrap_or_else(|_| panic!("reserve"));
    let open = Open(completion.job.clone());
    let job = open.0.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = job.evidence.lock().unwrap();
            panic!("poison isolated job");
        })
        .join()
        .is_err()
    );
    drop(completion);
    drop(owner);
    ready(&open).await;
    assert!(matches!(open.result(), Err(durable::Error::Storage)));
    assert!(matches!(
        fixture.account.editor_inventory(),
        Err(durable::Error::Storage)
    ));
    assert!(matches!(
        fixture.account.retained_openings(),
        Err(durable::Error::Storage)
    ));
}

#[test]
fn actual_runtime_shutdown_before_first_poll_retains_unknown_open() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let fixture = runtime.block_on(Fixture::new());
    let open = {
        let _entered = runtime.enter();
        Request {
            owner: fixture.owner.clone(),
            expected: None,
        }
        .begin()
        .unwrap_or_else(|_| panic!("reserve before spawn"))
    };
    // No block_on/poll after spawning: only the preconstructed guard can record
    // this cancellation. The isolated core worker is independent of this runtime.
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert_eq!(open.0.evidence.lock().unwrap().phase, Phase::Reserved);
    assert_eq!(open.0.evidence.lock().unwrap().terminal, Terminal::Unknown);
    assert!(matches!(open.result(), Err(durable::Error::Storage)));
    assert_eq!(fixture.account.editor_inventory().unwrap().len(), 1);
}
