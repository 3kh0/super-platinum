//! Actual isolated worker + Account/Host paths. No secret backend/default store.
use super::super::account::wait;
use super::*;
use std::time::Duration;

fn id(index: usize) -> String {
    format!("00000000-0000-4000-8000-{:012}", 800 + index)
}
fn shell(id: &str) -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some(id.into());
    shell.core.account_epoch = 7;
    shell
}
async fn ready(acquisition: &Acquisition) {
    tokio::time::timeout(Duration::from_secs(10), acquisition.ready())
        .await
        .unwrap();
}
async fn loaded(load: &Load) {
    tokio::time::timeout(Duration::from_secs(10), load.ready())
        .await
        .unwrap();
}

#[tokio::test]
async fn concurrent_acquire_canceled_waiter_uses_one_actual_initializer_and_arc() {
    let source = Source::fresh(id(1), true);
    let mut root = Root::new();
    let first = root.acquire(source.clone()).unwrap();
    let canceled = first.clone();
    let task = tokio::spawn(async move {
        canceled.ready().await;
    });
    tokio::task::yield_now().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let second = root.acquire(source.clone()).unwrap();
    assert!(matches!(second.account(), Err(Error::Pending)));
    assert!(Arc::ptr_eq(&first.0, &second.0));
    source.release();
    ready(&second).await;
    assert!(Arc::ptr_eq(
        &first.account().unwrap(),
        &second.account().unwrap()
    ));
    assert_eq!(source.starts(), 1);
}

#[tokio::test]
async fn failed_initializer_is_retained_after_disk_repair_no_automatic_replay() {
    let source = Source::fresh(id(2), true);
    let marker = std::fs::read(source.marker()).unwrap();
    let mut root = Root::new();
    let first = root.acquire(source.clone()).unwrap();
    std::fs::write(source.marker(), b"wrong owned marker").unwrap();
    source.release();
    ready(&first).await;
    assert!(matches!(
        first.account(),
        Err(Error::Storage(durable::Error::Storage))
    ));
    std::fs::write(source.marker(), marker).unwrap();
    let again = root.acquire(source.clone()).unwrap();
    ready(&again).await;
    assert!(Arc::ptr_eq(&first.0, &again.0));
    assert!(matches!(
        again.account(),
        Err(Error::Storage(durable::Error::Storage))
    ));
    assert_eq!(source.starts(), 1);
    let shell = shell(source.id());
    let selection = root.select(&shell, &again).unwrap();
    assert!(matches!(
        root.begin_load(&shell, &selection),
        Err(Error::Storage(durable::Error::Storage))
    ));
}

#[tokio::test]
async fn sixteen_accounts_are_retained_and_seventeenth_does_not_evict() {
    let mut root = Root::new();
    let mut entries = Vec::new();
    for n in 0..MAX_ACCOUNTS {
        let source = Source::fresh(id(20 + n), true);
        let acquired = root.acquire(source.clone()).unwrap();
        entries.push((source, acquired));
    }
    let overflow = Source::fresh(id(99), false);
    assert!(matches!(
        root.acquire(overflow.clone()),
        Err(Error::Capacity)
    ));
    assert_eq!(overflow.starts(), 0);
    assert_eq!(root.accounts.len(), MAX_ACCOUNTS);
    for (source, original) in entries {
        let revisited = root.acquire(source.clone()).unwrap();
        assert!(Arc::ptr_eq(&original.0, &revisited.0));
        source.release();
        ready(&revisited).await;
        assert_eq!(source.starts(), 1);
    }
}

#[tokio::test]
async fn same_uuid_different_worker_source_is_not_an_alias() {
    let source = Source::fresh(id(3), false);
    let other = Source::fresh(id(3), false);
    let mut root = Root::new();
    let first = root.acquire(source).unwrap();
    assert!(matches!(root.acquire(other.clone()), Err(Error::Identity)));
    assert_eq!(other.starts(), 0);
    ready(&first).await;
    assert_eq!(root.accounts.len(), 1);
}

