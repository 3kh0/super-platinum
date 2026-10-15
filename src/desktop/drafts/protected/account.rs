//! Captured account lifetime and the only blocking Reply wait boundary.
use super::*;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct Account {
    id: String,
    client: durable::Client,
    initialization: tokio::sync::Mutex<Initialization>,
    ready: AtomicBool,
    gates: Mutex<BTreeMap<DraftKey, Arc<tokio::sync::Mutex<()>>>>,
    gaps: Mutex<(u64, BTreeMap<String, PendingGap>)>,
    pub(super) journal: Mutex<super::journal::Journal>,
    pub(super) editors: Mutex<super::retention::Editors>,
    pub(super) operations: Mutex<super::operation::Records>,
}

#[cfg(test)]
mod editor_open_initialization_tests {
    use super::*;
    #[tokio::test]
    async fn opening_is_retained_while_first_initialization_is_held() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("c3-init-{}-{stamp}", std::process::id()));
        let id = "00000000-0000-4000-8000-000000000394";
        let store = durable::test_support::Isolated::create(id.into(), path).unwrap();
        let account = Account::isolated(id.into(), store.client());
        let guard = account.initialization.lock().await;
        assert!(matches!(*guard, Initialization::New));
        let owner = account
            .owner(DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            })
            .unwrap();
        let opened = super::super::editor_open::Request {
            owner,
            expected: None,
        }
        .begin()
        .unwrap_or_else(|_| panic!("reserve before initialization"));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if account.editor_inventory().unwrap()[0].phase
                    == Some(super::super::editor_open::Phase::Initializing)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(matches!(account.client(), Err(durable::Error::Pending)));
        assert!(account.retained_editors().unwrap().is_empty());
        assert_eq!(account.retained_openings().unwrap().len(), 1);
        assert_eq!(
            account.editor_inventory().unwrap()[0].terminal,
            Some(super::super::editor_open::Terminal::Running)
        );
        drop(guard);
        tokio::time::timeout(std::time::Duration::from_secs(5), opened.ready())
            .await
            .unwrap();
        // No managed row exists: the actual read fails after initialization;
        // the completed reservation must still survive rather than evaporate.
        assert!(account.client().is_ok());
        assert!(opened.result().is_err());
        assert_eq!(account.editor_inventory().unwrap().len(), 1);
    }
}
struct PendingGap {
    generation: u64,
    stream: durable::Stream,
    reason: durable::GapReason,
    running: bool,
}
enum Initialization {
    New,
    Running(tokio::task::JoinHandle<Result<(), durable::Error>>),
    Ready,
}

pub(super) async fn wait<T: Send + 'static>(reply: durable::Reply<T>) -> Result<T, durable::Error> {
    tokio::task::spawn_blocking(move || reply.wait())
        .await
        .map_err(|_| durable::Error::Storage)?
}

