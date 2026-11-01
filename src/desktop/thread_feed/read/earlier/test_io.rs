//! Inert scoped raw IO using the actual request/decoder/publication pipeline.
use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;
type Reply = Result<Value, Error>;
pub(super) fn current() -> Option<Rc<Io>> {
    let runtime = dioxus::core::Runtime::try_current()?;
    runtime.try_current_scope_id()?;
    dioxus::prelude::try_consume_context()
}
#[derive(Default)]
pub(super) struct Io {
    queue: RefCell<VecDeque<(String, String, Option<String>, oneshot::Receiver<Reply>)>>,
    pub(super) reads: RefCell<Vec<(String, String)>>,
    pub(super) completed: Cell<usize>,
    pub(super) cursors: RefCell<Vec<Option<String>>>,
}
impl Io {
    pub(super) fn expect(&self, root: &str, boundary: &str) -> oneshot::Sender<Reply> {
        self.expect_page(root, boundary, None)
    }
    pub(super) fn expect_page(
        &self,
        root: &str,
        boundary: &str,
        cursor: Option<&str>,
    ) -> oneshot::Sender<Reply> {
        assert!(self.queue.borrow().len() < 4);
        let (send, receive) = oneshot::channel();
        self.queue.borrow_mut().push_back((
            root.into(),
            boundary.into(),
            cursor.map(str::to_owned),
            receive,
        ));
        send
    }
    pub(super) async fn execute(&self, request: PreparedRequest) -> Reply {
        let (root, boundary, cursor, receive) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned earlier-reply read");
        assert_eq!(request.method, "POST");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/conversations.replies"
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("documented full-body form")
        };
        let actual = fields
            .into_iter()
            .filter(|(k, _)| k != "token")
            .collect::<BTreeMap<_, _>>();
        let mut expected = BTreeMap::from([
            ("channel".into(), "C1".into()),
            ("ts".into(), root.clone()),
            ("latest".into(), boundary.clone()),
            ("inclusive".into(), "true".into()),
            ("limit".into(), "100".into()),
        ]);
        if let Some(cursor) = cursor.as_ref() {
            expected.insert("cursor".into(), cursor.clone());
        }
        assert_eq!(actual, expected);
        self.cursors.borrow_mut().push(cursor);
        self.reads.borrow_mut().push((root, boundary));
        let result = receive.await.expect("fixture owns raw reply");
        self.completed.set(self.completed.get() + 1);
        result
    }
}
