//! Mandatory scoped offline dependency. Never fall through to live transport.
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
    bytes: RefCell<VecDeque<oneshot::Receiver<Result<Vec<u8>, Error>>>>,
    pub(crate) calls: RefCell<Vec<(bool, String)>>,
    pub(crate) byte_urls: RefCell<Vec<String>>,
    pub(crate) completed: Cell<usize>,
    pub(crate) byte_completed: Cell<usize>,
}
pub(super) fn get() -> Rc<Io> {
    try_consume_context::<Rc<Io>>().expect("isolated image preview test requires scoped offline IO")
}
impl Io {
    pub(crate) fn expect(&self, ip: bool) -> oneshot::Sender<Reply> {
        let (s, r) = oneshot::channel();
        self.queue.borrow_mut().push_back((ip, r));
        s
    }
    pub(crate) fn expect_bytes(&self) -> oneshot::Sender<Result<Vec<u8>, Error>> {
        let (s, r) = oneshot::channel();
        self.bytes.borrow_mut().push_back(r);
        s
    }
    pub(super) async fn execute(&self, request: Request) -> Reply {
        let (ip, r) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned isolated image request");
        assert_eq!(request.is_ip(), ip);
        self.calls.borrow_mut().push((ip, request.file_id().into()));
        let result = r.await.expect("test owns original reply");
        self.completed.set(self.completed.get() + 1);
        result
    }
    pub(super) async fn bytes(&self, url: &str) -> Result<Vec<u8>, Error> {
        let r = self
            .bytes
            .borrow_mut()
            .pop_front()
            .expect("unplanned image fetch");
        self.byte_urls.borrow_mut().push(url.into());
        let result = r.await.expect("test owns image bytes");
        self.byte_completed.set(self.byte_completed.get() + 1);
        result
    }
}
