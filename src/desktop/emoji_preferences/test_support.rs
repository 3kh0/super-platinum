//! Deterministic isolated executor: polls the actual writer futures, never a
//! mirror state machine. Synthetic replies cannot reach Native or the network.
use super::*;
use crate::media::MediaRegistry;
use boundary::{Task, Work};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    task::{Context as PollContext, Poll, Wake, Waker},
};
use super_platinum_core::{
    config::Session,
    slack::{Error, PreparedRequest, SlackClient, Transport, client::RequestBody},
};
use tokio::sync::oneshot;

pub(super) struct Request {
    pub at: Duration,
    pub prepared: PreparedRequest,
    response: Option<oneshot::Sender<Result<Value, Error>>>,
}
struct Inner {
    origin: Instant,
    elapsed: Cell<Duration>,
    tasks: RefCell<Vec<Task>>,
    hold: RefCell<VecDeque<Work>>,
    blocked: RefCell<Vec<(Work, Rc<Cell<bool>>)>>,
    pub requests: RefCell<Vec<Request>>,
    pub failures: RefCell<Vec<String>>,
    pub persists: Cell<usize>,
}
#[derive(Clone)]
pub(super) struct Isolated {
    inner: Rc<Inner>,
}
impl Isolated {
    fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                origin: Instant::now(),
                elapsed: Cell::new(Duration::ZERO),
                tasks: RefCell::new(vec![]),
                hold: RefCell::new(VecDeque::new()),
                blocked: RefCell::new(vec![]),
                requests: RefCell::new(vec![]),
                failures: RefCell::new(vec![]),
                persists: Cell::new(0),
            }),
        }
    }
    pub fn pump(&self) {
        struct NoWake;
        impl Wake for NoWake {
            fn wake(self: Arc<Self>) {}
        }
        let waker = Waker::from(Arc::new(NoWake));
        let mut cx = PollContext::from_waker(&waker);
        for _ in 0..64 {
            let tasks = std::mem::take(&mut *self.inner.tasks.borrow_mut());
            for mut task in tasks {
                if task.as_mut().poll(&mut cx).is_pending() {
                    self.inner.tasks.borrow_mut().push(task);
                }
            }
        }
    }
    pub fn advance(&self, ms: u64) {
        self.inner
            .elapsed
            .set(self.inner.elapsed.get() + Duration::from_millis(ms));
        self.pump();
    }
    pub fn hold_next(&self, kind: Work) {
        self.inner.hold.borrow_mut().push_back(kind);
    }
    pub fn release(&self, index: usize) {
        self.inner.blocked.borrow()[index].1.set(true);
        self.pump();
    }
    pub fn blocked(&self) -> usize {
        self.inner.blocked.borrow().len()
    }
    pub fn requests(&self) -> usize {
        self.inner.requests.borrow().len()
    }
    pub fn failures(&self) -> usize {
        self.inner.failures.borrow().len()
    }
    pub fn reply(&self, index: usize, value: Result<Value, Error>) {
        self.inner.requests.borrow_mut()[index]
            .response
            .take()
            .unwrap()
            .send(value)
            .unwrap();
        self.pump();
    }
    pub fn at(&self, index: usize) -> Duration {
        self.inner.requests.borrow()[index].at
    }
    pub fn form(&self, index: usize) -> Vec<(String, String)> {
        let requests = self.inner.requests.borrow();
        let RequestBody::Form(fields) = &requests[index].prepared.body else {
            panic!("form required")
        };
        fields.clone()
    }
    pub fn wire(&self, index: usize) -> String {
        self.form(index)
            .into_iter()
            .find(|(k, _)| k == "value")
            .unwrap()
            .1
    }
    pub fn sent(&self, index: usize) -> Value {
        serde_json::from_str(&self.wire(index)).unwrap()
    }
}
impl Boundary for Isolated {
    fn now(&self) -> Instant {
        self.inner.origin + self.inner.elapsed.get()
    }
    fn now_ms(&self) -> f64 {
        1_700_000_000_000.0 + self.inner.elapsed.get().as_millis() as f64
    }
    fn is_fixture(&self) -> bool {
        false
    }
    fn spawn(&self, task: Task) {
        self.inner.tasks.borrow_mut().push(task);
    }
    async fn sleep(&self, wait: Duration) {
        let deadline = self.now() + wait;
        std::future::poll_fn(|_| {
            if self.now() >= deadline {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
    async fn work<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
        &self,
        kind: Work,
        work: F,
    ) -> Result<T, ()> {
        let hold = self.inner.hold.borrow().front() == Some(&kind);
        if hold {
            self.inner.hold.borrow_mut().pop_front();
            let permit = Rc::new(Cell::new(false));
            self.inner.blocked.borrow_mut().push((kind, permit.clone()));
            std::future::poll_fn(|_| {
                if permit.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
        }
        Ok(work())
    }
    async fn execute(&self, _: Arc<Owner>, request: PreparedRequest) -> Result<Value, Error> {
        let (tx, rx) = oneshot::channel();
        self.inner.requests.borrow_mut().push(Request {
            at: self.inner.elapsed.get(),
            prepared: request,
            response: Some(tx),
        });
        rx.await
            .unwrap_or_else(|_| Err(Error::Transport("isolated response lost".into())))
    }
    fn persist(&self, _: Signal<ShellState>, _: &str) {
        self.inner.persists.set(self.inner.persists.get() + 1);
    }
    fn failure(&self, _: Signal<ShellState>, error: Error, _: &'static str) {
        self.inner.failures.borrow_mut().push(
            match error {
                Error::RateLimited { .. } => "rate_limit",
                Error::Transport(_) => "unknown",
                _ => "failure",
            }
            .into(),
        );
    }
}

fn shell() -> ShellState {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    shell.core.active_account = Some("synthetic-account".into());
    shell.core.account_epoch = 7;
    shell.core.active_team = Some("T1".into());
    shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    shell.core.client = SlackClient::default();
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: "synthetic-cookie".into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                user_id: "U1".into(),
                enterprise_id: None,
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "synthetic-token".into(),
            },
        )]
        .into(),
    });
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences
        .replace(&Default::default());
    let mut other = shell.core.workspaces["T1"].clone();
    other.team_id = "OTHER".into();
    shell.core.workspaces.insert("OTHER".into(), other);
    shell
}
pub(super) struct Harness {
    pub boundary: Isolated,
    pub state: Signal<ShellState>,
    _dom: VirtualDom,
}
impl Harness {
    pub fn new() -> Self {
        let slot = Rc::new(RefCell::new(None));
        let capture = slot.clone();
        let mut dom = VirtualDom::new_with_props(
            |slot: Rc<RefCell<Option<Signal<ShellState>>>>| {
                let state = use_signal(shell);
                *slot.borrow_mut() = Some(state);
                rsx! {div{}}
            },
            capture,
        );
        dom.rebuild_in_place();
        let state = slot.borrow().unwrap();
        Self {
            boundary: Isolated::new(),
            state,
            _dom: dom,
        }
    }
    pub fn record(&self, id: &str, query: &str) {
        let future = writer::record_with(
            self.boundary.clone(),
            self.state,
            Usage {
                id: id.into(),
                query: query.into(),
            },
        );
        self.boundary.spawn(Box::pin(future));
    }
    pub fn remote(&mut self, wire: &str) {
        self.state
            .write()
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .emoji_preferences
            .apply_change("frecency", Value::String(wire.into()));
    }
    pub fn value(&self) -> Option<Value> {
        self.state.peek().core.workspaces["T1"]
            .emoji_preferences
            .frecency()
            .cloned()
    }
    pub fn history(&self) -> Value {
        serde_json::from_str(self.value().unwrap().as_str().unwrap()).unwrap()
    }
    pub fn unloaded(&mut self) {
        self.state
            .write()
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .emoji_preferences = Default::default();
    }
    pub fn rotate(&mut self, kind: &str) {
        let mut shell = self.state.write();
        match kind {
            "account" => shell.core.active_account = Some("other-account".into()),
            "epoch" => shell.core.account_epoch += 1,
            "team" => shell.core.active_team = Some("OTHER".into()),
            "transport" => {
                shell.core.transport =
                    Some(Arc::new(Transport::new("other-synthetic-cookie").unwrap()))
            }
            field => {
                let session = shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap();
                match field {
                    "token" => session.token = "other-synthetic-token".into(),
                    "user" => session.user_id = "U_OTHER".into(),
                    "url" => session.url = "https://other-fixture.slack.com".into(),
                    "enterprise" => session.enterprise_id = Some("E_OTHER".into()),
                    "name" => session.name = "Other".into(),
                    _ => panic!("unknown rotation"),
                }
            }
        }
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.boundary.inner.tasks.borrow_mut().clear();
    }
}

pub(super) const ROTATIONS: &[&str] = &[
    "account",
    "epoch",
    "team",
    "transport",
    "token",
    "user",
    "url",
    "enterprise",
    "name",
];
pub(super) const REMOTE: &str =
    r#"{"server":{"id":"Eserver","count":8,"visits":[1699000000000],"opaque":{"keep":"yes"}}}"#;
