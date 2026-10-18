//! Actual isolated Editor + Owned worker. No fake native proof and no HTTP.
use super::*;
use serde_json::json;
fn done<I: Send + Sync + 'static, T: Clone + Send + 'static>(job: o::StorageJob<I, T>) -> T {
    job.submit().unwrap();
    assert_eq!(job.wait().unwrap(), o::JobState::Ready);
    job.result().unwrap().unwrap()
}
fn raw(id: &str) -> Value {
    json!({"id":id,"mode":"hosted","is_external":false,"filetype":"pdf","title":id,"future":{"keep":42}})
}
struct Fixture {
    account: Arc<Account>,
    editor: Arc<editor::Editor>,
    stream: durable::Stream,
}
impl Fixture {
    async fn new(text: &str) -> Self {
        let (initialized, scope, stream) = tokio::task::spawn_blocking(|| {
            let route = o::OwnedRoute::fresh_fixture().unwrap();
            let before = route.inspect().unwrap().wait().unwrap();
            route.commit(&before).unwrap().wait().unwrap();
            let review = route.inspect().unwrap().wait().unwrap();
            route.initialize_slash(&review).unwrap().wait().unwrap();
            let review = route.inspect().unwrap().wait().unwrap();
            route.initialize_hosted(&review).unwrap().wait().unwrap();
            let initialized = route
                .inspect()
                .unwrap()
                .wait()
                .unwrap()
                .open()
                .unwrap()
                .initialize_cold(&o::FactoryIdentity::default())
                .unwrap()
                .wait()
                .unwrap();
            let ops = initialized.operations();
            let scope = Scope {
                account_id: ops.account().into(),
                key: DraftKey {
                    team: "T00000001".into(),
                    channel: "C00000001".into(),
                    thread: None,
                },
            };
            let local = done(ops.local_inventory(scope.clone()).unwrap());
            let review = done(ops.enroll(local).unwrap());
            let expected = done(
                ops.stream_state(o::TeamInput {
                    scope: scope.clone(),
                })
                .unwrap(),
            );
            let stream = done(
                ops.open_stream(o::OpenInput {
                    scope: scope.clone(),
                    expected,
                })
                .unwrap(),
            );
            done(
                ops.verify_local(o::MutationInput {
                    stream: stream.clone(),
                    expected: review,
                })
                .unwrap(),
            );
            (initialized, scope, stream)
        })
        .await
        .unwrap();
        let account = Account::from_retained(initialized);
        let editor = editor::Editor::open(account.owner(scope.key).unwrap())
            .await
            .unwrap();
        if !text.is_empty() {
            editor
                .submit(&LocalDraft {
                    composer: super_platinum_core::ComposerState::with_text(text),
                    ..Default::default()
                })
                .unwrap();
            editor.flush().await.unwrap();
        }
        Self {
            account,
            editor,
            stream,
        }
    }
    fn ops(&self) -> o::Operations {
        self.account.owned.as_ref().unwrap().operations.clone()
    }
    async fn insert(&self) -> Snapshot {
        let c = self.editor.hosted();
        let s = c.load(&self.editor).await.unwrap();
        let version = c.insert(s.version, raw("FSECOND")).unwrap();
        c.insert(version, raw("FFIRST")).unwrap();
        c.persist(&self.editor, self.stream.clone()).await.unwrap()
    }
}
#[tokio::test]
async fn exact_editor_set_survives_save_reorder_and_rejects_old_display_version() {
    let f = Box::pin(Fixture::new("Original caption 日本語")).await;
    let c = f.editor.hosted();
    let saved = Box::pin(f.insert()).await;
    assert!(saved.saved);
    assert_eq!(saved.selection.ids(), vec!["FSECOND", "FFIRST"]);
    let before = f.account.ownership_inventory().unwrap();
    let version = c
        .reorder(saved.version, &["FFIRST".into(), "FSECOND".into()])
        .unwrap();
    assert_eq!(before.validate(&f.account), Err(ownership::Error::Changed));
    assert!(c.remove(saved.version, "FFIRST").is_err());
    let current = c.persist(&f.editor, f.stream.clone()).await.unwrap();
    assert_eq!(current.version, version);
    assert!(current.saved);
    let loaded = done(f.ops().inventory().unwrap());
    let row = &loaded.hosted().drafts()[0];
    assert_eq!(row.input().snapshot().file_ids, vec!["FFIRST", "FSECOND"]);
    assert_eq!(row.input().sources(), current.selection.sources());
    assert_eq!(
        f.editor.snapshot().view.local().composer.text,
        "Original caption 日本語"
    );
}
#[tokio::test]
async fn file_only_set_gets_real_writer_serial_and_exact_empty_caption() {
    let f = Box::pin(Fixture::new("")).await;
    assert_eq!(f.editor.snapshot().serial, 0);
    let saved = Box::pin(f.insert()).await;
    assert!(saved.saved);
    assert!(f.editor.snapshot().serial > 0);
    let loaded = done(f.ops().inventory().unwrap());
    let input = loaded.hosted().drafts()[0].input();
    assert!(input.snapshot().blocks.is_empty());
    assert_eq!(input.snapshot().file_ids, vec!["FSECOND", "FFIRST"]);
    assert!(f.editor.snapshot().view.local().empty());
}
#[tokio::test]
async fn failed_editor_save_retains_exact_set_and_retries_original_owned_job() {
    let f = Box::pin(Fixture::new("Frozen caption")).await;
    let c = f.editor.hosted();
    let empty = c.load(&f.editor).await.unwrap();
    c.insert(empty.version, raw("FFIRST")).unwrap();
    done(
        f.ops()
            .fixture_fault(
                f.editor.scope().clone(),
                durable::test_support::FaultPoint::MutationReceipt,
            )
            .unwrap(),
    );
    assert!(matches!(
        c.persist(&f.editor, f.stream.clone()).await,
        Err(durable::Error::Storage)
    ));
    let original = c.state.lock().unwrap().save.as_ref().unwrap().1.clone();
    let key = original.job().unwrap().key();
    let input = original.job().unwrap().input();
    assert!(c.snapshot().unwrap().retry_saved);
    assert!(c.remove(c.snapshot().unwrap().version, "FFIRST").is_err());
    let saved = c.retry_saved().await.unwrap();
    assert!(saved.saved);
    assert!(!saved.retry_saved);
    assert_eq!(saved.selection.ids(), vec!["FFIRST"]);
    assert!(Arc::ptr_eq(&input, &original.job().unwrap().input()));
    assert!(f.ops().resume_hosted_draft_save(&key).is_err());
}
#[tokio::test]
async fn durable_claim_blocks_set_edits_and_new_actor_input_invalidates_original_dispatch() {
    let f = Box::pin(Fixture::new("Original")).await;
    let saved = Box::pin(f.insert()).await;
    let c = f.editor.hosted();
    let prepared = c
        .begin_send(&f.editor, f.stream.clone(), saved.version)
        .unwrap();
    let completion = c.claim_send().await.unwrap();
    assert!(completion.returned().is_none());
    assert!(c.send_current(&f.editor, &prepared));
    assert!(c.remove(saved.version, "FFIRST").is_err());
    f.editor
        .submit(&LocalDraft {
            composer: super_platinum_core::ComposerState::with_text("Newer"),
            ..Default::default()
        })
        .unwrap();
    assert!(!c.send_current(&f.editor, &prepared));
    assert!(c.clear_basis(&f.editor).is_err());
    let loaded = done(f.ops().inventory().unwrap());
    assert_eq!(loaded.hosted().claims().len(), 1);
    assert_eq!(
        loaded.hosted().claims()[0].input().sources(),
        saved.selection.sources()
    );
    assert!(completion.returned().is_none());
}

