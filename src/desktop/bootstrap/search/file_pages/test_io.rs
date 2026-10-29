//! Mandatory scoped offline executor after actual private request construction.
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use tokio::sync::oneshot;
type Reply = Result<serde_json::Value, Error>;
#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<(u32, oneshot::Receiver<Reply>)>>,
    pub(crate) pages: RefCell<Vec<u32>>,
    pub(crate) completed: Cell<usize>,
}
impl Io {
    pub(crate) fn expect(&self, page: u32) -> oneshot::Sender<Reply> {
        let (s, r) = oneshot::channel();
        self.queue.borrow_mut().push_back((page, r));
        s
    }
    pub(crate) async fn execute(&self, request: api::Request) -> Reply {
        let (page, r) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned private Files IO");
        assert_eq!(request.requested_page(), page);
        self.pages.borrow_mut().push(page);
        let result = r.await.expect("test owns reply");
        self.completed.set(self.completed.get() + 1);
        result
    }
}
