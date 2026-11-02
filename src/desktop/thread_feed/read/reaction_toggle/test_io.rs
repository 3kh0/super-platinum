//! Scoped synthetic requests. Never falls through to external Slack execution.
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use super_platinum_core::slack::{Error, PreparedRequest};
use tokio::sync::oneshot;
struct Expected {
    path: String,
    reply: oneshot::Receiver<Result<Value, Error>>,
}
#[derive(Default)]
pub(crate) struct Io {
    pub(crate) requests: RefCell<Vec<PreparedRequest>>,
    pub(crate) completed: Cell<usize>,
    expected: RefCell<VecDeque<Expected>>,
}
impl Io {
    pub(crate) fn expect(&self, path: &str) -> oneshot::Sender<Result<Value, Error>> {
        let (tx, reply) = oneshot::channel();
        self.expected.borrow_mut().push_back(Expected {
            path: path.into(),
            reply,
        });
        tx
    }
    pub(crate) async fn execute(&self, request: PreparedRequest) -> Result<Value, Error> {
        let path = url::Url::parse(&request.url).unwrap().path().to_owned();
        let expected = {
            let mut queue = self.expected.borrow_mut();
            let index = queue
                .iter()
                .position(|e| e.path == path)
                .expect("unexpected synthetic Root/legacy request path");
            queue.remove(index).unwrap()
        };
        self.requests.borrow_mut().push(request);
        let result = expected
            .reply
            .await
            .map_err(|_| Error::Offline("synthetic Root reply retired".into()))
            .and_then(|r| r);
        self.completed.set(self.completed.get() + 1);
        result
    }
    pub(crate) fn paths(&self) -> Vec<String> {
        self.requests
            .borrow()
            .iter()
            .map(|r| url::Url::parse(&r.url).unwrap().path().to_owned())
            .collect()
    }
}
