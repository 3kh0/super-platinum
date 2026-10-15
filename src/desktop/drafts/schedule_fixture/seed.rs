//! Private ordinary predecessor or exact rejected raw-rich Schedule; real dialog drives decisions.
use super::*;
use super_platinum_core::drafts::{
    codec,
    store::{DraftStore, sync::Commit},
    sync::{DraftSync, RemoteDraft},
};
fn seed(path: &std::path::Path, scenario: &str) -> Result<(), String> {
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
                "DrScheduleThread"
            } else {
                "DrScheduleChannel"
            }
            .into(),
            client_msg_id: if matches!(target, ComposerTarget::Thread) {
                "00000000-0000-4000-8000-000000000123"
            } else {
                "00000000-0000-4000-8000-000000000122"
            }
            .into(),
            last_updated_ts: "10.000001".into(),
            snapshot: codec::encode_composer(&key, &local(BASE, target).composer)
                .map_err(|_| "Base codec")?,
        };
        let mut state = DraftSync::adopt_remote(&scope, base).map_err(|_| "Adopt base")?;
        let is_target = matches!(target, ComposerTarget::Thread) == scenario.ends_with("thread");
        let rejected = is_target && matches!(scenario, "retry-channel" | "expired-change-thread");
        let local = if rejected {
            let frozen = rich_snapshot(target);
            let previous = state.clone();
            let now = super_platinum_core::state::now_secs() / 60 * 60;
            let time = if scenario.starts_with("expired-") {
                now - 3600
            } else {
                now + 7200
            };
            state
                .request_reviewed_schedule(&previous, frozen, time)
                .map_err(|_| "Seed rich intent")?;
            let op = state.begin_operation(&scope).ok_or("Seed operation")?;
            if !state.reject(&scope, &op.id) {
                return Err("Seed rejection".into());
            }
            let independent = local(NEWER, target);
            state.edit(
                &scope,
                Some(
                    codec::encode_composer(&key, &independent.composer)
                        .map_err(|_| "Seed local")?,
                ),
            );
            independent
        } else {
            local(BASE, target)
        };
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
pub(crate) async fn install(mut state: Signal<ShellState>) -> Result<Arc<Binding>, String> {
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
    let binding = Arc::new(Binding {
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
        store::{LocalWriter, sync::Prepare},
        sync::Workflow,
    };
    struct PrivateSeed {
        path: PathBuf,
        account: Arc<Account>,
    }
    impl PrivateSeed {
        fn new(scenario: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "managed-schedule-seed-{}.sqlite",
                LocalWriter::default().id()
            ));
            seed(&path, scenario).unwrap();
            let account = Account::new(Backend::Isolated {
                account: ACCOUNT.into(),
                path: path.clone(),
            });
            Self { path, account }
        }
        fn binding(&self) -> Binding {
            Binding {
                account: self.account.clone(),
                path: self.path.clone(),
                nonce: "a".repeat(32),
                scenario: "held-response-thread".into(),
                phase: "exercise".into(),
                epoch: 1,
                view_id: 1,
                editors: BTreeMap::new(),
                requests: Mutex::new(vec![]),
                routed: AtomicU64::new(0),
                refused: AtomicU64::new(0),
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
    async fn ordinary_seeds_are_exact_supported_acknowledged_channel_and_broadcast_thread() {
        let f = PrivateSeed::new("rich-channel");
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let view = f.account.owner(key(target)).load().await.unwrap();
            assert_eq!(json!(view.local), json!(local(BASE, target)));
            let row = view.stored.unwrap();
            assert_eq!(row.revision, 1);
            assert_eq!(row.state.workflow(), &Workflow::Idle);
            assert!(row.state.consumption().is_none());
            let ack = row.state.acknowledged().unwrap();
            assert_eq!(row.state.desired(), Some(&ack.snapshot));
            assert_eq!(ack.scope.key, key(target));
            assert_eq!(
                codec::decode(&key(target), &ack.snapshot)
                    .unwrap()
                    .composer()
                    .text,
                BASE
            );
            assert_eq!(
                ack.snapshot.destinations[0]
                    .get("broadcast")
                    .and_then(Value::as_bool),
                if target == ComposerTarget::Thread {
                    Some(true)
                } else {
                    None
                }
            );
        }
    }
    #[tokio::test]
    async fn rejected_seeds_retain_raw_intent_separate_newer_local_and_exact_time_class() {
        for scenario in ["retry-channel", "expired-change-thread"] {
            let f = PrivateSeed::new(scenario);
            let target = if scenario.ends_with("thread") {
                ComposerTarget::Thread
            } else {
                ComposerTarget::Channel
            };
            let view = f.account.owner(key(target)).load().await.unwrap();
            assert_eq!(json!(view.local), json!(local(NEWER, target)));
            let row = view.stored.unwrap();
            let (intent, op) = row.state.rejected_schedule().unwrap();
            assert_eq!(intent.snapshot, rich_snapshot(target));
            assert_eq!(op.snapshot.as_ref(), Some(&intent.snapshot));
            assert!(op.known_schedule_check());
            assert!(codec::decode(&key(target), &intent.snapshot).is_err());
            let OperationKind::Schedule { post_at } = op.kind else {
                panic!("Wrong seed kind")
            };
            assert_eq!(post_at % 60, 0);
            assert_eq!(
                post_at > super_platinum_core::state::now_secs(),
                scenario == "retry-channel"
            );
            assert_eq!(
                row.state.desired(),
                Some(
                    &codec::encode_composer(&key(target), &local(NEWER, target).composer).unwrap()
                )
            );
        }
    }
    #[tokio::test]
    async fn live_inventory_preserves_actual_inflight_schedule_and_restart_alone_normalizes_it() {
        let f = PrivateSeed::new("rich-thread");
        let owner = f.account.owner(key(ComposerTarget::Thread));
        let mut queue = checkpoint::Queue::default();
        let editor = queue
            .editor(
                &f.account,
                1,
                owner.scope.key.clone(),
                owner.load().await.unwrap(),
            )
            .unwrap();
        let visit = checkpoint::ScheduleVisit::new();
        let lease = editor
            .prepare_reviewed_schedule(local(BASE, ComposerTarget::Thread), visit.clone())
            .await
            .unwrap();
        let consumed = editor
            .consume_reviewed_schedule(
                lease,
                visit,
                rich_snapshot(ComposerTarget::Thread),
                super_platinum_core::state::now_secs() + 7200,
            )
            .await
            .unwrap();
        let Prepare::Prepared { operation, .. } = f
            .account
            .backend
            .prepare(owner.scope.clone(), consumed.revision)
            .await
            .unwrap()
        else {
            panic!("Schedule not prepared")
        };
        let b = f.binding();
        let before = durable(&b).await.unwrap();
        assert_eq!(
            before["thread"]["stored"]["state"]["workflow"],
            json!({"InFlight":operation})
        );
        assert_eq!(durable(&b).await.unwrap(), before);
        let reopened = Account::new(f.account.backend.clone())
            .owner(owner.scope.key.clone())
            .load()
            .await
            .unwrap()
            .stored
            .unwrap();
        assert_eq!(reopened.state.workflow(), &Workflow::Uncertain(operation));
        assert_eq!(
            reopened.revision,
            before["thread"]["stored"]["revision"].as_u64().unwrap() + 1
        );
    }
}
