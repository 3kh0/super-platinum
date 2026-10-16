//! Actual failed settlement and the storage-only retry owner; never Transport.
use super::*;
use super_platinum_core::file_sharing::State as ReceiptState;

struct Failed {
    temp: Temp,
    shell: ShellState,
    id: String,
    receipt: Receipt,
    storage: recovery::Storage,
    key: admission::Key,
}
impl Failed {
    async fn new(result: Result<Value, Error>) -> Self {
        let temp = Temp::new();
        let (mut shell, target) = sample();
        let id = open(&mut shell, target, "source".into(), &temp)
            .await
            .unwrap();
        choose(&mut shell, &id, "C00000001").unwrap();
        let context = admission::Context::capture(&shell).unwrap();
        let dialog = visible(&shell).unwrap();
        let visit = dialog.draft_visit.clone().unwrap();
        let scope = Scope {
            team: context.key.team.clone(),
            user: context.key.user.clone(),
            file: dialog.file_id.clone(),
            destination: "C00000001".into(),
        };
        let intent = Intent::new(
            scope.file.clone(),
            scope.destination.clone(),
            Some(note()),
            None,
        )
        .unwrap();
        let directory = temp.0.join("journal");
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("receipt.sqlite");
        let receipt = match Journal::open(&path, ACCOUNT)
            .unwrap()
            .begin(scope, intent, "File".into(), "ship".into(), 10)
            .unwrap()
        {
            Begin::Started(r) => r,
            _ => panic!("new"),
        };
        let storage = recovery::Storage::at(context.key.clone(), path);
        let mut lease =
            delivery::Lease::reserve(shell.file_share.custody.clone(), &context.key, id.clone())
                .unwrap();
        lease.visit = Some(visit.clone());
        lease.admit(&receipt).unwrap();
        shell
            .file_share
            .custody
            .lock()
            .unwrap()
            .capture_note(&visit, note(), dialog.destination.clone())
            .unwrap();
        let retained = temp.0.join("original");
        std::fs::rename(&directory, &retained).unwrap();
        std::fs::write(&directory, b"owned test storage failure").unwrap();
        assert!(matches!(
            delivery::settle(&storage, &lease, &receipt, result).await,
            Err(Refusal::Storage)
        ));
        drop(lease);
        std::fs::remove_file(&directory).unwrap();
        std::fs::rename(&retained, &directory).unwrap();
        let dialog = shell.file_share.dialog.as_mut().unwrap();
        dialog.note = note();
        dialog.receipt = Some(receipt.clone());
        dialog.phase = Phase::StorageFailed;
        Self {
            temp,
            shell,
            id,
            receipt,
            storage,
            key: context.key,
        }
    }
    fn job(&self) -> retry::Job {
        retry::Job::capture(&self.shell, &self.id, &self.receipt.attempt_id).unwrap()
    }
    async fn read(&self) -> Receipt {
        let scope = self.receipt.scope.clone();
        self.storage
            .run(move |j| j.get(&scope))
            .await
            .unwrap()
            .unwrap()
    }
}

#[tokio::test]
async fn retry_saves_captured_raw_note_and_cleans_only_exact_accepted_draft() {
    for newer in [false, true] {
        let raw = json!({"ok":true,"file_msg_ts":"1719800400.000001","future":{"keep":"\u{65e5}\u{672c}\u{8a9e}"}});
        let mut f = Failed::new(Ok(raw.clone())).await;
        assert_eq!(retry_status(&f.shell, &f.id), Some(RetryState::Ready));
        let job = f.job();
        assert_eq!(retry_status(&f.shell, &f.id), Some(RetryState::Saving));
        assert!(matches!(
            retry::Job::capture(&f.shell, &f.id, &f.receipt.attempt_id),
            Err(Refusal::Busy)
        ));
        if newer {
            let source = f
                .shell
                .file_share
                .dialog
                .as_ref()
                .unwrap()
                .admission
                .clone()
                .unwrap();
            let destination = f
                .shell
                .file_share
                .dialog
                .as_ref()
                .unwrap()
                .destination
                .clone();
            let mut held = f.shell.file_share.custody.lock().unwrap();
            let next = held.visit(&source).unwrap();
            held.capture_note(&next, note(), destination).unwrap();
        }
        let saved = job.run().await.unwrap();
        assert_eq!(saved.attempt_id, f.receipt.attempt_id);
        assert_eq!(saved.raw_response(), Some(&raw));
        assert_eq!(saved.intent.note(), Some(note().as_slice()));
        drop(job);
        retry::publish(&mut f.shell, &f.key, &f.receipt, Ok(saved));
        assert_eq!(f.shell.file_share.dialog.as_ref().unwrap().id, f.id);
        assert_eq!(f.shell.file_share.dialog.as_ref().unwrap().note, note());
        assert_eq!(
            f.shell.file_share.custody.lock().unwrap().drafts.len(),
            usize::from(newer)
        );
        assert!(
            f.shell
                .file_share
                .custody
                .lock()
                .unwrap()
                .pending
                .is_empty()
        );
        assert!(retained_status(&f.shell, &f.receipt.attempt_id).is_none());
        assert!(retry_status(&f.shell, &f.id).is_none());
        assert!(retry::Job::capture(&f.shell, &f.id, &f.receipt.attempt_id).is_err());
        assert_eq!(f.read().await.raw_response(), Some(&raw));
    }
}

