//! Mandatory scoped offline writer; no test can fall through to real Slack.
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use super_platinum_core::slack::{
    api::file_sort_preference as wire, transport::FileSortPreferenceFailure,
};
use tokio::sync::oneshot;
type Reply = Result<serde_json::Value, FileSortPreferenceFailure>;
#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<(api::Sort, oneshot::Receiver<Reply>)>>,
    pub(crate) sorts: RefCell<Vec<api::Sort>>,
    pub(crate) completed: Cell<usize>,
}
impl Io {
    pub(crate) fn expect(&self, sort: api::Sort) -> oneshot::Sender<Reply> {
        let (s, r) = oneshot::channel();
        self.queue.borrow_mut().push_back((sort, r));
        s
    }
    pub(crate) async fn execute(&self, request: wire::Request) -> Reply {
        let (sort, r) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned saved File preference IO");
        assert_eq!(request.submitted_sort(), sort);
        self.sorts.borrow_mut().push(sort);
        let result = r.await.expect("offline test owns reply");
        self.completed.set(self.completed.get() + 1);
        result
    }
}
