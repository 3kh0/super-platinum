//! Fresh private acknowledged idle drafts; only real composer Send consumes them.
use super::*;
use super_platinum_core::drafts::{
    codec,
    store::{DraftStore, sync::Commit},
    sync::{DraftSync, RemoteDraft},
};
fn seed(path: &std::path::Path) -> Result<(), String> {
    let mut store = DraftStore::open(path).map_err(|_| "Open private store")?;
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let key = key(target);
        let scope = Scope {
            account_id: ACCOUNT.into(),
            key: key.clone(),
        };
        let base = RemoteDraft {
            scope: scope.clone(),
            id: if matches!(target, ComposerTarget::Thread) {
                "DrSendThread"
            } else {
                "DrSendChannel"
            }
            .into(),
            client_msg_id: if matches!(target, ComposerTarget::Thread) {
                "00000000-0000-4000-8000-000000000103"
            } else {
                "00000000-0000-4000-8000-000000000102"
            }
            .into(),
            last_updated_ts: "10.000001".into(),
            snapshot: codec::encode_composer(&key, &local(BASE, target).composer)
                .map_err(|_| "Base codec")?,
        };
        let state = DraftSync::adopt_remote(&scope, base).map_err(|_| "Adopt base")?;
        let local = local(BASE, target);
        store
            .save([(key, local.clone())])
            .map_err(|_| "Save local seed")?;
        if !matches!(
            store
                .commit_synced(&scope, None, &local, &state, None)
                .map_err(|_| "Commit seed")?,
            Commit::Applied(_)
        ) {
            return Err("Seed CAS failed".into());
        }
    }
    Ok(())
}
pub(crate) async fn install(mut state: Signal<ShellState>) -> Result<Arc<FixtureBinding>, String> {
    let (path, nonce, scenario, phase) = isolation::configuration()?;
    let epoch = {
        let s = state.peek();
        if s.core.active_account.as_deref() != Some(ACCOUNT)
            || s.core.session.is_some()
            || s.core.transport.is_some()
            || s.core.cache.is_some()
        {
            return Err("Disconnected synthetic account required".into());
        }
        s.core.account_epoch
    };
    let (p, n, stage) = (path.clone(), nonce.clone(), phase.clone());
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        isolation::validate_directory(&p, &n)?;
        if stage == "exercise" {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
                .map_err(|_| "New database required")?;
            seed(&p)?;
        } else {
            let m =
                std::fs::symlink_metadata(&p).map_err(|_| "Existing restart database required")?;
            if !m.is_file() || m.file_type().is_symlink() {
                return Err("Invalid restart database".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if m.file_attributes() & 0x400 != 0 {
                    return Err("Reparse database refused".into());
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "Private storage task failed")??;
    let account = Account::new(Backend::Isolated {
        account: ACCOUNT.into(),
        path: path.clone(),
    });
    let mut views = Vec::new();
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let view = account
            .owner(key(target))
            .load()
            .await
            .map_err(|e| format!("Load isolated draft: {e:?}"))?;
        views.push((key(target), view));
    }
    let mut s = state.write();
    if s.core.account_epoch != epoch
        || s.core.active_account.as_deref() != Some(ACCOUNT)
        || s.core.session.is_some()
        || s.core.transport.is_some()
        || s.core.cache.is_some()
    {
        return Err("Owner changed during private load".into());
    }
    s.managed_drafts = session::Session {
        account: Some(ACCOUNT.into()),
        epoch,
        ready: true,
        ..Default::default()
    };
    let mut editors = BTreeMap::new();
    for (key, view) in views {
        s.managed_drafts
            .apply_view(ACCOUNT, epoch, key.clone(), view.clone());
        s.core.drafts.items.insert(key.clone(), view.local.clone());
        if view.stored.is_some() {
            editors.insert(
                key.clone(),
                s.draft_writes
                    .editor(&account, epoch, key, view)
                    .map_err(|e| format!("Isolated editor: {e:?}"))?,
            );
        }
    }
    let binding = Arc::new(FixtureBinding {
        account,
        path,
        nonce,
        scenario,
        phase,
        epoch,
        view_id: s.draft_writes.view_id,
        editors,
        requests: Mutex::new(vec![]),
        routed: AtomicU64::new(0),
        refused: AtomicU64::new(0),
        ui_done: AtomicBool::new(false),
        gate: Mutex::default(),
    });
    *installed()
        .lock()
        .map_err(|_| "Fixture binding unavailable")? = Arc::downgrade(&binding);
    s.core.drafts.channel = None;
    s.core.drafts.thread = None;
    s.sync_composer_drafts();
    Ok(binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::drafts::{
        store::sync::Prepare,
        sync::{ConsumeKind, Workflow},
    };
    struct PrivateSeed {
        path: PathBuf,
        account: Arc<Account>,
    }
    impl PrivateSeed {
        fn new() -> Self {
            let id = api::ForwardMessage::new("C1".into(), ROOT.into(), "C2".into(), vec![])
                .client_msg_id;
            let path = std::env::temp_dir().join(format!("managed-send-seed-{id}.sqlite"));
            seed(&path).unwrap();
            let account = Account::new(Backend::Isolated {
                account: ACCOUNT.into(),
                path: path.clone(),
            });
            Self { path, account }
        }
        fn binding(&self) -> FixtureBinding {
            FixtureBinding {
                account: self.account.clone(),
                path: self.path.clone(),
                nonce: "a".repeat(32),
                scenario: "held-response-channel".into(),
                phase: "exercise".into(),
                epoch: 1,
                view_id: 1,
                editors: BTreeMap::new(),
                requests: Mutex::new(vec![]),
                routed: AtomicU64::new(0),
                refused: AtomicU64::new(0),
                ui_done: AtomicBool::new(false),
                gate: Mutex::default(),
            }
        }
    }
    impl Drop for PrivateSeed {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    #[tokio::test]
    async fn seed_contains_only_acknowledged_idle_scopes_and_no_consumption() {
        let seed = PrivateSeed::new();
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let owner = seed.account.owner(key(target));
            let view = owner.load().await.unwrap();
            assert_eq!(json!(view.local), json!(local(BASE, target)));
            let stored = view.stored.unwrap();
            assert_eq!(stored.revision, 1);
            assert!(matches!(stored.state.workflow(), Workflow::Idle));
            assert!(stored.state.consumption().is_none());
            let ack = stored.state.acknowledged().unwrap();
            assert_eq!(stored.state.desired(), Some(&ack.snapshot));
            assert_eq!(ack.scope.key, key(target));
            assert_eq!(ack.last_updated_ts, "10.000001");
        }
    }
    #[tokio::test]
    async fn fixture_inventory_does_not_normalize_live_send_but_real_restart_does() {
        let seed = PrivateSeed::new();
        let owner = seed.account.owner(key(ComposerTarget::Channel));
        let initial = owner.load().await.unwrap().stored.unwrap();
        let edited = owner
            .checkpoint(initial.revision, local(REQUESTED, ComposerTarget::Channel))
            .await
            .unwrap();
        let consumed = owner
            .consume(edited.revision, ConsumeKind::Send)
            .await
            .unwrap();
        let Prepare::Prepared { operation, .. } = seed
            .account
            .backend
            .prepare(owner.scope.clone(), consumed.revision)
            .await
            .unwrap()
        else {
            panic!("Synthetic Send preparation refused")
        };
        let binding = seed.binding();
        let before = durable(&binding).await.unwrap();
        assert_eq!(
            before["channel"]["stored"]["state"]["workflow"],
            json!({"InFlight":operation})
        );
        assert_eq!(durable(&binding).await.unwrap(), before);
        let reopened = Account::new(seed.account.backend.clone())
            .owner(owner.scope.key.clone())
            .load()
            .await
            .unwrap()
            .stored
            .unwrap();
        assert_eq!(reopened.state.workflow(), &Workflow::Uncertain(operation));
        assert_eq!(
            reopened.revision,
            before["channel"]["stored"]["revision"].as_u64().unwrap() + 1
        );
    }
}
