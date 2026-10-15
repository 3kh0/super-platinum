//! Native blocking I/O boundary. Fixture storage can never fall through to an account.
use super::*;
use super_platinum_core::drafts::store;
use super_platinum_core::drafts::store::DraftStore;

#[derive(Clone)]
pub(super) enum Backend {
    Account(String),
    Isolated {
        account: String,
        path: std::path::PathBuf,
    },
}
pub(super) enum LocalWrite {
    Account(store::LocalWrite),
    Isolated(tokio::sync::oneshot::Receiver<Result<bool, Fault>>),
}
impl LocalWrite {
    pub async fn finish(self) -> Result<(), Fault> {
        let saved = match self {
            Self::Account(receipt) => tokio::task::spawn_blocking(move || receipt.wait())
                .await
                .map_err(|_| Fault::Storage)?
                .map_err(|_| Fault::Storage)?,
            Self::Isolated(result) => result.await.map_err(|_| Fault::Storage)??,
        };
        if saved { Ok(()) } else { Err(Fault::Stale) }
    }
}

impl Backend {
    /// Production enqueue is memory-only and synchronous so a detached editor
    /// cannot submit older typing after a new account visit's UI write.
    pub fn queue_local(
        &self,
        key: DraftKey,
        expected: LocalDraft,
        local: LocalDraft,
        writer: store::LocalWriter,
        revision: u64,
    ) -> LocalWrite {
        let scope = Scope {
            account_id: self.account().into(),
            key,
        };
        match self {
            Self::Account(_) => LocalWrite::Account(store::queue_owned_local_if_unchanged(
                scope, expected, local, writer, revision,
            )),
            // This backend is available only in deterministic storage tests,
            // never in a UI runtime or a production account.
            Self::Isolated { path, .. } => isolated_queue(path.clone(), move |path| {
                DraftStore::open(path)
                    .and_then(|mut store| {
                        store.save_owned_local_if_unchanged(
                            &scope, &expected, &local, &writer, revision,
                        )
                    })
                    .map_err(|_| Fault::Storage)
            }),
        }
    }

    pub fn account(&self) -> &str {
        match self {
            Self::Account(account) => account,
            Self::Isolated { account, .. } => account,
        }
    }
    async fn run<T: Send + 'static>(
        &self,
        action: impl FnOnce(Self) -> Result<T, super_platinum_core::error::AppError> + Send + 'static,
    ) -> Result<T, Fault> {
        let backend = self.clone();
        tokio::task::spawn_blocking(move || action(backend))
            .await
            .map_err(|_| Fault::Storage)?
            .map_err(|_| Fault::Storage)
    }
    pub async fn load(&self, mode: LoadMode) -> Result<Loaded, Fault> {
        self.run(move |backend| match backend {
            Self::Account(account) => store::load_synced(&account, mode),
            Self::Isolated { account, path } => DraftStore::open(path)?.load_synced(&account, mode),
        })
        .await
    }
    pub async fn commit(
        &self,
        scope: Scope,
        expected: Option<u64>,
        local: LocalDraft,
        state: DraftSync,
    ) -> Result<Commit, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::commit_synced(scope, expected, local, state, None),
            Self::Isolated { path, .. } => {
                DraftStore::open(path)?.commit_synced(&scope, expected, &local, &state, None)
            }
        })
        .await
    }
    pub async fn prepare(&self, scope: Scope, expected: u64) -> Result<Prepare, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::prepare_synced(scope, expected),
            Self::Isolated { path, .. } => DraftStore::open(path)?.prepare_synced(&scope, expected),
        })
        .await
    }
    pub async fn release(
        &self,
        scope: Scope,
        expected: u64,
    ) -> Result<store::sync::Release, Fault> {
        self.run(move |backend| match backend {
            Self::Account(_) => store::release_synced(scope, expected),
            Self::Isolated { path, .. } => DraftStore::open(path)?.release_synced(&scope, expected),
        })
        .await
    }

    pub async fn preserve_alternate(
        &self,
        key: DraftKey,
        local: LocalDraft,
        writer: store::LocalWriter,
        revision: u64,
    ) -> Result<(), Fault> {
        let scope = Scope {
            account_id: self.account().into(),
            key,
        };
        self.run(move |backend| match backend {
            Self::Account(_) => store::save_owned_local_alternate(scope, local, writer, revision),
            Self::Isolated { path, .. } => DraftStore::open(path)?
                .save_owned_local_alternate(&scope, &local, &writer, revision),
        })
        .await
    }
}

// Fixture and test path writes retain submission order without blocking the UI.
fn isolated_queue(
    path: std::path::PathBuf,
    action: impl FnOnce(std::path::PathBuf) -> Result<bool, Fault> + Send + 'static,
) -> LocalWrite {
    static TAILS: OnceLock<
        Mutex<BTreeMap<std::path::PathBuf, tokio::sync::oneshot::Receiver<()>>>,
    > = OnceLock::new();
    let (complete, next) = tokio::sync::oneshot::channel();
    let previous = TAILS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap()
        .insert(path.clone(), next);
    let (reply, result) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        if let Some(previous) = previous {
            let _ = previous.await;
        }
        let outcome = tokio::task::spawn_blocking(move || action(path))
            .await
            .map_err(|_| Fault::Storage)
            .and_then(|value| value);
        let _ = reply.send(outcome);
        let _ = complete.send(());
    });
    LocalWrite::Isolated(result)
}

#[path = "settlement/storage.rs"]
mod settlement;

#[path = "remote_review/storage.rs"]
mod remote_review;

#[path = "rejected_schedule_storage.rs"]
mod rejected_schedule;
#[path = "schedule_storage.rs"]
mod schedule;
