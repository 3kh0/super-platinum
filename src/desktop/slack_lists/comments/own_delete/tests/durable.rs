use super::*;
use crate::slack_lists::comments::own_delete::durable::{Outcome, Recovery};
use super_platinum_core::slack_lists::comments::{
    OwnEditPolicy, OwnEditSeed, journal::Scope as DiskScope,
};

pub(in crate::slack_lists::comments::own_delete) struct Local(pub std::path::PathBuf);
impl Local {
    pub(in crate::slack_lists::comments::own_delete) fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "list-own-delete-host-{}.sqlite",
            uuid::Uuid::new_v4()
        )))
    }
    pub(in crate::slack_lists::comments::own_delete) fn disk(
        &self,
        record: &OwnDeleteRecord,
    ) -> storage::Disk {
        storage::Disk::test(
            DiskScope::capture(record.workspace(), record.user(), record.target()).unwrap(),
            self.0.clone(),
        )
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("sqlite-journal"));
    }
}

#[tokio::test]
async fn pending_commit_restores_original_uuid_and_cannot_be_replayed() {
    let (shell, source, policy) = setup();
    let record = intent(&shell, &source, &policy).record().clone();
    let local = Local::new();
    let disk = local.disk(&record);
    let recovery = Recovery::load(disk.clone(), TS.into()).await.unwrap();
    let id = recovery.begin(&record).await.unwrap();
    let committed = recovery.inventory().await;
    assert_eq!(committed.attempt().unwrap().phase(), DeletePhase::Pending);
    assert_eq!(committed.attempt().unwrap().id(), id);
    assert_eq!(committed.attempt().unwrap().record(), &record);
    assert!(committed.attempt().unwrap().raw().is_none());
    drop(recovery);
    let restored = Recovery::load(disk, TS.into()).await.unwrap();
    assert_eq!(restored.inventory().await, committed);
    assert!(restored.begin(&record).await.is_err());
    let raw = json!({"ok":false,"error":"internal_error","opaque":"retain original"});
    restored
        .resolve(id, Outcome::Uncertain(Some(raw.clone())), 0)
        .await
        .unwrap();
    let unknown = restored.inventory().await;
    assert_eq!(unknown.attempt().unwrap().phase(), DeletePhase::Uncertain);
    assert_eq!(unknown.attempt().unwrap().raw(), Some(&raw));
    assert!(restored.begin(&record).await.is_err());
}

#[tokio::test]
async fn failed_ack_commit_retains_exact_receipt_until_flush_and_never_touches_reply_or_edit() {
    let (shell, source, policy) = setup();
    let intent = intent(&shell, &source, &policy);
    let record = intent.record().clone();
    let local = Local::new();
    let disk = local.disk(&record);
    let edit_policy = OwnEditPolicy::observed(
        "T1",
        record.user(),
        json!({"id":record.target().channel_id()}),
        true,
        false,
        vec![-1],
        false,
        false,
    )
    .unwrap();
    let seed = OwnEditSeed::capture(
        "T1",
        record.user(),
        shell
            .slack_lists
            .comments
            .entry
            .as_ref()
            .unwrap()
            .page
            .as_ref()
            .unwrap(),
        &source.original,
        &edit_policy,
        1_700_000_100_000,
    )
    .unwrap();
    let scope = disk.scope.clone();
    let saved = disk
        .work(move |j| {
            let reply = j.inventory(&scope)?;
            let reply = j.save(
                &reply,
                &uuid::Uuid::new_v4().to_string(),
                rich("Independent reply A 中🙂"),
            )?;
            let edit = j.edit_inventory(&scope, TS)?;
            let edit = j.edit_save(
                &edit,
                &seed,
                &uuid::Uuid::new_v4().to_string(),
                rich("Independent unsent edit B 日"),
            )?;
            Ok((reply, edit))
        })
        .await
        .unwrap();
    let recovery = Recovery::load(disk.clone(), TS.into()).await.unwrap();
    let id = recovery.begin(&record).await.unwrap();
    let raw =
        json!({"ok":true,"channel":source.target.channel_id(),"ts":TS,"opaque":"original ACK"});
    let ack = Arc::new(intent.accept(raw.clone()).unwrap());
    // Isolated synthetic DB only. Like existing own-edit tests, force a real
    // SQLite open failure and restore its exact closed bytes before retry.
    let original = std::fs::read(&local.0).unwrap();
    std::fs::write(&local.0, b"temporary invalid test database").unwrap();
    assert!(
        recovery
            .resolve(id, Outcome::Confirmed(ack.clone()), 0)
            .await
            .is_err()
    );
    assert!(recovery.pending().await);
    assert_eq!(
        recovery.inventory().await.attempt().unwrap().phase(),
        DeletePhase::Pending
    );
    assert!(recovery.begin(&record).await.is_err());
    std::fs::write(&local.0, original).unwrap();
    let Some(Outcome::Confirmed(recovered)) = recovery.flush().await.unwrap() else {
        panic!("original receipt missing")
    };
    assert!(Arc::ptr_eq(&ack, &recovered));
    assert_eq!(recovered.raw(), &raw);
    assert!(!recovery.pending().await);
    assert!(recovery.flush().await.unwrap().is_none());
    let inventory = Recovery::load(disk.clone(), TS.into())
        .await
        .unwrap()
        .inventory()
        .await;
    assert_eq!(inventory.attempt().unwrap().phase(), DeletePhase::Confirmed);
    assert_eq!(inventory.attempt().unwrap().raw(), Some(&raw));
    let scope = disk.scope.clone();
    let restored = disk
        .work(move |j| Ok((j.inventory(&scope)?, j.edit_inventory(&scope, TS)?)))
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::to_value(&saved).unwrap()
    );
    assert!(recovery.begin(&record).await.is_err());
}

#[tokio::test]
async fn positive_readback_is_separate_from_unknown_receipt_and_missing_page_is_not_proof() {
    let (shell, source, policy) = setup();
    let record = intent(&shell, &source, &policy).record().clone();
    let local = Local::new();
    let disk = local.disk(&record);
    let recovery = Recovery::load(disk, TS.into()).await.unwrap();
    let id = recovery.begin(&record).await.unwrap();
    let raw = json!({"ok":false,"error":"fatal_error"});
    recovery
        .resolve(id.clone(), Outcome::Uncertain(Some(raw.clone())), 0)
        .await
        .unwrap();
    let missing = Page::parse(
        &source.target,
        json!({"ok":true,"messages":[],"has_more":false}),
    )
    .unwrap();
    assert!(!record.observed(&missing).unwrap());
    let deleted = json!({"ok":true,"messages":[],"deleted":[TS],"has_more":false});
    let page = Page::parse(&source.target, deleted.clone()).unwrap();
    assert!(record.observed(&page).unwrap());
    recovery
        .resolve(id, Outcome::Observed(page), 0)
        .await
        .unwrap();
    let inventory = recovery.inventory().await;
    assert_eq!(inventory.attempt().unwrap().phase(), DeletePhase::Observed);
    assert_eq!(inventory.attempt().unwrap().raw(), Some(&raw));
    assert_eq!(inventory.attempt().unwrap().readback(), Some(&deleted));
    assert!(recovery.begin(&record).await.is_err());
}