impl Account {
    /// Inactive owned factory only: readiness comes from the actual once-only
    /// core load. No caller mode/boolean and no second Account::initialize load.
    #[cfg(test)]
    pub(super) fn from_initialized(value: durable::owned_store::Initialized) -> Arc<Self> {
        Arc::new(Self {
            id: value.account().to_owned(),
            client: value.client(),
            initialization: tokio::sync::Mutex::new(Initialization::Ready),
            ready: AtomicBool::new(true),
            gates: Mutex::default(),
            gaps: Mutex::default(),
            journal: Mutex::default(),
            editors: Mutex::default(),
            operations: Mutex::default(),
        })
    }
    #[cfg(any(test, feature = "protected-native-fixture"))]
    pub(super) fn isolated(id: String, client: durable::Client) -> Arc<Self> {
        Arc::new(Self {
            id,
            client,
            initialization: tokio::sync::Mutex::new(Initialization::New),
            ready: AtomicBool::new(false),
            gates: Mutex::default(),
            gaps: Mutex::default(),
            journal: Mutex::default(),
            editors: Mutex::default(),
            operations: Mutex::default(),
        })
    }
    /// Memory only. This package has no selector callsite. The core Client must
    /// reject missing/wrong-format authority rather than migrate a legacy row.
    pub fn captured(id: &str) -> Result<Arc<Self>, durable::Error> {
        if crate::fixture::is_fixture() || !super_platinum_core::drafts::sync::valid_account_id(id)
        {
            return Err(durable::Error::Unsupported);
        }
        static ACCOUNTS: OnceLock<Mutex<BTreeMap<String, Arc<Account>>>> = OnceLock::new();
        let mut accounts = ACCOUNTS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        Ok(accounts
            .entry(id.to_owned())
            .or_insert_with(|| {
                Arc::new(Self {
                    id: id.to_owned(),
                    client: durable::Client::new(id.to_owned()),
                    initialization: tokio::sync::Mutex::new(Initialization::New),
                    ready: AtomicBool::new(false),
                    gates: Mutex::default(),
                    gaps: Mutex::default(),
                    journal: Mutex::default(),
                    editors: Mutex::default(),
                    operations: Mutex::default(),
                })
            })
            .clone())
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub(super) fn block_team(&self, stream: &durable::Stream, reason: durable::GapReason) -> u64 {
        let mut gaps = self.gaps.lock().unwrap();
        gaps.0 = gaps.0.checked_add(1).expect("gap generation exhausted");
        let generation = gaps.0;
        gaps.1.insert(
            stream.team().to_owned(),
            PendingGap {
                generation,
                stream: stream.clone(),
                reason,
                running: true,
            },
        );
        generation
    }
    pub(super) fn finish_gap(&self, team: &str, generation: u64, success: bool) {
        let mut gaps = self.gaps.lock().unwrap();
        if gaps
            .1
            .get(team)
            .is_some_and(|gap| gap.generation == generation)
        {
            if success {
                gaps.1.remove(team);
            } else if let Some(gap) = gaps.1.get_mut(team) {
                gap.running = false;
            }
        }
    }
    /// Revoke before enqueueing. Account retains a failed repair after the
    /// originating socket or its renderer task has disappeared.
    pub(super) fn retire_stream(
        self: &Arc<Self>,
        stream: durable::Stream,
        reason: durable::GapReason,
        runtime: Option<tokio::runtime::Handle>,
    ) {
        stream.revoke();
        let generation = self.block_team(&stream, reason);
        let team = stream.team().to_owned();
        let Some(runtime) = runtime else {
            self.finish_gap(&team, generation, false);
            return;
        };
        let account = self.clone();
        let reply = self.client().map(|client| client.mark_gap(stream, reason));
        runtime.spawn(async move {
            let success = match reply {
                Ok(reply) => wait(reply).await.is_ok(),
                Err(_) => false,
            };
            account.finish_gap(&team, generation, success);
        });
    }
    /// Repair remains owned by Account after its source/worker disappears.
    pub fn retry_gap(self: &Arc<Self>, team: &str) -> Result<(), durable::Error> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| durable::Error::Storage)?;
        let (stream, reason, generation) = {
            let mut gaps = self.gaps.lock().map_err(|_| durable::Error::Storage)?;
            let gap = gaps.1.get_mut(team).ok_or(durable::Error::Stale)?;
            if gap.running {
                return Err(durable::Error::Pending);
            }
            gap.running = true;
            (gap.stream.clone(), gap.reason, gap.generation)
        };
        let account = self.clone();
        let team = team.to_owned();
        let reply = self.client().map(|client| client.mark_gap(stream, reason));
        runtime.spawn(async move {
            let success = match reply {
                Ok(reply) => wait(reply).await.is_ok(),
                Err(_) => false,
            };
            account.finish_gap(&team, generation, success);
        });
        Ok(())
    }
    pub(super) fn team_blocked(&self, team: &str) -> bool {
        self.gaps
            .lock()
            .map_or(true, |gaps| gaps.1.contains_key(team))
    }
    pub async fn initialize(self: &Arc<Self>) -> Result<(), durable::Error> {
        let mut state = self.initialization.lock().await;
        if matches!(*state, Initialization::New) {
            // Enqueue once before detaching the wait. Canceling the caller keeps
            // this handle in Account; no second AfterRestart may run beside it.
            let reply = self.client.load(durable::LoadMode::AfterRestart);
            *state =
                Initialization::Running(tokio::spawn(async move { wait(reply).await.map(|_| ()) }));
        }
        if let Initialization::Running(task) = &mut *state {
            let result = task.await.unwrap_or(Err(durable::Error::Storage));
            *state = if result.is_ok() {
                Initialization::Ready
            } else {
                Initialization::New
            };
            result?;
            self.ready.store(true, Ordering::Release);
        }
        Ok(())
    }
    pub async fn inventory(self: &Arc<Self>) -> Result<durable::Loaded, durable::Error> {
        self.initialize().await?;
        wait(self.client.load(durable::LoadMode::Current)).await
    }
    pub(super) fn client(&self) -> Result<&durable::Client, durable::Error> {
        self.ready
            .load(Ordering::Acquire)
            .then_some(&self.client)
            .ok_or(durable::Error::Pending)
    }
    pub fn owner(self: &Arc<Self>, key: DraftKey) -> Result<super::owner::Owner, durable::Error> {
        if key.team.is_empty() || key.channel.is_empty() {
            return Err(durable::Error::Unsupported);
        }
        let gate = self
            .gates
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .entry(key.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        Ok(super::owner::Owner {
            scope: Scope {
                account_id: self.id.clone(),
                key,
            },
            account: self.clone(),
            gate,
        })
    }
}