#[tokio::test]
async fn inventory_storage_retry_keeps_original_job_and_retires_only_observed_attempts() {
    let f = Box::pin(Fixture::new("Caption stays")).await;
    let c = f.editor.hosted();
    done(
        f.ops()
            .fixture_fault(
                f.editor.scope().clone(),
                durable::test_support::FaultPoint::InventoryRead,
            )
            .unwrap(),
    );
    assert!(matches!(
        c.load(&f.editor).await,
        Err(durable::Error::Storage)
    ));
    let original = c.state.lock().unwrap().load.clone().unwrap();
    let key = original.job().unwrap().key();
    let input = original.job().unwrap().input();
    assert!(c.snapshot().unwrap().retry_load);
    assert!(!c.snapshot().unwrap().loaded);
    assert!(c.insert(0, raw("FFIRST")).is_err());
    // Repeating ordinary load keeps the failed original; it does not issue a new read.
    assert!(matches!(
        c.load(&f.editor).await,
        Err(durable::Error::Storage)
    ));
    assert!(Arc::ptr_eq(
        &original,
        c.state.lock().unwrap().load.as_ref().unwrap()
    ));
    let loaded = c.retry_load(&f.editor).await.unwrap();
    assert!(loaded.loaded && loaded.saved && !loaded.retry_load);
    assert!(Arc::ptr_eq(&input, &original.job().unwrap().input()));
    assert!(f.ops().resume_inventory(&key).is_err());
    assert!(original.job().unwrap().retry_inventory_storage().is_err());
    assert!(c.retry_load(&f.editor).await.is_err());
    assert_eq!(
        f.editor.snapshot().view.local().composer.text,
        "Caption stays"
    );
}
#[tokio::test]
async fn inventory_retry_rejects_another_editor_and_preserves_repeated_storage_failures() {
    let f = Box::pin(Fixture::new("Original")).await;
    let other = Box::pin(Fixture::new("Other account")).await;
    let c = f.editor.hosted();
    done(
        f.ops()
            .fixture_fault(
                f.editor.scope().clone(),
                durable::test_support::FaultPoint::InventoryRead,
            )
            .unwrap(),
    );
    assert!(matches!(
        c.load(&f.editor).await,
        Err(durable::Error::Storage)
    ));
    let original = c.state.lock().unwrap().load.clone().unwrap();
    assert!(matches!(
        c.retry_load(&other.editor).await,
        Err(durable::Error::Stale)
    ));
    assert_eq!(
        original.job().unwrap().state(),
        o::JobState::Failed(durable::Error::Storage)
    );
    done(
        f.ops()
            .fixture_fault(
                f.editor.scope().clone(),
                durable::test_support::FaultPoint::InventoryRead,
            )
            .unwrap(),
    );
    assert!(matches!(
        c.retry_load(&f.editor).await,
        Err(durable::Error::Storage)
    ));
    assert!(Arc::ptr_eq(
        &original,
        c.state.lock().unwrap().load.as_ref().unwrap()
    ));
    assert!(c.snapshot().unwrap().retry_load);
    assert!(c.retry_load(&f.editor).await.unwrap().loaded);
    assert!(
        f.ops()
            .resume_inventory(&original.job().unwrap().key())
            .is_err()
    );
    assert_eq!(
        other.editor.snapshot().view.local().composer.text,
        "Other account"
    );
}

#[path = "retirement_retry_tests.rs"]
mod retirement_retry_tests;

#[path = "hosted_closed_tests.rs"]
mod hosted_closed_tests;
