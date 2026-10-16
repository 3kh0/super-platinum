use super::super::super::super::account::wait;
use super::*;
use durable::test_support::{FaultPoint, Isolated};
use super_platinum_core::{
    ComposerState,
    config::{Session, WorkspaceSession},
    slack::{
        Transport,
        realtime::{ConnectParams, flannel_url},
    },
};

pub(super) struct Fixture {
    pub store: Isolated,
    pub account: Arc<Account>,
    pub shell: ShellState,
    pub socket: Arc<observation::Source>,
    _worker: observation::WorkerSource,
}
impl Fixture {
    pub async fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "source-owned-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let id = "00000000-0000-4000-8000-000000000397";
        let store = Isolated::create(id.into(), path).unwrap();
        let account = Account::isolated(id.into(), store.client());
        account.initialize().await.unwrap();
        let owner = account
            .owner(DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: None,
            })
            .unwrap();
        let inventory = owner.local_inventory().await.unwrap();
        let lease = wait(account.client().unwrap().renew_local(inventory.clone()))
            .await
            .unwrap();
        let serial = lease.initial_serial();
        let inventory = match wait(account.client().unwrap().write_local(
            lease,
            serial,
            serial + 1,
            inventory.local().clone(),
            LocalDraft {
                composer: ComposerState::with_text("Original"),
                attachments: vec![],
            },
        ))
        .await
        .unwrap()
        {
            durable::LocalWriteResult::Applied(value) => value,
            _ => panic!("canonical seed"),
        };
        wait(
            account
                .client()
                .unwrap()
                .enroll_local(owner.scope().clone(), inventory),
        )
        .await
        .unwrap();
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some(id.into());
        shell.core.account_epoch = 7;
        shell.core.transport = Some(Arc::new(Transport::new("fixture-only-cookie").unwrap()));
        shell.core.session = Some(Session {
            d_cookie: "fixture-only-cookie".into(),
            workspaces: [(
                "T1".into(),
                WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "fixture-only-token".into(),
                },
            )]
            .into(),
        });
        let mut worker = observation::WorkerSource::capture(
            &mut shell,
            ConnectParams {
                team: "T1".into(),
                ws_url: flannel_url("fixture-only-token", "T1"),
                d_cookie: "fixture-only-cookie".into(),
                user_agent: "fixture-only-agent".into(),
            },
        )
        .unwrap();
        worker.take_connect_params(&shell).unwrap();
        assert!(worker.can_accept(&shell, "T1", 9));
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 9;
        let socket = Arc::new(worker.accept(&mut shell, "T1", 9).unwrap());
        Self {
            store,
            account,
            shell,
            socket,
            _worker: worker,
        }
    }
    pub fn reserve(&self) -> Arc<Job> {
        Job::reserve(
            &self.account,
            self.socket.clone(),
            9,
            "T1",
            tokio::runtime::Handle::current(),
        )
        .unwrap()
    }
    pub async fn read(&self, job: &Arc<Job>) -> durable::StreamState {
        let read = ReplyJob::new(self.account.client().unwrap().stream_state("T1".into()));
        {
            let mut state = job.state.lock().unwrap();
            state.read = Some(read.clone());
            state.phase = Phase::Reading;
        }
        read.start(&tokio::runtime::Handle::current(), |_| ())
            .unwrap();
        read.ready().await.unwrap()
    }
    pub fn open(
        &self,
        job: &Arc<Job>,
        expected: durable::StreamState,
    ) -> Arc<ReplyJob<durable::Stream>> {
        let open = ReplyJob::new(
            self.account
                .client()
                .unwrap()
                .open_stream("T1".into(), expected),
        );
        {
            let mut state = job.state.lock().unwrap();
            state.open = Some(open.clone());
            state.phase = Phase::Opening;
        }
        open
    }
    pub async fn issued(&self, job: &Arc<Job>) -> durable::Stream {
        let expected = self.read(job).await;
        let open = self.open(job, expected);
        let retained = job.clone();
        open.start(&tokio::runtime::Handle::current(), move |result| {
            retained.issued(result.unwrap())
        })
        .unwrap();
        open.ready().await.unwrap()
    }
    pub async fn fault(&self) {
        wait(self.store.fail_next(FaultPoint::GapCommit))
            .await
            .unwrap();
    }
}
pub(super) async fn settled_gap(job: &Arc<Job>) -> Result<(), durable::Error> {
    let reply = job.gap.attempts().last().unwrap().clone();
    tokio::time::timeout(std::time::Duration::from_secs(5), reply.ready())
        .await
        .unwrap()
}