#[tokio::test]
async fn retry_canceled_owner_can_recover_an_identical_commit_without_new_admission() {
    let f = Failed::new(Ok(json!({"ok":true}))).await;
    let job = f.job();
    let (started, received) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _job = job;
        started.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    received.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(retry_status(&f.shell, &f.id), Some(RetryState::Ready));
    // Model the blocking transaction completing after its async owner was lost.
    // This is an actual journal commit, not a synthetic saved UI state.
    let expected = f.receipt.clone();
    f.storage
        .run(move |j| j.retry_result(&expected, Ok(json!({"ok":true})), 20))
        .await
        .unwrap()
        .unwrap();
    let again = f.job();
    let saved = again.run().await.unwrap();
    assert_eq!(saved.attempt_id, f.receipt.attempt_id);
    assert!(!saved.unresolved());
    assert!(
        f.shell
            .file_share
            .custody
            .lock()
            .unwrap()
            .pending
            .is_empty()
    );
}

#[tokio::test]
async fn retry_rejects_stale_callbacks_owner_and_absent_received_result() {
    let mut f = Failed::new(Ok(json!({"ok":true}))).await;
    assert!(retry::Job::capture(&f.shell, "old-dialog", &f.receipt.attempt_id).is_err());
    assert!(retry::Job::capture(&f.shell, &f.id, "old-attempt").is_err());
    let account = f.shell.core.active_account.replace("other".into());
    assert!(retry::Job::capture(&f.shell, &f.id, &f.receipt.attempt_id).is_err());
    f.shell.core.active_account = account;
    f.shell
        .file_share
        .custody
        .lock()
        .unwrap()
        .pending
        .get_mut(&f.id)
        .unwrap()
        .outcome = None;
    assert!(retry_status(&f.shell, &f.id).is_none());
    assert!(retry::Job::capture(&f.shell, &f.id, &f.receipt.attempt_id).is_err());
    assert_eq!(f.read().await.state(), ReceiptState::Unconfirmed);
}

#[tokio::test]
async fn retry_survives_account_replacement_and_only_updates_matching_readonly_receipts() {
    let mut f = Failed::new(Ok(json!({"ok":true}))).await;
    let job = f.job();
    let (mut replacement, target) = sample();
    replacement.core.active_account = Some("different-account".into());
    carry_pending(&mut f.shell, &mut replacement);
    drop(f.shell);
    let saved = job.run().await.unwrap();
    retry::publish(&mut replacement, &f.key, &f.receipt, Ok(saved.clone()));
    assert!(replacement.file_share.dialog.is_none());
    replacement.core.active_account = Some(ACCOUNT.into());
    let editing = open(&mut replacement, target, "new".into(), &f.temp)
        .await
        .unwrap();
    retry::publish(&mut replacement, &f.key, &f.receipt, Ok(saved.clone()));
    assert_eq!(visible(&replacement).unwrap().id, editing);
    assert_eq!(visible(&replacement).unwrap().phase, Phase::Editing);
    close(&mut replacement, &editing);
    let review =
        recovery::show_receipt(&mut replacement, f.receipt.clone(), "saved".into(), None).unwrap();
    retry::publish(&mut replacement, &f.key, &f.receipt, Ok(saved));
    assert_eq!(visible(&replacement).unwrap().id, review);
    assert!(
        !visible(&replacement)
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .unresolved()
    );
}

