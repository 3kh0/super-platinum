//! Mandatory scoped offline IO. No fallthrough to live transport or media.
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use tokio::sync::oneshot;
type Reply = Result<serde_json::Value, Error>;
#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<(bool, oneshot::Receiver<Reply>)>>,
    bodies: RefCell<VecDeque<oneshot::Receiver<Result<Vec<u8>, Error>>>>,
    pub calls: RefCell<Vec<(bool, String)>>,
    pub urls: RefCell<Vec<String>>,
    pub completed: Cell<usize>,
    pub body_completed: Cell<usize>,
}
pub(super) fn get() -> Rc<Io> {
    try_consume_context::<Rc<Io>>().expect("isolated PDF test requires scoped offline IO")
}
impl Io {
    pub(crate) fn expect(&self, ip: bool) -> oneshot::Sender<Reply> {
        let (tx, rx) = oneshot::channel();
        self.queue.borrow_mut().push_back((ip, rx));
        tx
    }
    pub(crate) fn expect_bytes(&self) -> oneshot::Sender<Result<Vec<u8>, Error>> {
        let (tx, rx) = oneshot::channel();
        self.bodies.borrow_mut().push_back(rx);
        tx
    }
    pub(super) async fn execute(&self, request: Request) -> Reply {
        let (ip, rx) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned PDF-purpose request");
        assert_eq!(request.is_ip(), ip);
        self.calls.borrow_mut().push((ip, request.file_id().into()));
        let result = rx.await.expect("original PDF response retained");
        self.completed.set(self.completed.get() + 1);
        result
    }
    pub(super) async fn bytes(&self, url: &str) -> Result<Vec<u8>, Error> {
        let rx = self
            .bodies
            .borrow_mut()
            .pop_front()
            .expect("unplanned PDF content read");
        self.urls.borrow_mut().push(url.into());
        let result = rx.await.expect("original PDF body retained");
        self.body_completed.set(self.body_completed.get() + 1);
        result
    }
}