#[tokio::test]
async fn source_cannot_be_initialized_by_two_roots_even_after_first_root_drop() {
    let source = Source::fresh(id(4), true);
    let mut first = Root::new();
    let acquisition = first.acquire(source.clone()).unwrap();
    let mut second = Root::new();
    assert!(matches!(
        second.acquire(source.clone()),
        Err(Error::Identity)
    ));
    drop(first);
    assert!(matches!(
        second.acquire(source.clone()),
        Err(Error::Identity)
    ));
    source.release();
    ready(&acquisition).await;
    assert!(acquisition.account().is_ok());
    assert_eq!(source.starts(), 1);
}

#[tokio::test]
async fn revisit_uses_current_and_does_not_revoke_live_stream() {
    let source = Source::fresh(id(5), false);
    let mut root = Root::new();
    let acquisition = root.acquire(source.clone()).unwrap();
    ready(&acquisition).await;
    let account = acquisition.account().unwrap();
    let client = account.client().unwrap();
    // A fresh protected store has no continuity team until a real enrollment.
    assert!(matches!(
        wait(client.stream_state("T1".into())).await,
        Err(durable::Error::Stale)
    ));
    let _owner = Box::pin(enrolled(&account)).await;
    let state = wait(client.stream_state("T1".into())).await.unwrap();
    let stream = wait(client.open_stream("T1".into(), state)).await.unwrap();
    assert!(stream.is_live());
    let mut shell = shell(source.id());
    for _ in 0..3 {
        let selected = root.select(&shell, &acquisition).unwrap();
        let load = root.begin_load(&shell, &selected).unwrap();
        loaded(&load).await;
        root.finish_load(&shell, &load).unwrap();
        root.retire_selection();
        shell.draft_writes.view_id += 1;
    }
    assert!(
        stream.is_live(),
        "a second AfterRestart would revoke this actual capability"
    );
    assert_eq!(source.starts(), 1);
    assert!(Arc::ptr_eq(
        &account,
        &root.acquire(source).unwrap().account().unwrap()
    ));
}

#[tokio::test]
async fn account_epoch_and_view_changes_reject_old_selection_and_old_load() {
    let source = Source::fresh(id(6), false);
    let mut root = Root::new();
    let acquired = root.acquire(source.clone()).unwrap();
    ready(&acquired).await;
    for field in 0..3 {
        let mut shell = shell(source.id());
        let selected = root.select(&shell, &acquired).unwrap();
        let load = root.begin_load(&shell, &selected).unwrap();
        match field {
            0 => shell.core.active_account = Some(id(70)),
            1 => shell.core.account_epoch += 1,
            _ => shell.draft_writes.view_id += 1,
        }
        loaded(&load).await;
        assert!(matches!(root.finish_load(&shell, &load), Err(Error::Stale)));
        assert!(matches!(
            root.begin_load(&shell, &selected),
            Err(Error::Stale)
        ));
    }
}

#[tokio::test]
async fn retirement_then_identical_shell_tuple_does_not_revive_old_ticket() {
    let source = Source::fresh(id(7), false);
    let mut root = Root::new();
    let acquired = root.acquire(source.clone()).unwrap();
    ready(&acquired).await;
    let shell = shell(source.id());
    let old = root.select(&shell, &acquired).unwrap();
    root.retire_selection();
    let next = root.select(&shell, &acquired).unwrap();
    assert!(!Arc::ptr_eq(&old.0, &next.0));
    assert!(matches!(root.begin_load(&shell, &old), Err(Error::Stale)));
    let load = root.begin_load(&shell, &next).unwrap();
    loaded(&load).await;
    root.finish_load(&shell, &load).unwrap();
}

