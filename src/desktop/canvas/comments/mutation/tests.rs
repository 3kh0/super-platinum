use super::*;
use serde_json::json;
fn setup() -> (ShellState, super::super::State, Attempt) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("147b5480-9f1a-4443-89da-836472cbb209".into());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Canvas".into(),
        permalink: None,
    });
    shell.overlay = Some(Overlay::Canvas);
    let owner = Owner::capture(&shell).unwrap();
    let message:Message=serde_json::from_value(json!({"type":"message","ts":"1719800000.000100","subtype":"document_comment_root","reply_count":1,"document_comment":{"thread_id":"thread-1"}})).unwrap();
    let intent = ArchiveIntent::capture(&super::super::fixture::file(), &message).unwrap();
    let disk = Disk {
        account: shell.core.active_account.clone().unwrap(),
        scope: DiskScope {
            workspace: owner.scope.team.clone(),
            user: shell.self_account.user_id.clone(),
            file: owner.file.clone(),
        },
        path: None,
    };
    let model = super::super::State {
        owner: Some(owner.clone()),
        serial: 7,
        ..Default::default()
    };
    let attempt = Attempt {
        owner,
        serial: 1,
        read: 7,
        intent,
        disk,
    };
    shell
        .canvas_checklist
        .comment_operations
        .operations
        .push(Operation {
            attempt: attempt.clone(),
            stage: Stage::Preparing,
            receipt: None,
            accepted: None,
        });
    (shell, model, attempt)
}
#[test]
fn query_and_visit_retirement_prevent_dispatch() {
    let (mut shell, mut model, attempt) = setup();
    assert!(current(&shell, &model, &attempt));
    model.serial += 1;
    assert!(!current(&shell, &model, &attempt));
    model.serial -= 1;
    shell.shortcut_generation += 1;
    assert!(!current(&shell, &model, &attempt));
}
#[test]
fn sent_unknown_keeps_same_stable_owner_lock_after_rotation_and_reopen() {
    let (mut shell, _, attempt) = setup();
    assert!(stage(
        &mut shell,
        &attempt,
        Stage::Preparing,
        Stage::Sending
    ));
    assert!(stage(
        &mut shell,
        &attempt,
        Stage::Sending,
        Stage::Uncertain
    ));
    shell.core.account_epoch += 1;
    shell.shortcut_generation += 1;
    let owner = Owner::capture(&shell).unwrap();
    assert!(index(&shell, &owner).is_some());
    assert_eq!(retained_count(&shell), (1, 1));
    assert!(!stage(
        &mut shell,
        &attempt,
        Stage::Preparing,
        Stage::Sending
    ));
}
#[test]
fn another_account_cannot_borrow_or_remove_an_owned_attempt() {
    let (mut shell, _, attempt) = setup();
    shell.core.active_account = Some("21381bbf-dccd-4e51-98aa-73fc9b4d971e".into());
    let owner = Owner::capture(&shell).unwrap();
    assert!(index(&shell, &owner).is_none());
    let mut foreign = attempt.clone();
    foreign.disk.account = shell.core.active_account.clone().unwrap();
    retire(&mut shell, &foreign);
    assert_eq!(retained_count(&shell), (0, 1));
    retire(&mut shell, &attempt);
    assert_eq!(retained_count(&shell), (0, 0));
}
#[test]
fn observed_content_cannot_manufacture_an_ack_backed_stage() {
    let (mut shell, _, attempt) = setup();
    stage(&mut shell, &attempt, Stage::Preparing, Stage::Sending);
    stage(&mut shell, &attempt, Stage::Sending, Stage::Uncertain);
    let operation = &shell.canvas_checklist.comment_operations.operations[0];
    assert!(operation.accepted.is_none());
    assert!(!owns(&shell, &attempt, Stage::AwaitingRead));
}

#[test]
fn canonical_preflight_and_settlement_have_distinct_retirement_fences() {
    use run::ReadPurpose::{Preflight, Settlement};
    let (mut shell, mut model, attempt) = setup();
    assert!(Preflight.current(&shell, &model, &attempt));
    model.serial += 1;
    assert!(!Preflight.current(&shell, &model, &attempt));
    assert!(Settlement.current(&shell, &model, &attempt));
    model.serial -= 1;
    shell.shortcut_generation += 1;
    assert!(!Preflight.current(&shell, &model, &attempt));
    assert!(Settlement.current(&shell, &model, &attempt));
    shell.core.account_epoch += 1;
    assert!(!Settlement.current(&shell, &model, &attempt));
}

