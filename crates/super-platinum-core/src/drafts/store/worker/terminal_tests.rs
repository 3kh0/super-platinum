//! Real FIFO/results and SQLite; no HTTP, fake drain, or production store.
use super::*;
use crate::drafts::store::terminal::{Execution as E, Inventory as I, Report};
use crate::drafts::store::{protected as p, sync::tests::local};
use std::sync::Arc;
struct Fixture {
    owner: Arc<p::upgrade::Owned>,
    worker: Worker,
}
impl Fixture {
    fn new() -> Self {
        let owner = p::upgrade::Owned::fresh().unwrap();
        let worker = Worker {
            protected_fixture: true,
            ..Default::default()
        };
        let id = worker
            .upgrade_id
            .get_or_init(|| uuid::Uuid::new_v4().to_string());
        owner.bind_worker(id).unwrap();
        worker.pending.lock().unwrap().accounting =
            Some(accounting::Book::owned(owner.account(), id));
        Self { owner, worker }
    }
    fn scope(&self) -> Scope {
        Scope {
            account_id: self.owner.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            },
        }
    }
    fn open(&self, account: &str) -> Result<DraftStore, AppError> {
        assert_eq!(account, self.owner.account());
        self.owner.check()?;
        DraftStore::open(self.owner.path())
    }
    fn run(&self) {
        self.worker.cycle(|account| self.open(account));
    }
    fn report(&self) -> Report {
        let pending = self.worker.pending.lock().unwrap();
        pending
            .accounting
            .as_ref()
            .unwrap()
            .snapshot(&pending)
            .records
            .last()
            .unwrap()
            .terminal
            .unwrap()
    }
    fn call<T>(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<T, p::Error>>) -> p::commands::Action,
    ) -> Result<T, p::Error> {
        let (reply, receive) = mpsc::channel();
        self.worker
            .submit(Request::Protected(Box::new(p::commands::Command {
                account: self.owner.account().into(),
                action: build(reply),
            })));
        self.run();
        receive.recv().unwrap()
    }
    fn seed(&self) -> p::Review {
        self.call(|reply| p::commands::Action::Initialize { reply })
            .unwrap();
        let scope = self.scope();
        let empty = self
            .call(|reply| p::commands::Action::ReviewLocal {
                scope: scope.clone(),
                reply,
            })
            .unwrap();
        let lease = self
            .call(|reply| p::commands::Action::RenewLocal {
                expected: empty,
                reply,
            })
            .unwrap();
        let p::LocalWriteResult::Applied(written) = self
            .call(|reply| p::commands::Action::WriteLocal {
                lease,
                predecessor_serial: 0,
                serial: 1,
                expected_local: LocalDraft::default(),
                local: local("initial"),
                reply,
            })
            .unwrap()
        else {
            panic!("seed")
        };
        self.call(|reply| p::commands::Action::EnrollLocal {
            scope,
            expected: written,
            reply,
        })
        .unwrap()
    }
    fn stream(&self) -> p::Stream {
        let state = self
            .call(|reply| p::commands::Action::StreamState {
                team: "T1".into(),
                reply,
            })
            .unwrap();
        self.call(|reply| p::commands::Action::OpenStream {
            team: "T1".into(),
            expected: state,
            reply,
        })
        .unwrap()
    }
}
fn same(actual: &LocalDraft, expected: &LocalDraft) {
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

#[test]
fn canonical_mixed_batch_reports_each_real_key_without_losing_blocked_input() {
    let f = Fixture::new();
    let scope = f.scope();
    sync::tests::enroll(&mut f.open(f.owner.account()).unwrap(), &scope);
    let mut other = scope.key.clone();
    other.channel = "C2".into();
    f.worker.enqueue(
        scope.account_id.clone(),
        [
            (scope.key.clone(), local("blocked")),
            (other.clone(), local("saved")),
        ]
        .into(),
    );
    f.run();
    let pending = f.worker.pending.lock().unwrap();
    let rows = pending
        .accounting
        .as_ref()
        .unwrap()
        .snapshot(&pending)
        .records;
    assert_eq!(rows[0].terminal.unwrap().execution, E::PreservedConflict);
    assert_eq!(rows[1].terminal.unwrap().execution, E::StorageAccepted);
    drop(pending);
    let loaded = f
        .open(f.owner.account())
        .unwrap()
        .load_synced(f.owner.account(), LoadMode::Current)
        .unwrap();
    same(&loaded.blocked_local[&scope.key].draft, &local("blocked"));
    same(
        &loaded
            .local
            .iter()
            .find(|(key, _)| key == &other)
            .unwrap()
            .1,
        &local("saved"),
    );
}

#[test]
fn failed_canonical_retry_remains_unresolved_until_actual_later_commit() {
    let f = Fixture::new();
    let scope = f.scope();
    f.worker.enqueue(
        scope.account_id.clone(),
        [(scope.key.clone(), local("retained"))].into(),
    );
    assert!(f.worker.cycle(|_| Err(sync::invalid("unavailable"))));
    let pending = f.worker.pending.lock().unwrap();
    let rows = pending
        .accounting
        .as_ref()
        .unwrap()
        .snapshot(&pending)
        .records;
    assert_eq!(rows[0].terminal.unwrap().execution, E::StorageUnresolved);
    assert_eq!(rows[1].terminal, None);
    same(
        &pending.updates[&(scope.account_id, scope.key)],
        &local("retained"),
    );
    drop(pending);
    f.run();
    assert_eq!(f.report().execution, E::StorageAccepted);
    same(
        &f.open(f.owner.account()).unwrap().load().unwrap()[0].1,
        &local("retained"),
    );
}

#[test]
fn stale_commit_is_not_accepted_even_though_the_reply_is_ok() {
    let f = Fixture::new();
    let scope = f.scope();
    let row = sync::tests::enroll(&mut f.open(f.owner.account()).unwrap(), &scope);
    let (reply, receive) = mpsc::sync_channel(1);
    f.worker.submit(Request::Commit(
        scope.clone(),
        Some(row.revision + 1),
        local("wrong"),
        row.state.clone(),
        None,
        reply,
    ));
    f.run();
    assert!(matches!(receive.recv().unwrap().unwrap(), Commit::Stale(_)));
    assert_eq!(f.report().execution, E::PreconditionRefused);
    let current = f
        .open(f.owner.account())
        .unwrap()
        .read_local_review(&scope)
        .unwrap();
    assert_eq!(current.stored.unwrap().revision, row.revision);
}

#[test]
fn conditional_false_does_not_claim_exact_input_preservation() {
    let f = Fixture::new();
    let scope = f.scope();
    f.open(f.owner.account())
        .unwrap()
        .save([(scope.key.clone(), local("existing"))])
        .unwrap();
    let (reply, receive) = mpsc::sync_channel(1);
    f.worker.submit(Request::ConditionalLocal(
        scope.clone(),
        local("different expected"),
        local("new"),
        reply,
    ));
    f.run();
    assert!(!receive.recv().unwrap().unwrap());
    assert_eq!(f.report().execution, E::PreconditionRefused);
    assert_eq!(f.report().inventory, I::NotAssessed);
    same(
        &f.open(f.owner.account())
            .unwrap()
            .read_local_review(&scope)
            .unwrap()
            .local,
        &local("existing"),
    );
}

#[test]
fn prepared_legacy_operation_is_durable_but_inventory_remains_pending() {
    let f = Fixture::new();
    let scope = f.scope();
    let row = sync::tests::enroll(&mut f.open(f.owner.account()).unwrap(), &scope);
    let (reply, receive) = mpsc::sync_channel(1);
    f.worker
        .submit(Request::Prepare(scope.clone(), row.revision, reply));
    f.run();
    assert!(matches!(
        receive.recv().unwrap().unwrap(),
        Prepare::Prepared { .. }
    ));
    assert_eq!(f.report().execution, E::StorageAccepted);
    assert_eq!(f.report().inventory, I::OperationPending);
    assert!(
        f.open(f.owner.account())
            .unwrap()
            .read_local_review(&scope)
            .unwrap()
            .stored
            .unwrap()
            .state
            .pending()
            .is_some()
    );
}

#[test]
fn upload_commit_survives_dropped_reply_and_existing_reservation_is_not_new() {
    let f = Fixture::new();
    let mut draft = super::super::uploads::tests::draft();
    draft.account_id = f.owner.account().into();
    let (reply, receive) = mpsc::sync_channel(1);
    drop(receive);
    f.worker.submit(Request::Upload(uploads::Command::Reserve(
        draft.clone(),
        reply,
    )));
    f.run();
    assert_eq!(f.report().execution, E::StorageAccepted);
    assert_eq!(
        f.open(f.owner.account())
            .unwrap()
            .load_uploads(f.owner.account())
            .unwrap()
            .len(),
        1
    );
    let (reply, receive) = mpsc::sync_channel(1);
    f.worker
        .submit(Request::Upload(uploads::Command::Reserve(draft, reply)));
    f.run();
    assert!(!receive.recv().unwrap().unwrap().created);
    assert_eq!(f.report().execution, E::ReadObserved);
}

#[test]
fn upload_late_finalize_none_is_not_reported_as_commit() {
    let f = Fixture::new();
    let mut draft = super::super::uploads::tests::draft();
    draft.account_id = f.owner.account().into();
    let row = f
        .open(f.owner.account())
        .unwrap()
        .reserve_upload(&draft)
        .unwrap()
        .stored;
    assert!(
        f.open(f.owner.account())
            .unwrap()
            .retire_upload(&row)
            .unwrap()
    );
    let (reply, receive) = mpsc::sync_channel(1);
    f.worker.submit(Request::Upload(uploads::Command::Finalize(
        row.ticket,
        vec!["F1".into()],
        reply,
    )));
    f.run();
    assert!(receive.recv().unwrap().unwrap().is_none());
    assert_eq!(f.report().execution, E::PreconditionRefused);
    assert!(
        f.open(f.owner.account())
            .unwrap()
            .load_uploads(f.owner.account())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn slash_actual_load_and_open_failure_report_without_sender_inference() {
    let f = Fixture::new();
    crate::slash_commands::journal::install(
        &mut f.open(f.owner.account()).unwrap().0,
        f.owner.account(),
    )
    .unwrap();
    let (reply, receive) = tokio::sync::oneshot::channel();
    f.worker.submit(Request::SlashJournal(Box::new(
        crate::slash_commands::journal::Command::Load {
            account: f.owner.account().into(),
            reply,
        },
    )));
    f.run();
    assert!(receive.blocking_recv().unwrap().unwrap().is_empty());
    assert_eq!(f.report().execution, E::ReadObserved);
    let (reply, receive) = tokio::sync::oneshot::channel();
    f.worker.submit(Request::SlashJournal(Box::new(
        crate::slash_commands::journal::Command::Load {
            account: f.owner.account().into(),
            reply,
        },
    )));
    f.worker.cycle(|_| Err(sync::invalid("offline storage")));
    assert!(matches!(
        receive.blocking_recv().unwrap(),
        Err(crate::slash_commands::Failure::Persistence)
    ));
    assert_eq!(f.report().execution, E::StorageUnresolved);
}

#[test]
fn protected_uncertain_receipt_acceptance_never_reports_settled_remote_inventory() {
    let f = Fixture::new();
    let before = f.seed();
    let stream = f.stream();
    let verified = f
        .call(|reply| p::commands::Action::VerifyLocal {
            stream: stream.clone(),
            expected: before,
            reply,
        })
        .unwrap();
    let ticket = f
        .call(|reply| p::commands::Action::Prepare {
            stream,
            expected: verified,
            reply,
        })
        .unwrap();
    let claimed = f
        .call(|reply| p::commands::Action::ClaimDispatch { ticket, reply })
        .unwrap();
    let (operation, receipt) = claimed.into_parts();
    let actual = f
        .call(|reply| p::commands::Action::Record {
            receipt,
            outcome: p::Outcome::Uncertain,
            known_until: 0,
            reply,
        })
        .unwrap();
    assert_eq!(actual.row().state().pending(), Some(&operation));
    assert!(matches!(
        actual.row().state().workflow(),
        crate::drafts::sync::Workflow::Uncertain(_)
    ));
    assert_eq!(f.report().execution, E::StorageAccepted);
    assert_eq!(f.report().inventory, I::OperationPending);
    assert!(!f.report().capability_effects_assessed);
}

#[test]
fn protected_writer_rollback_is_unresolved_and_explicit_same_serial_retry_commits() {
    let f = Fixture::new();
    let before = f.seed();
    let writer = f
        .call(|reply| p::commands::Action::RegisterWriter {
            expected: before.clone(),
            reply,
        })
        .unwrap();
    f.open(f.owner.account()).unwrap().0.execute_batch("CREATE TRIGGER fail_writer BEFORE UPDATE ON composer_draft_writer_lineage BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    let result = f.call(|reply| p::commands::Action::Checkpoint {
        writer: writer.clone(),
        serial: 1,
        expected: before.clone(),
        local: local("after"),
        direct: None,
        reply,
    });
    assert!(matches!(result, Err(p::Error::Storage)));
    assert_eq!(f.report().execution, E::StorageUnresolved);
    let scope = f.scope();
    let current = f
        .call(|reply| p::commands::Action::Review { scope, reply })
        .unwrap();
    assert!(current.same(&before));
    f.open(f.owner.account())
        .unwrap()
        .0
        .execute_batch("DROP TRIGGER fail_writer;")
        .unwrap();
    let p::WriteResult::Applied(after) = f
        .call(|reply| p::commands::Action::Checkpoint {
            writer,
            serial: 1,
            expected: before,
            local: local("after"),
            direct: None,
            reply,
        })
        .unwrap()
    else {
        panic!("retry")
    };
    same(after.local(), &local("after"));
    assert_eq!(f.report().execution, E::StorageAccepted);
}

#[test]
fn protected_retired_enrollment_preserves_exact_input_with_distinct_report() {
    let f = Fixture::new();
    let before = f.seed();
    let writer = f
        .call(|reply| p::commands::Action::RegisterWriter {
            expected: before.clone(),
            reply,
        })
        .unwrap();
    f.call(|reply| p::commands::Action::Release {
        expected: before.clone(),
        reply,
    })
    .unwrap();
    let scope = f.scope();
    let inventory = f
        .call(|reply| p::commands::Action::ReviewLocal {
            scope: scope.clone(),
            reply,
        })
        .unwrap();
    let newer = f
        .call(|reply| p::commands::Action::EnrollLocal {
            scope,
            expected: inventory,
            reply,
        })
        .unwrap();
    let p::WriteResult::Preserved(actual) = f
        .call(|reply| p::commands::Action::Checkpoint {
            writer,
            serial: 1,
            expected: before,
            local: local("old input"),
            direct: None,
            reply,
        })
        .unwrap()
    else {
        panic!("preserved")
    };
    assert_eq!(
        actual.row().unwrap().lineage().enrollment(),
        newer.row().lineage().enrollment()
    );
    same(&actual.alternates()[0].draft, &local("old input"));
    assert_eq!(f.report().execution, E::PreservedConflict);
    assert_eq!(f.report().inventory, I::ConflictPreserved);
}

#[test]
fn protected_open_failure_can_revoke_live_stream_without_claiming_no_effects() {
    let f = Fixture::new();
    f.seed();
    let old = f.stream();
    assert!(old.is_live());
    let expected = f
        .call(|reply| p::commands::Action::StreamState {
            team: "T1".into(),
            reply,
        })
        .unwrap();
    let (reply, receive) = mpsc::channel();
    f.worker
        .submit(Request::Protected(Box::new(p::commands::Command {
            account: f.owner.account().into(),
            action: p::commands::Action::OpenStream {
                team: "T1".into(),
                expected,
                reply,
            },
        })));
    f.worker
        .cycle(|_| Err(sync::invalid("failure after actual revoke")));
    assert!(matches!(receive.recv().unwrap(), Err(p::Error::Storage)));
    assert!(!old.is_live());
    assert_eq!(f.report().execution, E::StorageUnresolved);
    assert!(!f.report().capability_effects_assessed);
}

#[test]
fn idempotent_writer_report_describes_returned_history_not_current_inventory() {
    let f = Fixture::new();
    let before = f.seed();
    let writer = f
        .call(|reply| p::commands::Action::RegisterWriter {
            expected: before.clone(),
            reply,
        })
        .unwrap();
    let p::WriteResult::Applied(first) = f
        .call(|reply| p::commands::Action::Checkpoint {
            writer: writer.clone(),
            serial: 1,
            expected: before.clone(),
            local: local("first"),
            direct: None,
            reply,
        })
        .unwrap()
    else {
        panic!("first")
    };
    let other = f
        .call(|reply| p::commands::Action::RegisterWriter {
            expected: first.clone(),
            reply,
        })
        .unwrap();
    let p::WriteResult::Applied(second) = f
        .call(|reply| p::commands::Action::Checkpoint {
            writer: other,
            serial: 1,
            expected: first.clone(),
            local: local("second"),
            direct: None,
            reply,
        })
        .unwrap()
    else {
        panic!("second")
    };
    let p::WriteResult::Applied(retry) = f
        .call(|reply| p::commands::Action::Checkpoint {
            writer,
            serial: 1,
            expected: before,
            local: local("first"),
            direct: None,
            reply,
        })
        .unwrap()
    else {
        panic!("idempotent retry")
    };
    assert!(retry.same(&first));
    assert!(!retry.same(&second));
    assert_eq!(f.report().execution, E::StorageAccepted);
    assert_eq!(f.report().inventory, I::NotAssessed);
    let scope = f.scope();
    let current = f
        .call(|reply| p::commands::Action::Review { scope, reply })
        .unwrap();
    assert!(current.same(&second));
}

#[test]
fn refused_different_receipt_result_can_still_commit_independent_cooldown() {
    let f = Fixture::new();
    let before = f.seed();
    let stream = f.stream();
    let verified = f
        .call(|reply| p::commands::Action::VerifyLocal {
            stream: stream.clone(),
            expected: before,
            reply,
        })
        .unwrap();
    let ticket = f
        .call(|reply| p::commands::Action::Prepare {
            stream,
            expected: verified,
            reply,
        })
        .unwrap();
    let claimed = f
        .call(|reply| p::commands::Action::ClaimDispatch { ticket, reply })
        .unwrap();
    let (_, receipt) = claimed.into_parts();
    f.call(|reply| p::commands::Action::Record {
        receipt: receipt.clone(),
        outcome: p::Outcome::Uncertain,
        known_until: 0,
        reply,
    })
    .unwrap();
    assert!(matches!(
        f.call(|reply| p::commands::Action::Record {
            receipt,
            outcome: p::Outcome::Rejected,
            known_until: 90_000,
            reply
        }),
        Err(p::Error::Stale)
    ));
    assert_eq!(f.report().execution, E::PreconditionRefused);
    assert_eq!(f.report().inventory, I::NotAssessed);
    assert!(!f.report().capability_effects_assessed);
    let raw: String = f
        .open(f.owner.account())
        .unwrap()
        .0
        .query_row(
            "SELECT content FROM composer_draft_recovery_owners WHERE team='T1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["deadline"], 90_000);
}
