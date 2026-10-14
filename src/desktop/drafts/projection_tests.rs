//! Owner boundaries must preserve the codec's accepted raw representations.
use super::*;
use super_platinum_core::{
    ComposerState,
    drafts::{
        store::DraftStore,
        sync::{ConsumeKind, OperationKind},
    },
};

struct Fixture {
    account: Arc<Account>,
    owner: Owner,
    path: std::path::PathBuf,
    local: LocalDraft,
    remote: RemoteDraft,
}
impl Fixture {
    fn new(text: &str, broadcast: Option<bool>) -> Self {
        let account_id = "00000000-0000-4000-8000-000000000081";
        let key = DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some("1719800000.000001".into()),
        };
        let mut composer = ComposerState::with_text(text);
        composer.reply_broadcast = broadcast == Some(true);
        let local = LocalDraft {
            composer,
            attachments: vec![],
        };
        let mut snapshot = codec::encode_composer(&key, &local.composer).unwrap();
        if let Some(broadcast) = broadcast {
            snapshot.destinations[0]["broadcast"] = broadcast.into();
        }
        let remote = RemoteDraft {
            scope: Scope {
                account_id: account_id.into(),
                key: key.clone(),
            },
            id: "DrProjection".into(),
            client_msg_id: "00000000-0000-4000-8000-000000000091".into(),
            last_updated_ts: "1719800000000".into(),
            snapshot,
        };
        let unique = DraftSync::new(remote.scope.clone());
        let path = std::env::temp_dir().join(format!(
            "draft-projection-{}.sqlite",
            unique.client_msg_id()
        ));
        DraftStore::open(&path)
            .unwrap()
            .save([(key.clone(), local.clone())])
            .unwrap();
        let account = Account::new(Backend::Isolated {
            account: account_id.into(),
            path: path.clone(),
        });
        let owner = account.owner(key);
        Self {
            account,
            owner,
            path,
            local,
            remote,
        }
    }
    async fn enroll(&self) -> Stored {
        self.owner
            .enroll(self.local.clone(), Some(self.remote.clone()))
            .await
            .unwrap()
    }
    fn restart(&self) -> Owner {
        Account::new(self.account.backend.clone()).owner(self.owner.scope.key.clone())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[tokio::test]
async fn selection_only_checkpoints_retain_each_broadcast_shape_without_an_update() {
    for broadcast in [None, Some(false), Some(true)] {
        let f = Fixture::new("Reply 日本語 🚀", broadcast);
        let initial = f.enroll().await;
        let mut selected = f.local.clone();
        selected.composer.set_selection(0, 5);
        let changed = f
            .owner
            .checkpoint(initial.revision, selected.clone())
            .await
            .unwrap();
        assert_eq!(changed.state.desired(), Some(&f.remote.snapshot));
        assert_eq!(
            changed.state.local_revision(),
            initial.state.local_revision()
        );
        assert_eq!(changed.state.acknowledged(), Some(&f.remote));
        assert!(changed.revision > initial.revision);
        let restored = f.restart().load().await.unwrap();
        assert!(same_local(&restored.local, &selected));
        assert_eq!(restored.stored.unwrap().state, changed.state);
        let mut reducer = changed.state;
        assert!(reducer.begin_operation(&f.owner.scope).is_none());
    }
}

#[tokio::test]
async fn explicit_false_survives_text_edit_restart_and_frozen_consumption() {
    let f = Fixture::new("Original", Some(false));
    let initial = f.enroll().await;
    let mut edited = f.local.clone();
    edited.composer = ComposerState::with_text("Edited 日本語 🚀");
    let changed = f.owner.checkpoint(initial.revision, edited).await.unwrap();
    let mut expected = codec::encode(&f.owner.scope.key, "Edited 日本語 🚀").unwrap();
    expected.destinations[0]["broadcast"] = false.into();
    assert_eq!(changed.state.desired(), Some(&expected));
    let restarted = f.restart();
    let restored = restarted.load().await.unwrap().stored.unwrap();
    let consumed = restarted
        .consume(restored.revision, ConsumeKind::Send)
        .await
        .unwrap();
    assert!(restarted.load().await.unwrap().local.empty());
    let intent = consumed.state.consumption().unwrap();
    assert_eq!(intent.snapshot, expected);
    assert_eq!(intent.base.as_ref(), Some(&f.remote));
    let mut newer = f.local.clone();
    newer.composer = ComposerState::with_text("Next message");
    let typed = restarted
        .checkpoint(consumed.revision, newer)
        .await
        .unwrap();
    assert_eq!(typed.state.consumption(), Some(intent));
    assert!(
        typed.state.desired().unwrap().destinations[0]
            .get("broadcast")
            .is_none()
    );
}

#[tokio::test]
async fn empty_adopted_content_is_preserved_until_explicit_clear_and_cannot_send() {
    let f = Fixture::new("", Some(false));
    let initial = f.enroll().await;
    let unchanged = f
        .owner
        .checkpoint(initial.revision, f.local.clone())
        .await
        .unwrap();
    assert_eq!(unchanged.state.desired(), Some(&f.remote.snapshot));
    assert!(!unchanged.state.tombstone());
    assert!(matches!(
        f.owner.consume(unchanged.revision, ConsumeKind::Send).await,
        Err(Fault::Pending)
    ));
    assert_eq!(
        f.owner.load().await.unwrap().stored.unwrap().revision,
        unchanged.revision
    );
    let cleared = f.owner.clear(unchanged.revision).await.unwrap();
    assert!(cleared.state.desired().is_none());
    assert!(cleared.state.tombstone());
    let restored = f.restart().load().await.unwrap().stored.unwrap();
    assert_eq!(restored.state, cleared.state);
    let mut reducer = restored.state;
    let deletion = reducer.begin_operation(&f.owner.scope).unwrap();
    assert_eq!(deletion.kind, OperationKind::Delete);
    assert_eq!(deletion.base, Some(f.remote.clone()));
}

#[tokio::test]
async fn empty_broadcast_only_changes_preserve_content_in_both_directions() {
    for initial_broadcast in [false, true] {
        let f = Fixture::new("", Some(initial_broadcast));
        let initial = f.enroll().await;
        let mut changed_local = f.local.clone();
        changed_local.composer.reply_broadcast = !initial_broadcast;
        let changed = f
            .owner
            .checkpoint(initial.revision, changed_local.clone())
            .await
            .unwrap();
        let mut expected = f.remote.snapshot.clone();
        expected.destinations[0]["broadcast"] = (!initial_broadcast).into();
        assert_eq!(changed.state.desired(), Some(&expected));
        assert!(!changed.state.tombstone());
        assert_eq!(
            changed.state.local_revision(),
            initial.state.local_revision() + 1
        );
        let restored = f.restart().load().await.unwrap();
        assert!(same_local(&restored.local, &changed_local));
        assert_eq!(restored.stored.unwrap().state, changed.state);
        let mut reducer = changed.state;
        let update = reducer.begin_operation(&f.owner.scope).unwrap();
        assert_eq!(update.kind, OperationKind::Update);
        assert_eq!(update.snapshot, Some(expected));
    }
}

#[tokio::test]
async fn erasing_nonempty_text_is_still_a_clear_not_an_empty_raw_update() {
    let f = Fixture::new("Original", Some(false));
    let initial = f.enroll().await;
    let cleared = f
        .owner
        .checkpoint(initial.revision, LocalDraft::default())
        .await
        .unwrap();
    assert!(cleared.state.desired().is_none());
    assert!(cleared.state.tombstone());
    assert_eq!(cleared.state.acknowledged(), Some(&f.remote));
}

#[tokio::test]
async fn remote_enrollment_rejects_opaque_shapes_and_mismatching_content() {
    for kind in ["metadata", "files", "text", "destination", "scope"] {
        let f = Fixture::new("Original", Some(false));
        let mut remote = f.remote.clone();
        match kind {
            "metadata" => remote.snapshot.blocks[0]["block_id"] = "opaque".into(),
            "files" => remote.snapshot.file_ids.push("F1".into()),
            "text" => {
                remote.snapshot = codec::encode(&f.owner.scope.key, "Different").unwrap();
            }
            "destination" => remote.snapshot.destinations[0]["unknown"] = true.into(),
            "scope" => remote.scope.key.channel = "C2".into(),
            _ => unreachable!(),
        }
        assert!(matches!(
            f.owner.enroll(f.local.clone(), Some(remote)).await,
            Err(Fault::Unsupported)
        ));
        let unchanged = f.owner.load().await.unwrap();
        assert!(unchanged.stored.is_none());
        assert!(same_local(&unchanged.local, &f.local));
    }
}

#[tokio::test]
async fn unsupported_edits_and_stale_checkpoints_leave_the_pair_unchanged() {
    let f = Fixture::new("Original", Some(false));
    let initial = f.enroll().await;
    let mut unsupported = f.local.clone();
    unsupported.composer.text = "<@U123>".into();
    assert!(matches!(
        f.owner.checkpoint(initial.revision, unsupported).await,
        Err(Fault::Unsupported)
    ));
    assert!(matches!(
        f.owner
            .checkpoint(initial.revision - 1, f.local.clone())
            .await,
        Err(Fault::Stale)
    ));
    let unchanged = f.owner.load().await.unwrap();
    assert_eq!(unchanged.stored.unwrap().state, initial.state);
    assert!(same_local(&unchanged.local, &f.local));
    let mut opaque = f.remote.snapshot.clone();
    opaque.blocks[0]["block_id"] = "unsupported".into();
    assert!(matches!(
        retained_snapshot(&f.owner.scope.key, &f.local, Some(&opaque)),
        Err(Fault::Unsupported)
    ));
}