#[tokio::test]
async fn retirement_during_file_read_refuses_second_read_and_never_advances_write() {
    use std::cell::{Cell, RefCell};
    for retire_visit in [false, true] {
        let (shell, model, attempt) = setup();
        let shell = RefCell::new(shell);
        let model = RefCell::new(model);
        let reads = Cell::new(0);
        let result = run::guarded_reads(
            || run::ReadPurpose::Preflight.current(&shell.borrow(), &model.borrow(), &attempt),
            async {
                reads.set(reads.get() + 1);
                if retire_visit {
                    shell.borrow_mut().shortcut_generation += 1;
                } else {
                    model.borrow_mut().serial += 1;
                }
                Ok(())
            },
            || async {
                reads.set(reads.get() + 1);
                Ok(())
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(reads.get(), 1);
        assert!(owns(&shell.borrow(), &attempt, Stage::Preparing));
        assert!(
            shell
                .borrow()
                .canvas_checklist
                .comment_operations
                .operations[0]
                .receipt
                .is_none()
        );
    }
}

#[tokio::test]
async fn canonical_rate_limit_preserves_error_before_and_after_second_read() {
    use std::cell::Cell;
    for first_limited in [true, false] {
        let calls = Cell::new(0);
        let limited = || Error::RateLimited {
            retry_after_secs: Some(19),
        };
        let result = run::guarded_reads(
            || true,
            async {
                calls.set(1);
                if first_limited {
                    Err(limited())
                } else {
                    Ok(())
                }
            },
            || async {
                calls.set(2);
                Err::<(), _>(limited())
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(Error::RateLimited {
                retry_after_secs: Some(19)
            })
        ));
        assert_eq!(calls.get(), if first_limited { 1 } else { 2 });
    }
}

#[tokio::test]
async fn retired_before_canonical_read_makes_no_request() {
    use std::cell::Cell;
    let calls = Cell::new(0);
    let result = run::guarded_reads(
        || false,
        async {
            calls.set(1);
            Ok(())
        },
        || async {
            calls.set(2);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(calls.get(), 0);
}

#[test]
fn canonical_rate_fence_survives_retired_visit_without_dispatch_or_foreign_owner() {
    let (mut shell, mut model, mut attempt) = setup();
    // Unique synthetic account prevents coupling with other process-local rate tests.
    shell.core.active_account = Some(uuid::Uuid::new_v4().to_string());
    attempt.owner = Owner::capture(&shell).unwrap();
    model.owner = Some(attempt.owner.clone());
    let error = Error::RateLimited {
        retry_after_secs: Some(19),
    };
    let wait = run::remember_rate_error(&shell, &attempt, &error).unwrap();
    let super::super::rate::Wait::Until(deadline) = wait else {
        panic!("finite wait")
    };
    model.retry_at = Some(deadline);
    assert!(model.cooling());
    assert!(model.query.is_none());
    assert!(owns(&shell, &attempt, Stage::Preparing));
    shell.shortcut_generation += 1;
    assert!(!attempt.owner.current(&shell));
    assert!(run::remember_rate_error(&shell, &attempt, &error).is_some());
    assert!(super::super::rate::current(&shell).is_some());
    shell.core.active_account = Some(uuid::Uuid::new_v4().to_string());
    assert!(run::remember_rate_error(&shell, &attempt, &error).is_none());
    assert!(super::super::rate::current(&shell).is_none());
    assert!(run::remember_rate_error(&shell, &attempt, &Error::Api("other".into())).is_none());
    // Expiry is eligibility only, never a query or write transition.
    model.retry_at = Some(Instant::now() - std::time::Duration::from_secs(1));
    assert!(!model.cooling());
    assert!(model.query.is_none());
    assert!(owns(&shell, &attempt, Stage::Preparing));
}

#[test]
fn shell_replacement_retains_actual_accepted_proof_without_foreign_dispatch() {
    let (mut previous, model, attempt) = setup();
    let response = json!({"ok":true,"message":{"type":"message","ts":"1719800000.000100",
        "subtype":"document_comment_root","reply_count":1,
        "document_comment":{"thread_id":"thread-1","is_archived":true}}});
    let accepted = std::sync::Arc::new(attempt.intent.clone().accept(&response).unwrap());
    {
        let root = &mut previous.canvas_checklist.comment_operations;
        root.serial = 19;
        root.operations[0].stage = Stage::AwaitingRead;
        root.operations[0].accepted = Some(accepted.clone());
    }
    let mut loaded = ShellState::fixture(crate::media::MediaRegistry::default());
    loaded.core.active_account = Some(uuid::Uuid::new_v4().to_string());
    super::super::carry_pending(&mut previous, &mut loaded);
    assert!(
        previous
            .canvas_checklist
            .comment_operations
            .operations
            .is_empty()
    );
    assert_eq!(loaded.canvas_checklist.comment_operations.serial, 19);
    assert!(owns(&loaded, &attempt, Stage::AwaitingRead));
    assert!(!current(&loaded, &model, &attempt));
    assert!(attempt.owner.scope.request(&loaded).is_none());
    assert!(!stage(
        &mut loaded,
        &attempt,
        Stage::Preparing,
        Stage::Sending
    ));
    let retained = loaded.canvas_checklist.comment_operations.operations[0]
        .accepted
        .as_ref()
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(retained, &accepted));
    assert_eq!(retained.root().ts.as_deref(), Some("1719800000.000100"));
    assert_eq!(retained_count(&loaded), (0, 1));
    let mut foreign = attempt.clone();
    foreign.disk.account = loaded.core.active_account.clone().unwrap();
    retire(&mut loaded, &foreign);
    assert!(owns(&loaded, &attempt, Stage::AwaitingRead));
    // Original in-memory identity may retire only its own retained diagnostic.
    retire(&mut loaded, &attempt);
    assert_eq!(retained_count(&loaded), (0, 0));
    assert_eq!(loaded.canvas_checklist.comment_operations.serial, 19);
}
