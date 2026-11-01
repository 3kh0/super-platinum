//! Scoped offline execution only; production request/decoder/publication remain.
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use super_platinum_core::slack::{Error, PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;
type Reply = Result<serde_json::Value, Error>;
pub(super) fn current() -> Option<Rc<Io>> {
    let runtime = dioxus::core::Runtime::try_current()?;
    runtime.try_current_scope_id()?;
    dioxus::prelude::try_consume_context()
}
#[derive(Default)]
pub(in crate::thread_feed::read) struct Io {
    queue: RefCell<VecDeque<(Vec<String>, oneshot::Receiver<Reply>)>>,
    pub(in crate::thread_feed::read) reads: RefCell<Vec<Vec<String>>>,
    pub(in crate::thread_feed::read) completed: Cell<usize>,
}
impl Io {
    pub(in crate::thread_feed::read) fn expect(&self, ids: Vec<String>) -> oneshot::Sender<Reply> {
        assert!(self.queue.borrow().len() < 4);
        let (send, receive) = oneshot::channel();
        self.queue.borrow_mut().push_back((ids, receive));
        send
    }
    pub(super) async fn execute(&self, request: PreparedRequest) -> Reply {
        let (expected, receive) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned visible-author profile read");
        assert_eq!(request.method, "POST");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/cache/T1/users/info"
        );
        let RequestBody::Json(body) = request.body else {
            panic!("original Flannel JSON required")
        };
        assert_eq!(body["check_interaction"], serde_json::json!(true));
        assert_eq!(body["include_profile_only_users"], serde_json::json!(true));
        let ids = body["updated_ids"].as_object().unwrap();
        assert!(!ids.is_empty() && ids.len() <= 100);
        assert!(ids.values().all(|v| v == &serde_json::json!(0)));
        let mut actual = ids.keys().cloned().collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, expected);
        self.reads.borrow_mut().push(actual);
        let result = receive.await.expect("test owns profile response");
        self.completed.set(self.completed.get() + 1);
        result
    }
}
