//! The normal worker stack must support mixed legacy/protected requests.
use super::super::protected::{
    self,
    commands::{Action, Command},
};
use super::super::sync::tests::{Database, local, scope};
use super::*;

fn command<T>(
    worker: &Worker,
    db: &Database,
    account: &str,
    action: impl FnOnce(mpsc::Sender<Result<T, protected::Error>>) -> Action,
) -> Result<T, protected::Error> {
    let (reply, result) = mpsc::channel();
    worker.submit(Request::Protected(Box::new(Command {
        account: account.to_owned(),
        action: action(reply),
    })));
    assert!(!worker.cycle(|owner| {
        assert_eq!(owner, account);
        Ok(db.open())
    }));
    result.recv().unwrap()
}

fn ordinary_load(worker: &Worker, db: &Database, account: &str) -> Entries {
    let (reply, result) = mpsc::sync_channel(1);
    worker.submit(Request::Load(account.into(), reply));
    assert!(!worker.cycle(|owner| {
        assert_eq!(owner, account);
        Ok(db.open())
    }));
    result.recv().unwrap().unwrap()
}

#[test]
fn normal_stack_mixed_worker_preserves_archived_writer_input() {
    // No stack override. The application Default route uses this same cycle;
    // explicit temporary opens keep this test away from keyring/default paths.
    std::thread::Builder::new()
        .name("mixed-draft-dispatch-regression".into())
        .spawn(|| {
            let db = Database::new();
            let scope = scope();
            let account = scope.account_id.as_str();
            let mut worker = Worker {
                protected_fixture: true,
                ..Worker::default()
            };
            command(&worker, &db, account, |reply| Action::Initialize { reply }).unwrap();
            worker.protected_fixture = false;
            assert!(ordinary_load(&worker, &db, account).is_empty());
            // A normal worker cannot use the fixture activation operation.
            assert!(matches!(
                command(&worker, &db, account, |reply| Action::Initialize { reply }),
                Err(protected::Error::Stale)
            ));
            let empty = command(&worker, &db, account, |reply| Action::ReviewLocal {
                scope: scope.clone(),
                reply,
            })
            .unwrap();
            let lease = command(&worker, &db, account, |reply| Action::RenewLocal {
                expected: empty,
                reply,
            })
            .unwrap();
            let protected::LocalWriteResult::Applied(written) =
                command(&worker, &db, account, |reply| Action::WriteLocal {
                    lease,
                    predecessor_serial: 0,
                    serial: 1,
                    expected_local: LocalDraft::default(),
                    local: local("same"),
                    reply,
                })
                .unwrap()
            else {
                panic!("seed must be canonical")
            };
            let before = command(&worker, &db, account, |reply| Action::EnrollLocal {
                scope: scope.clone(),
                expected: written,
                reply,
            })
            .unwrap();
            let writer = command(&worker, &db, account, |reply| Action::RegisterWriter {
                expected: before.clone(),
                reply,
            })
            .unwrap();
            command(&worker, &db, account, |reply| Action::Release {
                expected: before.clone(),
                reply,
            })
            .unwrap();
            let inventory = command(&worker, &db, account, |reply| Action::ReviewLocal {
                scope: scope.clone(),
                reply,
            })
            .unwrap();
            let newer = command(&worker, &db, account, |reply| Action::EnrollLocal {
                scope: scope.clone(),
                expected: inventory,
                reply,
            })
            .unwrap();
            assert!(!before.row().same_enrollment(newer.row()));
            let protected::WriteResult::Preserved(preserved) =
                command(&worker, &db, account, |reply| Action::Checkpoint {
                    writer: writer.clone(),
                    serial: 1,
                    expected: before,
                    local: local("old visit input"),
                    direct: None,
                    reply,
                })
                .unwrap()
            else {
                panic!("archived input must be preserved")
            };
            assert_eq!(preserved.local().composer.text, "same");
            assert_eq!(
                preserved.row().unwrap().lineage().enrollment(),
                newer.row().lineage().enrollment()
            );
            assert_eq!(
                preserved.origins()[0].origin(),
                &protected::WriterOrigin::Enrollment(writer.enrollment().into())
            );
            assert_eq!(
                preserved.alternates()[0].draft.composer.text,
                "old visit input"
            );
            assert_eq!(
                ordinary_load(&worker, &db, account)[0].1.composer.text,
                "same"
            );
            assert!(worker.pending.lock().unwrap().requests.is_empty());
        })
        .unwrap()
        .join()
        .unwrap();
}
