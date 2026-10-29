//! Optional scoped test executor at the actual legacy Files load boundary.
//! The new typed-editor UI launcher requires it; production has no override.
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use super_platinum_core::slack::{Error, PreparedRequest};
use tokio::sync::oneshot;
type Reply = Result<serde_json::Value, Error>;
#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<(String, oneshot::Receiver<Reply>)>>,
    pub(crate) queries: RefCell<Vec<String>>,
    pub(crate) completed: Cell<usize>,
}
impl Io {
    pub(crate) fn expect(&self, query: &str) -> oneshot::Sender<Reply> {
        let (s, r) = oneshot::channel();
        self.queue.borrow_mut().push_back((query.into(), r));
        s
    }
    pub(crate) async fn execute(&self, request: PreparedRequest) -> Reply {
        let (query, reply) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned File category read");
        let url = url::Url::parse(&request.url).unwrap();
        assert_eq!(url.path(), "/api/search.files");
        assert_eq!(request.method, "POST");
        let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
            panic!("existing Files builder must supply the POST form");
        };
        let query_fields = fields
            .iter()
            .filter(|(key, _)| key == "query")
            .collect::<Vec<_>>();
        assert_eq!(query_fields.len(), 1, "exactly one query field");
        assert_eq!(query_fields[0].1, query);
        self.queries.borrow_mut().push(query);
        let result = reply.await.expect("test owns response");
        self.completed.set(self.completed.get() + 1);
        result
    }
}
