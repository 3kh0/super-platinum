//! Test-only execution boundary. Never creates a Session, Transport or cache.
use super::*;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use super_platinum_core::slack::{Error, PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;

pub(super) const TEAM: &str = "T1";
pub(super) const CHANNEL: &str = "C2";
pub(super) const ROOT: &str = "1719800400.000000";
pub(super) const REPLY: &str = "1719800401.000000";
pub(super) const LATEST: &str = "1719800402.000000";
pub(super) const METHODS: [&str; 5] = [
    "client.counts",
    "subscriptions.thread.getView",
    "activity.feed",
    "conversations.history",
    "conversations.replies",
];

pub(super) fn key() -> ThreadKey {
    (TEAM.into(), CHANNEL.into(), ROOT.into())
}

pub(super) fn shell() -> ShellState {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::fixture::install(&mut s);
    assert!(s.core.session.is_none() && s.core.transport.is_none() && s.core.cache.is_none());
    s.thread_root = Some(ROOT.into());
    let epoch = s.core.account_epoch;
    s.core.threads_view.ensure_scope(epoch, TEAM);
    s.core.activity.ensure_scope(epoch, TEAM);
    s.core
        .activity
        .upsert(serde_json::from_value(activity_item(true)).unwrap());
    let bag = s.core.threads.entry(key()).or_default();
    for value in [root(ROOT), reply(REPLY), reply(LATEST)] {
        bag.upsert(serde_json::from_value(value).unwrap());
    }
    bag.last_read = Some(ROOT.into());
    bag.recount_thread_unreads(ROOT);
    bag.has_more_older = true;
    bag.record_remote_deletion("1719800403.000000");
    s.core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .thread_counts
        .apply_live(serde_json::from_value(thread_counts(false)).unwrap());
    s.core
        .workspaces
        .get_mut(TEAM)
        .unwrap()
        .clear_unreads_preference =
        super_platinum_core::state::ClearUnreadsPreference::from_prefs(&BTreeMap::from([(
            super_platinum_core::state::CONFIRM_CLEAR_UNREADS.into(),
            json!(false),
        )]));
    s.refresh_from_core();
    s
}

pub(super) fn harness() -> (VirtualDom, Signal<ShellState>) {
    let handle = Rc::new(RefCell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |handle: Rc<RefCell<Option<Signal<ShellState>>>>| {
            let state = use_signal(shell);
            *handle.borrow_mut() = Some(state);
            rsx! { div {} }
        },
        handle.clone(),
    );
    dom.rebuild_in_place();
    let state = handle.borrow().unwrap();
    (dom, state)
}

pub(super) struct Run {
    pub context: Arc<Context>,
    pub baseline: Arc<Baseline>,
    pub id: u64,
}
pub(super) fn clear(mut state: Signal<ShellState>) -> Run {
    let mut s = state.write();
    let context = Arc::new(Context::capture(&s).unwrap());
    let work = super::super::lifecycle::start_clear(&mut s, context, true).unwrap();
    let id = super::super::finish_write(
        &mut s,
        &work,
        Ok(json!({"ok":true,"undo_ts":"1719800500.000000"})),
    )
    .unwrap();
    Run {
        context: work.context,
        baseline: work.baseline,
        id,
    }
}
pub(super) fn undo(mut state: Signal<ShellState>) -> Run {
    let mut s = state.write();
    let work = super::super::lifecycle::begin_undo(&mut s, std::time::Instant::now()).unwrap();
    let id = super::super::finish_write(&mut s, &work, Ok(json!({"ok":true}))).unwrap();
    Run {
        context: work.context,
        baseline: work.baseline,
        id,
    }
}
pub(super) fn retry(mut state: Signal<ShellState>) -> Run {
    let mut s = state.write();
    let context = Arc::new(Context::capture(&s).unwrap());
    let baseline = Arc::new(Baseline::capture(&s, &context));
    let id = super::super::lifecycle::start_reconcile(&mut s, &context).unwrap();
    Run {
        context,
        baseline,
        id,
    }
}
pub(super) async fn run(state: Signal<ShellState>, work: &Run, script: &Script) {
    run_with(
        state,
        work.context.clone(),
        work.baseline.clone(),
        work.id,
        script,
    )
    .await;
}
pub(super) fn failed(state: Signal<ShellState>, work: &Run) -> bool {
    state.peek().bulk_read.entries[&work.context.owner].refresh_failed
}

pub(super) struct Step {
    method: &'static str,
    result: Result<Value, Error>,
    during: Option<Box<dyn FnOnce()>>,
    started: Option<oneshot::Sender<()>>,
    release: Option<oneshot::Receiver<()>>,
}
impl Step {
    pub fn response(&mut self, edit: impl FnOnce(&mut Value)) {
        edit(self.result.as_mut().expect("positive synthetic response"));
    }
    pub fn during(&mut self, f: impl FnOnce() + 'static) {
        self.during = Some(Box::new(f));
    }
    pub fn error(&mut self) {
        self.result = Err(Error::Transport("Synthetic read failure".into()));
    }
    pub fn malformed(&mut self) {
        self.result = Ok(
            json!({"ok":true,"messages":"not an array","threads":"not an array","items":"not an array","channels":"not an array"}),
        );
    }
    pub fn pause(&mut self) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (started, waiting) = oneshot::channel();
        let (release, held) = oneshot::channel();
        self.started = Some(started);
        self.release = Some(held);
        (waiting, release)
    }
}
pub(super) struct Script {
    steps: RefCell<VecDeque<Step>>,
    pub ledger: RefCell<Vec<Value>>,
}
impl Script {
    pub fn new(s: &ShellState, cleared: bool) -> Self {
        let cursor = if cleared { LATEST } else { ROOT };
        let channels:Vec<_>=s.core.workspaces[TEAM].channels.values().map(|c|json!({
            "id":c.id,"has_unreads":!cleared&&c.id==CHANNEL,"unread_count":if !cleared&&c.id==CHANNEL{2}else{0},
            "unread_count_display":if !cleared&&c.id==CHANNEL{2}else{0},"mention_count":if !cleared&&c.id==CHANNEL{1}else{0},
            "last_read":if c.id==CHANNEL{Some(cursor)}else{c.last_read.as_deref()}
        })).collect();
        let responses = [
            json!({"ok":true,"channels":channels,"ims":[],"groups":[],"mpims":[],"threads":thread_counts(cleared),"activity_v2":{"at_user":if cleared{0}else{2}}}),
            json!({"ok":true,"threads_state":thread_counts(cleared),"threads":[{"root_msg":root(cursor),"latest_replies":[reply(REPLY),reply(LATEST)],"unread_replies":if cleared{json!([])}else{json!([reply(REPLY),reply(LATEST)])}}],"has_more":true}),
            json!({"ok":true,"items":[activity_item(!cleared)],"response_metadata":{"next_cursor":"next-activity"}}),
            json!({"ok":true,"messages":[root(cursor)],"has_more":false}),
            json!({"ok":true,"messages":[root(cursor),reply(REPLY),reply(LATEST),reply("1719800403.000000")],"has_more":false}),
        ];
        Self {
            steps: RefCell::new(
                METHODS
                    .into_iter()
                    .zip(responses)
                    .map(|(method, result)| Step {
                        method,
                        result: Ok(result),
                        during: None,
                        started: None,
                        release: None,
                    })
                    .collect(),
            ),
            ledger: RefCell::new(vec![]),
        }
    }
    pub fn step(&self, index: usize, f: impl FnOnce(&mut Step)) {
        f(&mut self.steps.borrow_mut()[index]);
    }
    pub fn count(&self) -> usize {
        self.ledger.borrow().len()
    }
    pub fn assert_methods(&self, expected: &[&str]) {
        let ledger = self.ledger.borrow();
        assert_eq!(
            ledger
                .iter()
                .map(|v| v["endpoint"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
    }
}
impl reader::Executor for Script {
    async fn execute(&self, request: PreparedRequest) -> Result<Value, Error> {
        let mut step = self
            .steps
            .borrow_mut()
            .pop_front()
            .expect("unexpected extra read");
        let url = url::Url::parse(&request.url).unwrap();
        assert_eq!(url.host_str(), Some("fixture.slack.com"));
        assert_eq!(request.method, "POST");
        assert_eq!(url.path(), format!("/api/{}", step.method));
        let RequestBody::Form(fields) = &request.body else {
            panic!("read must use production form builder")
        };
        let fields: Vec<_> = fields
            .iter()
            .filter(|(k, _)| k != "token")
            .cloned()
            .collect();
        let index = self.ledger.borrow().len();
        self.ledger.borrow_mut().push(json!({"sequence":index,"endpoint":step.method,"method":request.method,"url":request.url,"fields":fields,"retrySafe":request.retry_safe(),"response":step.result.as_ref().ok(),"error":step.result.as_ref().err().map(|_|"synthetic read error")}));
        if let Some(started) = step.started.take() {
            started.send(()).unwrap();
        }
        if let Some(release) = step.release.take() {
            release.await.expect("fixture release dropped");
        }
        tokio::task::yield_now().await;
        if let Some(during) = step.during.take() {
            during();
        }
        step.result
    }
}

fn thread_counts(cleared: bool) -> Value {
    json!({"has_unreads":!cleared,"mention_count":if cleared{0}else{1},"vip_count":0,
        "unread_count_by_channel":if cleared{json!({})}else{json!({(CHANNEL):2})},
        "mention_count_by_channel":if cleared{json!({})}else{json!({(CHANNEL):1})}})
}
fn root(cursor: &str) -> Value {
    json!({"channel":CHANNEL,"ts":ROOT,"thread_ts":ROOT,"text":"Authoritative parent","reply_count":2,"latest_reply":LATEST,"last_read":cursor,"subscribed":true})
}
fn reply(ts: &str) -> Value {
    json!({"channel":CHANNEL,"ts":ts,"thread_ts":ROOT,"text":"Authoritative reply"})
}
fn activity_item(unread: bool) -> Value {
    json!({"key":"bulk-activity","feed_ts":LATEST,"is_unread":unread,"item":{"type":"at_user","message":{"channel":CHANNEL,"ts":LATEST}}})
}
