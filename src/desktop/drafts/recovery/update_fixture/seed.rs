//! Fresh private SQLite seed containing a frozen uncertain Update.
use super::*;
use super_platinum_core::drafts::store::DraftStore;
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
                "DrUpdateThread"
            } else {
                "DrUpdateChannel"
            }
            .into(),
            client_msg_id: if matches!(target, ComposerTarget::Thread) {
                "00000000-0000-4000-8000-000000000093"
            } else {
                "00000000-0000-4000-8000-000000000092"
            }
            .into(),
            last_updated_ts: "10.000001".into(),
            snapshot: codec::encode_composer(&key, &local(BASE, target).composer)
                .map_err(|_| "Base codec")?,
        };
        let mut state = DraftSync::adopt_remote(&scope, base).map_err(|_| "Adopt base")?;
        let local = local(REQUESTED, target);
        state.edit(
            &scope,
            Some(codec::encode_composer(&key, &local.composer).map_err(|_| "Update codec")?),
        );
        let op = state
            .begin_operation(&scope)
            .ok_or("Update admission failed")?;
        if op.kind != OperationKind::Update || !state.mark_uncertain(&scope, &op.id) {
            return Err("Wrong uncertain seed".into());
        }
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
        editors.insert(
            key.clone(),
            s.draft_writes
                .editor(&account, epoch, key, view)
                .map_err(|e| format!("Isolated editor: {e:?}"))?,
        );
    }
    let binding = Arc::new(FixtureBinding {
        account,
        path,
        nonce,
        scenario,
        phase,
        epoch,
        editors,
        requests: Mutex::new(vec![]),
        routed: AtomicU64::new(0),
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
    #[test]
    fn seed_contains_exact_uncertain_updates_and_separate_destinations() {
        let path = std::env::temp_dir().join(format!(
            "update-recovery-seed-{}.sqlite",
            super_platinum_core::drafts::store::LocalWriter::default().id()
        ));
        seed(&path).unwrap();
        let mut store = DraftStore::open(&path).unwrap();
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let scope = Scope {
                account_id: ACCOUNT.into(),
                key: key(target),
            };
            let review = store.read_local_review(&scope).unwrap();
            assert_eq!(review.local.composer.text, REQUESTED);
            assert_eq!(
                review.local.composer.reply_broadcast,
                matches!(target, ComposerTarget::Thread)
            );
            assert!(review.blocked.is_none() && review.alternates.is_empty());
            let state = &review.stored.as_ref().unwrap().state;
            let Workflow::Uncertain(op) = state.workflow() else {
                panic!("Expected uncertain Update")
            };
            assert_eq!(op.kind, OperationKind::Update);
            assert_eq!(op.scope, scope);
            assert!(op.consume_id.is_none() && state.pending_check(op).is_none());
            assert_eq!(op.base.as_ref().unwrap().last_updated_ts, "10.000001");
            assert_eq!(op.snapshot.as_ref(), state.desired());
            assert_eq!(
                codec::decode(&scope.key, op.snapshot.as_ref().unwrap())
                    .unwrap()
                    .composer()
                    .text,
                REQUESTED
            );
        }
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}
