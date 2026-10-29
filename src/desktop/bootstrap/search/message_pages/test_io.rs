//! Scoped test-only IO after real control admission and request construction.
//! No default executor, global switch, transport call or fabricated page ticket.
use super::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use super_platinum_core::slack::{Error, PreparedRequest, client::RequestBody, models::User};
use tokio::sync::oneshot;
type Reply = Result<Value, Error>;
#[derive(Default)]
pub(crate) struct Io {
    queued: RefCell<VecDeque<(u32, oneshot::Receiver<Reply>)>>,
    pub(crate) pages: RefCell<Vec<u32>>,
    pub(crate) lookups: Cell<usize>,
}
impl Io {
    pub(crate) fn expect(&self, page: u32) -> oneshot::Sender<Reply> {
        let (send, receive) = oneshot::channel();
        self.queued.borrow_mut().push_back((page, receive));
        send
    }
    pub(crate) async fn execute(&self, request: PreparedRequest) -> Reply {
        assert!(
            request
                .url
                .split('?')
                .next()
                .unwrap()
                .ends_with("/api/search.modules.messages")
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("existing Message form")
        };
        let fields = fields.into_iter().collect::<BTreeMap<_, _>>();
        let (expected, receive) = self
            .queued
            .borrow_mut()
            .pop_front()
            .expect("unplanned page IO");
        assert_eq!(fields["page"], expected.to_string());
        assert_eq!(fields["query"], "release");
        assert_eq!(fields["count"], "20");
        self.pages.borrow_mut().push(expected);
        receive.await.expect("test owns reply")
    }
    pub(crate) fn lookup(&self, _ids: Vec<String>) -> std::future::Ready<Result<Vec<User>, Error>> {
        self.lookups.set(self.lookups.get() + 1);
        std::future::ready(Ok(Vec::new()))
    }
}
