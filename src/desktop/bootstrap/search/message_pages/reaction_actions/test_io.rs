//! Context-scoped raw responses after original control admission and wire build.
//! No default executor, page ticket, FreshPolicy, watch or write permit is made.
use super::*;
use std::collections::VecDeque;
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::oneshot;
type Reply = Result<Vec<u8>, Error>;
pub(crate) struct Io {
    token: String,
    queue: Mutex<VecDeque<(String, oneshot::Receiver<Reply>)>>,
    pub calls: Mutex<Vec<(String, BTreeMap<String, String>, bool)>>,
}
impl Io {
    pub(crate) fn new(token: &str) -> Self {
        Self {
            token: token.into(),
            queue: Mutex::new(VecDeque::new()),
            calls: Mutex::new(Vec::new()),
        }
    }
    pub(crate) fn expect(&self, path: &str) -> oneshot::Sender<Reply> {
        let (send, recv) = oneshot::channel();
        let mut q = self.queue.lock().unwrap();
        assert!(q.len() < 16);
        q.push_back((path.into(), recv));
        send
    }
    pub(super) async fn execute(
        &self,
        request: PreparedRequest,
        writing: bool,
    ) -> Result<Value, Error> {
        let (path, reply) = self
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .expect("unplanned Search reaction IO");
        assert_eq!(request.method, "POST");
        assert!(
            request
                .url
                .split('?')
                .next()
                .unwrap()
                .ends_with(&format!("/api/{path}"))
        );
        assert!(
            request
                .headers
                .iter()
                .all(|(k, _)| !k.eq_ignore_ascii_case("cookie"))
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("original form")
        };
        let len = fields.len();
        let fields = fields.into_iter().collect::<BTreeMap<_, _>>();
        assert_eq!(len, fields.len());
        assert_eq!(fields["token"], self.token);
        assert_eq!(
            writing,
            matches!(path.as_str(), "reactions.add" | "reactions.remove")
        );
        self.calls.lock().unwrap().push((path, fields, writing));
        let bytes = reply.await.expect("test owns original response");
        super_platinum_core::slash_commands::codec::decode(&bytes?, 1024 * 1024)
            .map_err(|_| Error::Transport("invalid synthetic response".into()))
    }
}
