//! Mandatory, scope-local offline reader; never falls through to a real socket.
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<tokio::sync::oneshot::Receiver<Result<Value, Error>>>>,
    pub calls: Cell<usize>,
    pub gated: Cell<usize>,
    gates: RefCell<VecDeque<tokio::sync::oneshot::Receiver<()>>>,
    pub completed: Cell<usize>,
    pub opens: RefCell<Vec<(String, String)>>,
}
impl Io {
    pub fn hold_dispatch(&self) -> tokio::sync::oneshot::Sender<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.gates.borrow_mut().push_back(rx);
        tx
    }
    pub fn expect(&self) -> tokio::sync::oneshot::Sender<Result<Value, Error>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.queue.borrow_mut().push_back(rx);
        tx
    }
}
pub(super) async fn execute(
    req: &PreparedRequest,
    current: impl Fn() -> bool,
    mut admit: impl FnMut() -> bool,
    mut observe: impl FnMut(&Result<Value, Error>),
) -> Result<Option<Value>, Error> {
    let io = try_consume_context::<Rc<Io>>()
        .expect("channel Threads tests require original scoped executor");
    assert_eq!(req.method, "POST");
    assert!(req.url.contains("/api/subscriptions.thread.getView?"));
    let RequestBody::Form(fields) = &req.body else {
        panic!("original POST form required")
    };
    assert_eq!(fields.iter().filter(|(k, _)| k == "token").count(), 1);
    let params = fields
        .iter()
        .filter(|(k, _)| k != "token")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        params,
        vec![
            ("limit".into(), "5".into()),
            ("fetch_threads_state".into(), "true".into()),
            ("priority_mode".into(), "all".into()),
            ("channel_id".into(), "CINIT0001".into())
        ]
    );
    let response = io
        .queue
        .borrow_mut()
        .pop_front()
        .expect("one original explicit read expectation");
    let gate = io.gates.borrow_mut().pop_front();
    if let Some(gate) = gate {
        io.gated.set(io.gated.get() + 1);
        gate.await.expect("original synthetic link dispatch gate");
    }
    if !current() {
        return Err(Error::Api("request_superseded".into()));
    }
    if !admit() {
        return Ok(None);
    }
    io.calls.set(io.calls.get() + 1);
    let result = response
        .await
        .expect("owned offline response must be delivered");
    io.completed.set(io.completed.get() + 1);
    observe(&result);
    if !current() {
        return Err(Error::Api("request_superseded".into()));
    }
    result.map(Some)
}
pub(super) fn opened(_: Signal<ShellState>, root: &str, scope: &str) {
    let io =
        try_consume_context::<Rc<Io>>().expect("thread handoff requires original scoped executor");
    io.opens.borrow_mut().push((root.into(), scope.into()));
}
