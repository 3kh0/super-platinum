//! Scoped synthetic responses after the real request and owner admission.
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use super_platinum_core::slack::{Error, PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;

type Reply = Result<serde_json::Value, Error>;
#[derive(Default)]
pub(super) struct Io {
    queued: RefCell<VecDeque<(&'static str, oneshot::Receiver<Reply>)>>,
    pub(super) calls: RefCell<Vec<(&'static str, serde_json::Value)>>,
    pub(super) completed: Cell<usize>,
}
impl Io {
    pub(super) fn expect(&self, method: &'static str) -> oneshot::Sender<Reply> {
        assert!(self.queued.borrow().len() < 4);
        assert!(matches!(method, "users.prefs.get" | "users.info"));
        let (send, receive) = oneshot::channel();
        self.queued.borrow_mut().push_back((method, receive));
        send
    }
    pub(super) async fn execute(&self, request: PreparedRequest) -> Reply {
        let (method, receive) = self
            .queued
            .borrow_mut()
            .pop_front()
            .expect("unplanned Search-author metadata read");
        assert_eq!(request.method, "POST");
        let path = request.url.split('?').next().unwrap();
        let body = match (method, request.body) {
            ("users.prefs.get", RequestBody::Form(fields)) => {
                assert!(path.ends_with("/api/users.prefs.get"));
                assert_eq!(fields.len(), 1, "complete own prefs, no target or names");
                assert_eq!(fields[0].0, "token");
                assert!(!fields[0].1.is_empty());
                serde_json::json!({})
            }
            ("users.info", RequestBody::Json(body)) => {
                assert_eq!(path, "https://edgeapi.slack.com/cache/T1/users/info");
                assert_eq!(body["check_interaction"], serde_json::json!(true));
                assert_eq!(body["include_profile_only_users"], serde_json::json!(true));
                let ids = body["updated_ids"].as_object().unwrap();
                assert!(!ids.is_empty() && ids.len() <= 100);
                assert!(ids.values().all(|v| v == &serde_json::json!(0)));
                body
            }
            _ => panic!("original full prefs form or strict profile JSON required"),
        };
        self.calls.borrow_mut().push((method, body));
        let result = receive.await.expect("test owns the metadata response");
        self.completed.set(self.completed.get() + 1);
        result
    }
}