#[tokio::test]
async fn retry_conflicting_or_missing_record_preserves_retained_result_and_note() {
    for missing in [false, true] {
        let f = Failed::new(Ok(json!({"ok":true,"file_msg_ts":"1719800400.000001"}))).await;
        if missing {
            std::fs::remove_file(f.temp.0.join("journal/receipt.sqlite")).unwrap();
        } else {
            let expected = f.receipt.clone();
            f.storage
                .run(move |j| {
                    j.retry_result(
                        &expected,
                        Ok(json!({"ok":false,"error":"dlp_processing"})),
                        20,
                    )
                })
                .await
                .unwrap()
                .unwrap();
        }
        let job = f.job();
        assert!(job.run().await.is_err());
        drop(job);
        assert_eq!(retry_status(&f.shell, &f.id), Some(RetryState::Ready));
        assert!(matches!(
            retained_status(&f.shell, &f.receipt.attempt_id),
            Some(ReceiptState::Server(
                api::file_share::FileShareDisposition::Accepted { .. }
            ))
        ));
        assert_eq!(f.shell.file_share.custody.lock().unwrap().drafts.len(), 1);
        assert_eq!(f.shell.file_share.custody.lock().unwrap().pending.len(), 1);
    }
}

#[tokio::test]
async fn retry_preserves_original_rate_limit_deadline_and_unresolved_classifications() {
    let cases = [
        Err(Error::RateLimited {
            retry_after_secs: Some(120),
        }),
        Err(Error::Transport("synthetic lost response".into())),
        Ok(json!({"ok":false,"error":"dlp_user_warning","dlp_custom_message":"kept"})),
        Ok(json!({"ok":false,"error":"dlp_processing"})),
        Ok(json!({"large":"x".repeat(1024*1024)})),
    ];
    for result in cases {
        let f = Failed::new(result).await;
        let expected = retained_status(&f.shell, &f.receipt.attempt_id).unwrap();
        // A fixed original observation makes a retry-at-now bug deterministic.
        f.shell
            .file_share
            .custody
            .lock()
            .unwrap()
            .pending
            .get_mut(&f.id)
            .unwrap()
            .settlement = Some(retry::Settlement::new(f.storage.clone(), 20, None));
        let job = f.job();
        let saved = job.run().await.unwrap();
        assert_eq!(saved.state(), expected);
        assert!(saved.unresolved());
        assert_eq!(saved.intent.note(), Some(note().as_slice()));
        assert_eq!(f.shell.file_share.custody.lock().unwrap().drafts.len(), 1);
        if matches!(expected, ReceiptState::RateLimited { .. }) {
            let key = f.key.clone();
            assert_eq!(
                f.storage
                    .run(move |j| j.retry_at(&key.team, &key.user))
                    .await
                    .unwrap(),
                140
            );
            assert_eq!(
                f.shell
                    .file_share
                    .custody
                    .lock()
                    .unwrap()
                    .check(&f.key, recovery::now()),
                Err(Refusal::Cooldown)
            );
        }
    }
}

#[tokio::test]
async fn retry_refuses_closing_without_changing_retained_result_or_busy_owner() {
    for phase in [Phase::StorageFailed, Phase::Outcome] {
        let mut f = Failed::new(Ok(json!({"ok":true}))).await;
        f.shell.file_share.dialog.as_mut().unwrap().phase = phase;
        assert_eq!(retry_status(&f.shell, &f.id), Some(RetryState::Ready));
        let retained = retained_status(&f.shell, &f.receipt.attempt_id);
        f.shell.file_share.dialog.as_mut().unwrap().closing = true;
        assert_eq!(retry_status(&f.shell, &f.id), None);
        assert!(matches!(
            retry::Job::capture(&f.shell, &f.id, &f.receipt.attempt_id),
            Err(Refusal::Changed)
        ));
        assert_eq!(retained_status(&f.shell, &f.receipt.attempt_id), retained);
        assert!(f.read().await == f.receipt);
        assert_eq!(f.shell.file_share.custody.lock().unwrap().pending.len(), 1);
        // A failed Close can re-enable the same retained result. Refusal did not
        // acquire a retry owner or consume the outcome.
        f.shell.file_share.dialog.as_mut().unwrap().closing = false;
        assert_eq!(retry_status(&f.shell, &f.id), Some(RetryState::Ready));
        let admitted = f.job();
        // Closing after admission never cancels captured settlement custody.
        f.shell.file_share.dialog.as_mut().unwrap().closing = true;
        assert_eq!(retry_status(&f.shell, &f.id), None);
        assert!(matches!(
            retry::Job::capture(&f.shell, &f.id, &f.receipt.attempt_id),
            Err(Refusal::Changed)
        ));
        let saved = admitted.run().await.unwrap();
        assert_eq!(saved.attempt_id, f.receipt.attempt_id);
        assert_eq!(saved.raw_response(), Some(&json!({"ok":true})));
        assert!(
            f.shell
                .file_share
                .custody
                .lock()
                .unwrap()
                .pending
                .is_empty()
        );
    }
}
