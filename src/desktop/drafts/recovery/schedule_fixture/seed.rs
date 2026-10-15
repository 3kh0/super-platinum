//! Seed frozen uncertain rich Schedule beside a distinct local draft.
use super::*;
use super_platinum_core::drafts::{
    codec,
    store::{DraftStore, sync::Commit},
    sync::{ConsumeKind, DraftSync, RemoteDraft},
};
pub(super) fn withheld_keys() -> Vec<DraftKey> {
    vec![
        DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        },
        DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: Some(ROOT.into()),
        },
        DraftKey {
            team: "T1".into(),
            channel: "C2".into(),
            thread: Some("1719800300.000000".into()),
        },
    ]
}
fn seed(path: &std::path::Path, scenario: &str) -> Result<(), String> {
    let mut store = DraftStore::open(path).map_err(|_| "Open private store")?;
    let mut keys = vec![key(ComposerTarget::Channel), key(ComposerTarget::Thread)];
    if scenario == "positive-channel" {
        keys.extend(withheld_keys());
    }
    for (index, key) in keys.into_iter().enumerate() {
        let scope = Scope {
            account_id: ACCOUNT.into(),
            key: key.clone(),
        };
        let mut snapshot = frozen(&key);
        if index >= 2 {
            snapshot.blocks[0]["elements"][0]["elements"][0]["text"] = json!(match index {
                2 => "Withheld unknown ID schedule",
                3 => "Withheld file schedule",
                _ => "Withheld extra routing schedule",
            });
            if index == 3 {
                snapshot.file_ids = vec!["FWithheld".into()];
            }
            if index == 4 {
                snapshot.destinations[0]["unproven_route"] = json!("retained");
            }
        }
        let mut state = if index == 2 {
            let mut state = DraftSync::new(scope.clone());
            state.edit(&scope, Some(snapshot));
            state
        } else {
            let base = RemoteDraft {
                scope: scope.clone(),
                id: match index {
                    0 => "DrScheduleChannel",
                    1 => "DrScheduleThread",
                    3 => "DrScheduleFile",
                    _ => "DrScheduleRouting",
                }
                .into(),
                client_msg_id: format!("00000000-0000-4000-8000-{:012}", 112 + index),
                last_updated_ts: "10.000001".into(),
                snapshot,
            };
            DraftSync::adopt_remote(&scope, base).map_err(|_| "Adopt rich Schedule base")?
        };
        let revision = state.local_revision();
        let intent = state
            .request_consume(&scope, revision, ConsumeKind::Schedule { post_at: POST_AT })
            .map_err(|_| "Freeze Schedule")?;
        let op = state.begin_operation(&scope).ok_or("Prepare Schedule")?;
        if op.kind != (OperationKind::Schedule { post_at: POST_AT })
            || !state.mark_uncertain(&scope, &op.id)
        {
            return Err("Wrong uncertain Schedule seed".into());
        }
        let target = if key.thread.is_some() {
            ComposerTarget::Thread
        } else {
            ComposerTarget::Channel
        };
        let local = local(CURRENT, target);
        state
            .checkpoint_direct_local(
                &intent,
                Some(codec::encode_composer(&key, &local.composer).map_err(|_| "Local codec")?),
            )
            .map_err(|_| "Preserve new local beside frozen Schedule")?;
        store
            .save([(key, local.clone())])
            .map_err(|_| "Save canonical")?;
        if !matches!(
            store
                .commit_synced(&scope, None, &local, &state, None)
                .map_err(|_| "Commit Schedule seed")?,
            Commit::Applied(_)
        ) {
            return Err("Seed CAS refused".into());
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
    let scenario_for_seed = scenario.clone();
    let (p, n, stage) = (path.clone(), nonce.clone(), phase.clone());
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        isolation::validate_directory(&p, &n)?;
        if stage == "exercise" {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
                .map_err(|_| "New database required")?;
            seed(&p, &scenario_for_seed)?;
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
    let mut keys = vec![key(ComposerTarget::Channel), key(ComposerTarget::Thread)];
    if scenario == "positive-channel" {
        keys.extend(withheld_keys());
    }
    for key in keys {
        let view = account
            .owner(key.clone())
            .load()
            .await
            .map_err(|e| format!("Load isolated draft: {e:?}"))?;
        views.push((key, view));
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
    let mut operations = BTreeMap::new();
    let mut withheld = Vec::new();
    for (key, view) in views {
        let op = view
            .stored
            .as_ref()
            .and_then(|stored| stored.state.pending())
            .cloned()
            .ok_or("Seed pending Schedule missing")?;
        if key == self::key(ComposerTarget::Channel) || key == self::key(ComposerTarget::Thread) {
            operations.insert(key.clone(), op);
        } else {
            withheld.push(op);
        }
        s.managed_drafts
            .apply_view(ACCOUNT, epoch, key.clone(), view.clone());
        s.core.drafts.items.insert(key.clone(), view.local.clone());
        if view.stored.is_some()
            && !(matches!(scenario.as_str(), "successor-channel" | "reenroll-channel")
                && key == self::key(ComposerTarget::Channel))
        {
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
        operations,
        withheld,
        cooldown: Mutex::new(0),
        requests: Mutex::new(vec![]),
        routed: AtomicU64::new(0),
        refused: AtomicU64::new(0),
        ui_done: AtomicBool::new(false),
        gate: Mutex::default(),
    });
    *installed()
        .lock()
        .map_err(|_| "Fixture binding unavailable")? = Arc::downgrade(&binding);
    s.core.scheduled.ensure_scope(epoch, "T1");
    s.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Drafts;
    s.core.drafts.channel = None;
    s.core.drafts.thread = None;
    s.sync_composer_drafts();
    Ok(binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Private {
        path: PathBuf,
    }
    impl Private {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "schedule-recovery-seed-{}.sqlite",
                super_platinum_core::drafts::store::LocalWriter::default().id()
            ));
            seed(&path, "positive-channel").unwrap();
            Self { path }
        }
    }
    impl Drop for Private {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    #[test]
    fn seed_preserves_rich_schedule_and_distinct_paired_local_with_withheld_cases() {
        let private = Private::new();
        let mut store = DraftStore::open(&private.path).unwrap();
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let key = key(target);
            let scope = Scope {
                account_id: ACCOUNT.into(),
                key: key.clone(),
            };
            let review = store.read_local_review(&scope).unwrap();
            assert_eq!(json!(review.local), json!(local(CURRENT, target)));
            let stored = review.stored.unwrap();
            assert_eq!(stored.revision, 1);
            let Workflow::Uncertain(op) = stored.state.workflow() else {
                panic!("uncertain Schedule required")
            };
            assert!(op.known_schedule_check());
            assert_eq!(op.local_revision, 0);
            assert_eq!(op.snapshot, Some(frozen(&key)));
            assert_eq!(stored.state.local_revision(), 2);
            assert!(stored.state.pending_check(op).is_none());
        }
        for (index, key) in withheld_keys().into_iter().enumerate() {
            let review = store
                .read_local_review(&Scope {
                    account_id: ACCOUNT.into(),
                    key,
                })
                .unwrap();
            let stored = review.stored.unwrap();
            let op = stored.state.pending().unwrap();
            assert!(matches!(
                op.kind,
                OperationKind::Schedule { post_at: POST_AT }
            ));
            assert!(!op.known_schedule_check());
            assert!(crate::drafts::recovery::CheckKind::for_operation(op).is_none());
            if index == 0 {
                assert!(op.base.is_none());
            } else if index == 1 {
                assert_eq!(op.snapshot.as_ref().unwrap().file_ids, vec!["FWithheld"]);
            } else {
                assert_eq!(
                    op.snapshot.as_ref().unwrap().destinations[0]["unproven_route"],
                    "retained"
                );
            }
        }
    }
    #[tokio::test]
    async fn current_inventory_preserves_durable_pending_ticket_across_true_restart() {
        let private = Private::new();
        let backend = Backend::Isolated {
            account: ACCOUNT.into(),
            path: private.path.clone(),
        };
        let account = Account::new(backend.clone());
        let owner = account.owner(key(ComposerTarget::Channel));
        let view = owner.load().await.unwrap();
        let op = view
            .stored
            .as_ref()
            .unwrap()
            .state
            .pending()
            .unwrap()
            .clone();
        let ticket = owner.reserve_check(&op, 100).await.unwrap();
        let before = owner.review_local().await.unwrap();
        assert_eq!(
            before.stored.as_ref().unwrap().state.pending_check(&op),
            Some(&ticket)
        );
        assert_eq!(
            json!(crate::drafts::remote_review_fixture::inventory_json(
                &before
            )),
            json!(crate::drafts::remote_review_fixture::inventory_json(
                &owner.review_local().await.unwrap()
            ))
        );
        let restart = Account::new(backend).owner(op.scope.key.clone());
        assert_eq!(
            restart.reserve_check(&op, u64::MAX).await,
            Err(CheckError::Pending)
        );
        assert_eq!(
            crate::drafts::remote_review_fixture::inventory_json(
                &restart.review_local().await.unwrap()
            ),
            crate::drafts::remote_review_fixture::inventory_json(&before)
        );
    }
}
