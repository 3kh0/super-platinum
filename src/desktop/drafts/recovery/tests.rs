use super::*;
use super_platinum_core::{
    ComposerState,
    drafts::{store::DraftStore, sync::ConsumeKind},
};

struct Fixture {
    owner: Owner,
    path: std::path::PathBuf,
}
impl Fixture {
    async fn new() -> (Self, Operation) {
        let id = super_platinum_core::slack::api::ForwardMessage::new(
            "C1".into(),
            "1.000001".into(),
            "C2".into(),
            vec![],
        )
        .client_msg_id;
        let path = std::env::temp_dir().join(format!("draft-check-{id}.sqlite"));
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        };
        let local = LocalDraft {
            composer: ComposerState::with_text("Saved 日本語 🚀"),
            attachments: vec![],
        };
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), local.clone())])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: id,
            path: path.clone(),
        });
        let owner = account.owner(key);
        let stored = owner.enroll(local, None).await.unwrap();
        let consumed = owner
            .consume(stored.revision, ConsumeKind::Send)
            .await
            .unwrap();
        let receipt = owner
            .dispatch(consumed.revision, |_| async { Outcome::Uncertain })
            .await
            .unwrap();
        (Self { owner, path }, receipt.operation)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[tokio::test]
async fn cross_view_checks_are_serialized_and_restart_keeps_deadline() {
    let (fixture, op) = Fixture::new().await;
    let other = fixture.owner.account.owner(op.scope.key.clone());
    let (a, b) = tokio::join!(
        fixture.owner.reserve_check(&op, 100),
        other.reserve_check(&op, 100)
    );
    assert!(matches!(
        (a, b),
        (Ok(()), Err(CheckError::Cooldown(30100))) | (Err(CheckError::Cooldown(30100)), Ok(()))
    ));
    let restarted = Account::new(fixture.owner.account.backend.clone()).owner(op.scope.key.clone());
    assert_eq!(
        restarted.reserve_check(&op, 30099).await,
        Err(CheckError::Cooldown(30100))
    );
    restarted.defer_check(&op, 90_000).await.unwrap();
    restarted.defer_check(&op, 1).await.unwrap();
    assert_eq!(
        restarted.reserve_check(&op, 30101).await,
        Err(CheckError::Cooldown(90000))
    );
}

#[tokio::test]
async fn stale_callback_and_foreign_account_cannot_reserve_check() {
    let (fixture, op) = Fixture::new().await;
    let mut foreign = op.clone();
    foreign.scope.account_id.push('x');
    assert_eq!(
        fixture.owner.reserve_check(&foreign, 1).await,
        Err(CheckError::Fault(Fault::InvalidReceipt))
    );
    let mut stale = op.clone();
    stale.id.push('x');
    assert_eq!(
        fixture.owner.reserve_check(&stale, 1).await,
        Err(CheckError::Fault(Fault::Stale))
    );
    fixture
        .owner
        .record(
            op.clone(),
            Outcome::Sent {
                client_msg_id: op.client_msg_id.clone(),
                draft_id: None,
                ts: "1700000000.000001".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        fixture.owner.reserve_check(&op, 1).await,
        Err(CheckError::Fault(Fault::Stale))
    );
}

#[tokio::test]
async fn failed_deadline_save_blocks_checks_until_disk_write_succeeds() {
    let (fixture, op) = Fixture::new().await;
    let held = fixture.path.with_extension("held");
    std::fs::rename(&fixture.path, &held).unwrap();
    std::fs::create_dir(&fixture.path).unwrap();
    assert!(matches!(
        fixture.owner.defer_check(&op, 120_000).await,
        Err(Fault::Storage)
    ));
    assert_eq!(
        fixture.owner.reserve_check(&op, 100).await,
        Err(CheckError::Fault(Fault::Storage))
    );
    std::fs::remove_dir(&fixture.path).unwrap();
    std::fs::rename(held, &fixture.path).unwrap();
    assert_eq!(
        fixture.owner.reserve_check(&op, 100).await,
        Err(CheckError::Cooldown(120000))
    );
    assert!(!barriers().lock().unwrap().contains_key(&barrier_key(&op)));
    let stored = fixture.owner.load().await.unwrap().stored.unwrap();
    assert_eq!(stored.state.check_retry_at_ms(&op), Some(120000));
}
