//! Scoped test executor after actual Messages request and page admission.
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use super_platinum_core::{
    search::SearchSort,
    slack::{Error, PreparedRequest, client::RequestBody, models::User},
};
use tokio::sync::oneshot;
type Reply = Result<serde_json::Value, Error>;
struct Expected {
    query: String,
    page: u32,
    sort: SearchSort,
    reply: oneshot::Receiver<Reply>,
}
#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<Expected>>,
    pub(crate) reads: RefCell<Vec<(String, u32)>>,
    pub(crate) completed: Cell<usize>,
    pub(crate) lookups: Cell<usize>,
}
impl Io {
    pub(crate) fn expect(
        &self,
        query: &str,
        page: u32,
        sort: SearchSort,
    ) -> oneshot::Sender<Reply> {
        let (send, reply) = oneshot::channel();
        self.queue.borrow_mut().push_back(Expected {
            query: query.into(),
            page,
            sort,
            reply,
        });
        send
    }
    pub(crate) async fn execute(&self, request: PreparedRequest) -> Reply {
        let expected = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned Messages scope read");
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/search.modules.messages"
        );
        assert_eq!(request.method, "POST");
        let RequestBody::Form(fields) = request.body else {
            panic!("unchanged Messages POST form required")
        };
        let field = |name: &str| {
            let values = fields
                .iter()
                .filter(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>();
            assert_eq!(values.len(), 1, "one exact {name} field");
            values[0]
        };
        assert_eq!(field("module"), "messages");
        assert_eq!(field("query"), expected.query);
        assert_eq!(field("page"), expected.page.to_string());
        assert_eq!(field("count"), "20");
        assert_eq!(field("sort"), expected.sort.api().0);
        assert_eq!(field("sort_dir"), expected.sort.api().1);
        self.reads
            .borrow_mut()
            .push((expected.query, expected.page));
        let result = expected.reply.await.expect("test owns reply");
        self.completed.set(self.completed.get() + 1);
        result
    }
    pub(crate) fn lookup(&self, _ids: Vec<String>) -> std::future::Ready<Result<Vec<User>, Error>> {
        self.lookups.set(self.lookups.get() + 1);
        std::future::ready(Ok(Vec::new()))
    }
}
