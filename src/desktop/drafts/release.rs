//! Explicit return to ordinary local editing after all remote ownership ends.
use super::*;
use super_platinum_core::drafts::store::sync::Release;

impl Owner {
    pub async fn release(&self, expected: u64) -> Result<LocalDraft, Fault> {
        let _guard = self.gate.lock().await;
        self.current().await?.ready(expected)?;
        match self
            .account
            .backend
            .release(self.scope.clone(), expected)
            .await?
        {
            Release::Released(local) => Ok(local),
            Release::Stale(_) => Err(Fault::Stale),
            Release::Blocked(_) => Err(Fault::Pending),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::drafts::{store::DraftStore, sync::ConsumeKind};

    #[tokio::test]
    async fn owner_release_preserves_local_and_refuses_outstanding_consumption() {
        for pending in [false, true] {
            let id = super_platinum_core::slack::api::ForwardMessage::new(
                "C1".into(),
                "1.000001".into(),
                "C2".into(),
                vec![],
            )
            .client_msg_id;
            let path = std::env::temp_dir().join(format!("draft-release-{id}.sqlite"));
            let key = DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            };
            let local = LocalDraft {
                composer: super_platinum_core::ComposerState::with_text("Retain newer local text"),
                attachments: vec![],
            };
            DraftStore::open(&path)
                .unwrap()
                .save([(key.clone(), local.clone())])
                .unwrap();
            let account = Account::new(Backend::Isolated {
                account: "00000000-0000-4000-8000-000000000093".into(),
                path: path.clone(),
            });
            let owner = account.owner(key);
            let mut stored = owner.enroll(local.clone(), None).await.unwrap();
            if pending {
                stored = owner
                    .consume(stored.revision, ConsumeKind::Send)
                    .await
                    .unwrap();
                assert!(matches!(
                    owner.release(stored.revision).await,
                    Err(Fault::Pending)
                ));
                assert!(
                    owner
                        .load()
                        .await
                        .unwrap()
                        .stored
                        .unwrap()
                        .state
                        .consumption()
                        .is_some()
                );
            } else {
                let retained = owner.release(stored.revision).await.unwrap();
                assert!(same_local(&retained, &local));
                let view = owner.load().await.unwrap();
                assert!(view.stored.is_none());
                assert!(same_local(&view.local, &local));
            }
            std::fs::remove_file(path).unwrap();
        }
    }
}
