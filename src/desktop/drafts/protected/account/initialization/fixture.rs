use super::*;
pub(super) struct Fixture {
    pub account: Arc<Account>,
    pub path: std::path::PathBuf,
    pub _store: durable::test_support::Isolated,
}
impl Fixture {
    pub fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("init-custody-{}-{stamp}-{n}", std::process::id()));
        let id = format!("00000000-0000-4000-8000-{:012}", 9500 + n);
        let store = durable::test_support::Isolated::create(id.clone(), path.clone()).unwrap();
        Self {
            account: Account::isolated(id, store.client()),
            path,
            _store: store,
        }
    }
    pub fn status(&self) -> Status {
        self.account.initialization.state.lock().unwrap().status
    }
    pub fn task_id(&self) -> tokio::task::Id {
        self.account
            .initialization
            .state
            .lock()
            .unwrap()
            .task
            .as_ref()
            .unwrap()
            .id()
    }
    pub fn loaded(&self) -> Arc<durable::Loaded> {
        self.account
            .initialization
            .state
            .lock()
            .unwrap()
            .outcome
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .clone()
    }
}
pub(super) async fn enroll(account: &Arc<Account>) -> Scope {
    let owner = account
        .owner(DraftKey {
            team: "T1".into(),
            channel: "C1".into(),
            thread: None,
        })
        .unwrap();
    let inventory = owner.local_inventory().await.unwrap();
    let client = account.client().unwrap();
    let lease = wait(client.renew_local(inventory.clone())).await.unwrap();
    let serial = lease.initial_serial();
    let inventory = match wait(client.write_local(
        lease,
        serial,
        serial + 1,
        inventory.local().clone(),
        LocalDraft {
            composer: super_platinum_core::ComposerState::with_text("Newer local"),
            attachments: vec![],
        },
    ))
    .await
    .unwrap()
    {
        durable::LocalWriteResult::Applied(value) => value,
        _ => panic!("actual canonical write"),
    };
    wait(client.enroll_local(owner.scope().clone(), inventory))
        .await
        .unwrap();
    owner.scope().clone()
}