#[tokio::test]
async fn late_initialization_can_fill_pool_but_cannot_reselect_old_account() {
    let source = Source::fresh(id(8), true);
    let other = Source::fresh(id(9), false);
    let mut root = Root::new();
    let first = root.acquire(source.clone()).unwrap();
    let second = root.acquire(other.clone()).unwrap();
    let old_shell = shell(source.id());
    let old = root.select(&old_shell, &first).unwrap();
    let new_shell = shell(other.id());
    let next = root.select(&new_shell, &second).unwrap();
    source.release();
    ready(&first).await;
    ready(&second).await;
    assert!(matches!(
        root.begin_load(&old_shell, &old),
        Err(Error::Stale)
    ));
    let load = root.begin_load(&new_shell, &next).unwrap();
    loaded(&load).await;
    root.finish_load(&new_shell, &load).unwrap();
}

#[tokio::test]
async fn canceled_discovery_waiter_reattaches_actual_job_without_replay() {
    let source = Source::fresh(id(10), false);
    let mut root = Root::new();
    let acquired = root.acquire(source.clone()).unwrap();
    ready(&acquired).await;
    let shell = shell(source.id());
    let selected = root.select(&shell, &acquired).unwrap();
    drop(root.begin_load(&shell, &selected).unwrap());
    let recovered = root.pending_load(&shell, &selected).unwrap();
    loaded(&recovered).await;
    root.finish_load(&shell, &recovered).unwrap();
    assert!(matches!(
        root.finish_load(&shell, &recovered),
        Err(Error::Storage(durable::Error::Stale))
    ));
    assert_eq!(source.starts(), 1);
}

#[tokio::test]
async fn protected_load_failure_cannot_become_empty_success_or_legacy_route() {
    let source = Source::fresh(id(11), false);
    let mut root = Root::new();
    let acquired = root.acquire(source.clone()).unwrap();
    ready(&acquired).await;
    let mut shell = shell(source.id());
    shell.core.composer =
        super_platinum_core::ComposerState::with_text("Retain \u{65e5}\u{672c} \u{1f680}");
    shell.core.thread_composer =
        super_platinum_core::ComposerState::with_text("Retain thread e\u{301}");
    let before = serde_json::to_value((&shell.core.composer, &shell.core.thread_composer)).unwrap();
    let selected = root.select(&shell, &acquired).unwrap();
    let marker = std::fs::read(source.marker()).unwrap();
    std::fs::write(source.marker(), b"wrong marker").unwrap();
    let failed = root.begin_load(&shell, &selected).unwrap();
    loaded(&failed).await;
    assert!(matches!(
        root.finish_load(&shell, &failed),
        Err(Error::Storage(durable::Error::Storage))
    ));
    assert_eq!(
        serde_json::to_value((&shell.core.composer, &shell.core.thread_composer)).unwrap(),
        before
    );
    assert_eq!(
        root.host.discovery().status(&shell),
        discovery::Status::Unavailable(durable::Error::Storage)
    );
    std::fs::write(source.marker(), marker).unwrap();
    let retry = root.begin_load(&shell, &selected).unwrap();
    loaded(&retry).await;
    root.finish_load(&shell, &retry).unwrap();
    assert_eq!(source.starts(), 1, "only Current inventory may retry");
}

#[tokio::test]
async fn stopping_admission_keeps_pending_outcomes_and_rejects_new_work() {
    let source = Source::fresh(id(12), true);
    let mut root = Root::new();
    let acquired = root.acquire(source.clone()).unwrap();
    let shell = shell(source.id());
    let selected = root.select(&shell, &acquired).unwrap();
    let retained = root.stop_admission();
    assert_eq!(retained.len(), 1);
    assert!(Arc::ptr_eq(&retained[0].0, &acquired.0));
    assert!(matches!(root.acquire(source.clone()), Err(Error::Closed)));
    assert!(matches!(
        root.begin_load(&shell, &selected),
        Err(Error::Closed)
    ));
    source.release();
    ready(&retained[0]).await;
    assert!(retained[0].account().is_ok());
    // This is initialization completion, explicitly NOT a graceful-close proof.
}

