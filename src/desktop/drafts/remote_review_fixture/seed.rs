//! Fresh private SQLite only; every admitted snapshot uses the supported codec.
use super::*;
use super_platinum_core::drafts::{
    store::{DraftStore, LocalWriter},
    sync::{DraftObservation, ObservationKind},
};

fn remote(target: ComposerTarget, text: &str, stamp: &str) -> Result<RemoteDraft, ()> {
    Ok(RemoteDraft {
        scope: Scope {
            account_id: ACCOUNT.into(),
            key: key(target),
        },
        id: if matches!(target, ComposerTarget::Thread) {
            "DrReviewThread"
        } else {
            "DrReviewChannel"
        }
        .into(),
        client_msg_id: if matches!(target, ComposerTarget::Thread) {
            "00000000-0000-4000-8000-000000000083"
        } else {
            "00000000-0000-4000-8000-000000000082"
        }
        .into(),
        last_updated_ts: stamp.into(),
        snapshot: codec::encode_composer(&key(target), &local(text, target).composer)
            .map_err(|_| ())?,
    })
}
pub(super) fn raw(target: ComposerTarget) -> Result<Value, ()> {
    let r = remote(target, AFTER, "12.000001")?;
    Ok(
        json!({"ok":true,"draft":{"id":r.id,"client_msg_id":r.client_msg_id,"last_updated_ts":r.last_updated_ts,
        "team_id":"T1","user_id":"U00000001","is_deleted":false,"is_sent":false,"date_scheduled":0,
        "blocks":r.snapshot.blocks,"file_ids":r.snapshot.file_ids,"destinations":r.snapshot.destinations}}),
    )
}
fn seed(path: &std::path::Path) -> Result<(), String> {
    let mut store = DraftStore::open(path).map_err(|_| "Open private store")?;
    for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
        let base = remote(target, "Original acknowledged draft", "10.000001")
            .map_err(|_| "Base snapshot")?;
        let scope = base.scope.clone();
        let current = local(BEFORE, target);
        let mut state = DraftSync::adopt_remote(&scope, base).map_err(|_| "Adopt isolated base")?;
        state.edit(
            &scope,
            Some(
                codec::encode_composer(&scope.key, &current.composer)
                    .map_err(|_| "Local snapshot")?,
            ),
        );
        let previous = remote(target, OLDER, "11.000001").map_err(|_| "Previous snapshot")?;
        state
            .observe_event(&DraftObservation {
                scope: scope.clone(),
                draft_id: previous.id.clone(),
                client_msg_id: previous.client_msg_id.clone(),
                event_ts: "11.000002".into(),
                kind: ObservationKind::Active(previous),
            })
            .map_err(|_| "Conflict seed")?;
        store
            .save([(scope.key.clone(), current.clone())])
            .map_err(|_| "Save seed")?;
        if !matches!(
            store
                .commit_synced(&scope, None, &current, &state, None)
                .map_err(|_| "Commit seed")?,
            Commit::Applied(_)
        ) {
            return Err("Seed not committed".into());
        }
        for (text, serial) in [(BLOCKED, 7), (ALTERNATE, 11)] {
            store
                .save_owned_local_alternate(
                    &scope,
                    &local(text, target),
                    &LocalWriter::default(),
                    serial,
                )
                .map_err(|_| "Seed alternate")?;
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
    let binding = Arc::new(Binding {
        account,
        path,
        nonce,
        scenario,
        phase,
        epoch,
        editors,
        requests: Mutex::new(vec![]),
        routed: AtomicU64::new(0),
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
    fn all_sample_text_is_supported_plain_unicode_for_both_destinations() {
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            for text in [BEFORE, AFTER, NEWER, ALTERNATE, BLOCKED] {
                let snapshot =
                    codec::encode_composer(&key(target), &local(text, target).composer).unwrap();
                let decoded = codec::decode(&key(target), &snapshot).unwrap();
                assert_eq!(decoded.composer().text, text);
                assert_eq!(
                    decoded.composer().reply_broadcast,
                    matches!(target, ComposerTarget::Thread)
                );
                assert!(
                    text.contains('\u{1f680}')
                        && text.contains('\u{65e5}')
                        && text.contains('\u{301}')
                );
            }
        }
    }
    #[test]
    fn seed_has_full_distinct_inventory_and_admits_actual_positive_read() {
        // Isolated temp data only, no platform account/config/keychain access.
        let path = std::env::temp_dir().join(format!(
            "review-seed-{}.sqlite",
            LocalWriter::default().id()
        ));
        seed(&path).unwrap();
        let mut store = DraftStore::open(&path).unwrap();
        for target in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let review = store
                .read_local_review(&Scope {
                    account_id: ACCOUNT.into(),
                    key: key(target),
                })
                .unwrap();
            assert_eq!(review.local.composer.text, BEFORE);
            assert_eq!(
                review.blocked.as_ref().unwrap().draft.composer.text,
                BLOCKED
            );
            assert_eq!(review.alternates.len(), 2);
            assert!(matches!(
                review.stored.as_ref().unwrap().state.workflow(),
                super_platinum_core::drafts::sync::Workflow::Conflict { pending: None, .. }
            ));
            RemoteReview::prepare(review, raw(target).unwrap(), "U00000001").unwrap();
        }
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
}
