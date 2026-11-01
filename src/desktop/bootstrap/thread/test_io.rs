//! Explicit test-scope replies executor; actual request/decoder/loader stay.
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use super_platinum_core::slack::{Error, PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;

type Reply = Result<serde_json::Value, Error>;

pub(crate) fn current() -> Option<std::rc::Rc<Io>> {
    // Original reconnect/retired-read helpers also run outside a Dioxus runtime.
    // Optional test injection must preserve their original early-return paths.
    let runtime = dioxus::core::Runtime::try_current()?;
    runtime.try_current_scope_id()?;
    dioxus::prelude::try_consume_context()
}
#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<(String, String, oneshot::Receiver<Reply>)>>,
    pub reads: RefCell<Vec<(String, String)>>,
    pub completed: Cell<usize>,
}
impl Io {
    pub(crate) fn expect(&self, channel: &str, root: &str) -> oneshot::Sender<Reply> {
        assert!(self.queue.borrow().len() < 4);
        let (send, receive) = oneshot::channel();
        self.queue
            .borrow_mut()
            .push_back((channel.into(), root.into(), receive));
        send
    }
    pub(crate) async fn execute(&self, request: PreparedRequest) -> Reply {
        let (channel, root, receive) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned root-thread replies read");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/conversations.replies"
        );
        assert_eq!(request.method, "POST");
        let RequestBody::Form(fields) = request.body else {
            panic!("original replies form required")
        };
        let field = |name: &str| {
            let found = fields
                .iter()
                .filter(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>();
            assert_eq!(found.len(), 1, "one exact {name} field");
            found[0]
        };
        assert_eq!(field("channel"), channel);
        assert_eq!(field("ts"), root);
        assert_eq!(field("oldest"), root);
        assert_eq!(field("limit"), "200");
        assert_eq!(field("inclusive"), "true");
        assert!(
            !fields
                .iter()
                .any(|(k, _)| matches!(k.as_str(), "cursor" | "latest"))
        );
        self.reads.borrow_mut().push((channel, root));
        let reply = receive.await.expect("test owns replies response");
        self.completed.set(self.completed.get() + 1);
        reply
    }
}
