//! Blocking journal access; fixtures never fall through to the user's store.
use super::*;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use super_platinum_core::error::AppError;

/// The original read result/deadline stays in root custody until its exact local
/// write succeeds. Retrying this value has no transport or mutation capability.
#[derive(Clone)]
pub(super) enum Local {
    Edit {
        expected: model::Record,
        note: Vec<serde_json::Value>,
        time: i64,
        at: u64,
    },
    Resolve {
        expected: model::Record,
        raw: serde_json::Value,
        at: u64,
    },
    Preflight {
        expected: model::Record,
        raw: serde_json::Value,
        at: u64,
    },
    Review {
        expected: model::Record,
        raw: serde_json::Value,
        at: u64,
    },
    Cooldown {
        expected: model::Record,
        seconds: u64,
        at: u64,
    },
}
impl Local {
    /// Malformed network evidence is a failed read, not a failed disk write.
    /// Only a receipt that can satisfy its operation's immutable input checks
    /// enters the storage-only retry slot. Journal still owns transactional CAS.
    pub(super) fn admissible(&self) -> bool {
        match self {
            Self::Edit {
                expected,
                note,
                time,
                at,
            } => {
                matches!(
                    expected.phase(),
                    model::Phase::Ready | model::Phase::Rejected | model::Phase::Confirmed
                ) && (*time != 0 || expected.reviewed().is_some())
                    && (*time == 0
                        || i64::try_from(*at).ok().is_some_and(|now| {
                            super_platinum_core::scheduled::validate_time(*time, now).is_ok()
                        }))
                    && model::append_reference(note, expected.intent().source()).is_ok()
            }
            Self::Cooldown { expected, .. } => expected.phase() != model::Phase::Deleted,
            Self::Review { expected, raw, at } => {
                matches!(
                    expected.phase(),
                    model::Phase::Unknown | model::Phase::Conflict
                ) && *at >= expected.retry_at()
                    && model::valid_readback(expected, raw)
            }
            Self::Preflight { expected, raw, .. } => {
                matches!(
                    expected.phase(),
                    model::Phase::Ready | model::Phase::Rejected | model::Phase::Confirmed
                ) && expected.reviewed().is_some()
                    && model::valid_readback(expected, raw)
            }
            Self::Resolve { expected, raw, at } => {
                expected.phase() == model::Phase::Conflict
                    && *at >= expected.retry_at()
                    && expected.observed().and_then(|v| v.get("draft")) == raw.get("draft")
                    && model::valid_readback(expected, raw)
                    && raw["draft"]["is_deleted"] == false
                    && raw["draft"]["is_sent"] == false
            }
        }
    }

    fn save(self, journal: &mut model::store::Journal) -> Result<model::Record, AppError> {
        match self {
            Self::Edit {
                expected,
                note,
                time,
                at,
            } => journal.retain_edit(&expected, note, time, at),
            Self::Resolve { expected, raw, at } => journal.resolve_conflict(&expected, &raw, at),
            Self::Preflight { expected, raw, at } => journal.observe_preflight(&expected, &raw, at),
            Self::Review { expected, raw, at } => journal.review(&expected, &raw, at),
            Self::Cooldown {
                expected,
                seconds,
                at,
            } => journal.defer_read(&expected, seconds, at),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Storage {
    pub account: String,
    pub owner: model::Owner,
    path: Option<PathBuf>,
}
impl Storage {
    #[cfg(test)]
    pub(super) fn isolated(account: String, owner: model::Owner, path: PathBuf) -> Self {
        Self {
            account,
            owner,
            path: Some(path),
        }
    }
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let account = shell.core.active_account.clone()?;
        let team = shell.core.active_team.clone()?;
        let user = shell
            .core
            .session
            .as_ref()?
            .workspaces
            .get(&team)?
            .user_id
            .clone();
        let path = if crate::fixture::is_fixture() {
            Some(fixture::journal_path(shell)?)
        } else {
            None
        };
        Some(Self {
            account,
            owner: model::Owner { team, user },
            path,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        Self::capture(shell).as_ref() == Some(self)
    }
    pub async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(&mut model::store::Journal) -> Result<T, AppError> + Send + 'static,
    ) -> Result<T, String> {
        let storage = self.clone();
        tokio::task::spawn_blocking(move || {
            if fixture::enabled() { fixture::validate_storage(&storage.account, &storage.owner, storage.path.as_deref()).map_err(|_| "Private scheduled-forward journal refused".to_owned())?; }
            let mut journal = match storage.path {
                Some(path) => model::store::Journal::open(path, &storage.account),
                None => model::store::Journal::open_default(&storage.account),
            }.map_err(|_| "Scheduled forwarding records could not be opened. Your input is retained.".to_owned())?;
            action(&mut journal).map_err(|_| "The scheduled forwarding record could not be saved. Retry saving the record; nothing will be resent.".to_owned())
        }).await.map_err(|_| "Scheduled forwarding storage did not finish. Your input is retained.".to_owned())?
    }
    pub async fn finish(
        &self,
        completion: Arc<Mutex<model::Completion>>,
    ) -> Result<model::Record, String> {
        self.run(move |journal| {
            let completion = completion
                .lock()
                .map_err(|_| std::io::Error::other("Completion custody unavailable"))?;
            journal.finish(&completion)
        })
        .await
    }
    pub async fn unsent(&self, proof: Arc<Mutex<model::Unsent>>) -> Result<model::Record, String> {
        self.run(move |journal| {
            let proof = proof
                .lock()
                .map_err(|_| std::io::Error::other("Unsent custody unavailable"))?;
            journal.save_unsent(&proof)
        })
        .await
    }
    pub async fn local(&self, receipt: Local) -> Result<model::Record, String> {
        self.run(move |journal| receipt.save(journal)).await
    }
    pub async fn page(&self, after: Option<String>) -> Result<Vec<model::Record>, String> {
        let owner = self.owner.clone();
        self.run(move |journal| journal.page(&owner, after.as_deref()))
            .await
    }
}

/// Existing scheduled writers consult the same durable unknown-state fence after
/// restart. This is a read, never a reserve or a recovery dispatch.
pub(crate) async fn allow_existing(
    state: Signal<ShellState>,
    client_id: &str,
) -> Result<(), super_platinum_core::slack::Error> {
    if client_id.is_empty() {
        return Ok(());
    }
    // A disconnected fixture without the explicit private journal capability
    // cannot create scheduled forwards. It must never touch the user's store.
    if (crate::fixture::is_fixture() || cfg!(test))
        && fixture::journal_path(&state.peek()).is_none()
    {
        return Ok(());
    }
    let storage = Storage::capture(&state.peek()).ok_or_else(|| {
        super_platinum_core::slack::Error::Api("scheduled_forward_owner_unavailable".into())
    })?;
    let id = client_id.to_owned();
    let record = storage
        .run(move |journal| journal.get(&id))
        .await
        .map_err(|_| {
            super_platinum_core::slack::Error::Api("scheduled_forward_storage_unavailable".into())
        })?;
    if !storage.current(&state.peek())
        || record.is_some_and(|record| {
            record.intent().owner() != &storage.owner
                || matches!(
                    record.phase(),
                    model::Phase::Unknown | model::Phase::Conflict
                )
        })
    {
        return Err(super_platinum_core::slack::Error::Api(
            "scheduled_forward_review_required".into(),
        ));
    }
    Ok(())
}