#[tokio::test]
async fn unfinished_job_drop_wakes_waiters_with_terminal_failure() {
    let job = Job::<()>::new();
    let completion = job.completion();
    let wait = job.wait();
    drop(completion);
    tokio::time::timeout(Duration::from_secs(1), wait.ready())
        .await
        .unwrap();
    assert_eq!(
        job.result().unwrap_err(),
        Error::Storage(durable::Error::Storage)
    );
}

fn local(text: &str) -> LocalDraft {
    LocalDraft {
        composer: super_platinum_core::ComposerState::with_text(text),
        attachments: vec![],
    }
}
async fn enrolled(account: &Arc<Account>) -> Owner {
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
        _ => panic!("expected actual canonical write"),
    };
    wait(
        account
            .client()
            .unwrap()
            .enroll_local(owner.scope().clone(), inventory),
    )
    .await
    .unwrap();
    owner
}

#[tokio::test]
async fn failed_editor_keeps_exact_input_and_identity_across_root_selection_retirement() {
    let source = Source::fresh(id(13), false);
    let mut root = Root::new();
    let acquired = root.acquire(source.clone()).unwrap();
    ready(&acquired).await;
    let account = acquired.account().unwrap();
    let owner = Box::pin(enrolled(&account)).await;
    let editor = Box::pin(editor::Editor::open(owner.clone())).await.unwrap();
    let newer = local("Retained \u{65e5}\u{672c} \u{1f680} e\u{301}");
    wait(source.fail_next(durable::test_support::FaultPoint::WriterCommit))
        .await
        .unwrap();
    editor.submit(&newer).unwrap();
    assert!(editor.flush().await.is_err());
    let before = editor.snapshot();
    assert!(same_local(before.pending.as_ref().unwrap(), &newer));
    let shell = shell(source.id());
    let selected = root.select(&shell, &acquired).unwrap();
    let load = root.begin_load(&shell, &selected).unwrap();
    loaded(&load).await;
    root.finish_load(&shell, &load).unwrap();
    root.retire_selection();
    let mut replacement = shell;
    replacement.draft_writes.view_id += 1;
    let next = root.select(&replacement, &acquired).unwrap();
    let load = root.begin_load(&replacement, &next).unwrap();
    loaded(&load).await;
    root.finish_load(&replacement, &load).unwrap();
    let retained = root.host.discovery().retained(&replacement).unwrap();
    assert_eq!(retained.len(), 1);
    assert!(Arc::ptr_eq(&retained[0], &editor));
    let after = retained[0].snapshot();
    assert_eq!(before.serial, after.serial);
    assert_eq!(before.writer_id, after.writer_id);
    assert!(same_local(after.pending.as_ref().unwrap(), &newer));
    assert_eq!(source.starts(), 1);
}

#[tokio::test]
async fn acquisition_from_another_root_cannot_select_even_with_same_uuid() {
    let a = Source::fresh(id(14), false);
    let b = Source::fresh(id(14), false);
    let mut left = Root::new();
    let mut right = Root::new();
    let first = left.acquire(a.clone()).unwrap();
    let second = right.acquire(b).unwrap();
    ready(&first).await;
    ready(&second).await;
    let shell = shell(a.id());
    let prior = left.select(&shell, &first).unwrap();
    assert!(matches!(left.select(&shell, &second), Err(Error::Identity)));
    assert!(matches!(left.begin_load(&shell, &prior), Err(Error::Stale)));
}

#[test]
fn missing_runtime_refuses_before_root_binding_or_entry_admission() {
    let source = Source::fresh(id(15), false);
    let mut root = Root::new();
    assert!(matches!(
        root.acquire(source.clone()),
        Err(Error::Storage(durable::Error::Storage))
    ));
    assert!(root.accounts.is_empty());
    assert_eq!(source.starts(), 0);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let next = root.acquire(source.clone()).unwrap();
        ready(&next).await;
        assert!(next.account().is_ok());
    });
    assert_eq!(source.starts(), 1);
}
