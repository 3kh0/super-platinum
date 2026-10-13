//! Explicitly injected offline SQLite storage; never resolves a real account.
use super::*;
use std::path::PathBuf;

pub(super) fn account(id: &str) -> Result<Arc<Account>, Fault> {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    let allowed = match name.as_str() {
        "managed-draft-recovery" | "managed-draft-recovery-narrow" => {
            std::env::var_os("SUPER_PLATINUM_MANAGED_DRAFT_RECOVERY_CHECK").is_some()
        }
        "rejected-send" | "rejected-send-narrow" => {
            std::env::var("SUPER_PLATINUM_REJECTED_SEND_CHECK").as_deref() == Ok("1")
        }
        _ => false,
    };
    if !allowed || id != "00000000-0000-4000-8000-000000000099" {
        return Err(Fault::Unsupported);
    }
    let path = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_DRAFT_SETTLEMENT_DB").ok_or(Fault::Unsupported)?,
    );
    if !path.is_absolute()
        || path.file_name().and_then(|name| name.to_str()) != Some("draft-settlement.sqlite")
    {
        return Err(Fault::Unsupported);
    }
    static ACCOUNTS: OnceLock<Mutex<BTreeMap<PathBuf, Arc<Account>>>> = OnceLock::new();
    let mut accounts = ACCOUNTS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| Fault::Storage)?;
    Ok(accounts
        .entry(path.clone())
        .or_insert_with(|| {
            Account::new(Backend::Isolated {
                account: id.into(),
                path,
            })
        })
        .clone())
}

pub(crate) async fn seed(
    mut state: dioxus::prelude::Signal<crate::state::ShellState>,
) -> Result<(), Fault> {
    use dioxus::prelude::*;
    use super_platinum_core::drafts::store::{DraftStore, LocalWriter};
    let (id, epoch, records) = {
        let shell = state.peek();
        if shell.core.session.is_some() || shell.core.transport.is_some() {
            return Err(Fault::Unsupported);
        }
        (
            shell
                .core
                .active_account
                .clone()
                .ok_or(Fault::Unsupported)?,
            shell.core.account_epoch,
            shell
                .managed_drafts
                .entries
                .iter()
                .map(|((_, key), view)| (key.clone(), view.clone()))
                .collect::<Vec<_>>(),
        )
    };
    let owner = account(&id)?;
    let Backend::Isolated { path, .. } = &owner.backend else {
        return Err(Fault::Unsupported);
    };
    let path = path.clone();
    let seed_id = id.clone();
    let loaded = tokio::task::spawn_blocking(move || -> Result<Loaded, Fault> {
        // Runner creates a fresh private directory. Existing content is loaded,
        // never overwritten, so a second process can prove restart persistence.
        let fresh = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
            Err(_) => return Err(Fault::Storage),
        };
        let mut store = DraftStore::open(path).map_err(|_| Fault::Storage)?;
        if fresh {
            store
                .save(
                    records
                        .iter()
                        .map(|(key, view)| (key.clone(), view.local.clone())),
                )
                .map_err(|_| Fault::Storage)?;
            for (key, view) in &records {
                let scope = Scope {
                    account_id: seed_id.clone(),
                    key: key.clone(),
                };
                if let Some(stored) = &view.stored {
                    let mut expected = None;
                    for _ in 0..stored.revision {
                        match store
                            .commit_synced(&scope, expected, &view.local, &stored.state, None)
                            .map_err(|_| Fault::Storage)?
                        {
                            Commit::Applied(row) => expected = Some(row.revision),
                            Commit::Stale(_) => return Err(Fault::Stale),
                        }
                    }
                }
                if let Some(blocked) = &view.blocked {
                    store
                        .save_local_alternate(&scope, blocked.draft.clone())
                        .map_err(|_| Fault::Storage)?;
                    let extra = LocalDraft {
                        composer: super_platinum_core::ComposerState::with_text(
                            "Saved editor version was preserved.",
                        ),
                        attachments: vec![super_platinum_core::domain::ComposerAttachment {
                            id: 501,
                            path: "/tmp/offline-reviewed-attachment.txt".into(),
                            name: "reviewed-attachment.txt".into(),
                            bytes: 17,
                            uploading: false,
                            upload_started: None,
                            upload_cancel: None,
                            upload_progress: None,
                            preview_path: None,
                        }],
                    };
                    store
                        .save_owned_local_alternate(&scope, &extra, &LocalWriter::default(), 3)
                        .map_err(|_| Fault::Storage)?;
                }
            }
        }
        store
            .load_synced(&seed_id, LoadMode::Current)
            .map_err(|_| Fault::Storage)
    })
    .await
    .map_err(|_| Fault::Storage)??;
    let mut shell = state.write();
    if shell.core.active_account.as_ref() != Some(&id) || shell.core.account_epoch != epoch {
        return Err(Fault::Stale);
    }
    let Loaded {
        local,
        synced,
        mut blocked_local,
        uncommitted_local: _,
        local_alternates,
    } = loaded;
    let locals: BTreeMap<_, _> = local.into_iter().collect();
    shell.managed_drafts = session::Session {
        account: Some(id.clone()),
        epoch,
        ready: true,
        ..Default::default()
    };
    for (key, stored) in synced {
        let local = locals.get(&key).cloned().unwrap_or_default();
        let blocked = blocked_local.remove(&key);
        shell.managed_drafts.apply_view(
            &id,
            epoch,
            key,
            View {
                local,
                stored: Some(stored),
                blocked,
                uncommitted: None,
            },
        );
    }
    shell.managed_drafts.local_alternates = local_alternates
        .into_iter()
        .map(|(key, versions)| ((id.clone(), key), versions))
        .collect();
    for (key, local) in locals {
        shell.core.drafts.items.insert(key, local);
    }
    Ok(())
}

pub(crate) async fn verify() -> Result<serde_json::Value, Fault> {
    let account = account("00000000-0000-4000-8000-000000000099")?;
    let local_key = DraftKey {
        team: "T1".into(),
        channel: "C1".into(),
        thread: None,
    };
    let local = account.owner(local_key).review_local().await?;
    let frozen = account
        .owner(DraftKey {
            team: "T1".into(),
            channel: "C2".into(),
            thread: None,
        })
        .review_local()
        .await?;
    let local_selected = local.local.composer.text == "Alternate local text was preserved.";
    let reviewed_removed = local.blocked.is_none() && local.alternates.is_empty();
    let remote_preserved=local.stored.as_ref().is_some_and(|row|matches!(row.state.workflow(),super_platinum_core::drafts::sync::Workflow::Conflict {remote:Some(remote),..} if remote.id=="DrRecoveryFixture"));
    let frozen_preserved = frozen.local.empty()
        && frozen.stored.as_ref().is_some_and(|row| {
            row.state.consumption().is_some()
                && matches!(
                    row.state.workflow(),
                    super_platinum_core::drafts::sync::Workflow::Uncertain(_)
                )
        });
    Ok(
        serde_json::json!({"ok":local_selected&&reviewed_removed&&remote_preserved&&frozen_preserved,"selectedLocal":local.local.composer.text,"reviewedRemoved":reviewed_removed,"remotePreserved":remote_preserved,"frozenPreserved":frozen_preserved}),
    )
}
