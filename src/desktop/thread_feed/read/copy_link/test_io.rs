//! Explicit inert raw IO/promise model after original intake; never native proof.
use super::*;
use std::collections::VecDeque;
use super_platinum_core::slack::{PreparedRequest, client::RequestBody};
use tokio::sync::{mpsc, oneshot};
type Reply = Result<Vec<u8>, Error>;
pub(crate) struct Io {
    token: String,
    replies: Mutex<VecDeque<(String, oneshot::Receiver<Reply>)>>,
    pub calls: Mutex<Vec<String>>,
    bridges: Mutex<VecDeque<ModelBridge>>,
    pub configs: Mutex<Vec<Config>>,
}
impl Io {
    pub(crate) fn new(token: &str) -> Self {
        Self {
            token: token.into(),
            replies: Mutex::new(VecDeque::new()),
            calls: Mutex::new(vec![]),
            bridges: Mutex::new(VecDeque::new()),
            configs: Mutex::new(vec![]),
        }
    }
    pub(crate) fn expect(&self, method: &str) -> oneshot::Sender<Reply> {
        let (tx, rx) = oneshot::channel();
        self.replies.lock().unwrap().push_back((method.into(), rx));
        tx
    }
    pub(super) async fn request(&self, request: PreparedRequest) -> Result<Value, Error> {
        let (expected, reply) = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unplanned Copy-purpose IO");
        assert!(
            request
                .url
                .split('?')
                .next()
                .unwrap()
                .ends_with(&format!("/api/{expected}"))
        );
        let fields = match &request.body {
            RequestBody::Form(f) => f.iter().cloned().collect::<BTreeMap<_, _>>(),
            RequestBody::Json(v) => v
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().into()))
                .collect(),
            _ => panic!("original account request body"),
        };
        assert_eq!(
            fields.get("token").map(String::as_str),
            Some(self.token.as_str())
        );
        if expected == "conversations.history" {
            assert_eq!(fields["limit"], "1");
            assert_eq!(fields["inclusive"], "true");
            assert!(!fields.contains_key("cursor"));
        }
        self.calls.lock().unwrap().push(expected);
        let bytes = reply.await.expect("test retains original response")?;
        super_platinum_core::slash_commands::codec::decode(&bytes, 1024 * 1024)
            .map_err(|_| Error::Transport("invalid model response".into()))
    }
    pub(crate) fn bridge(&self) -> Peer {
        let (to_js, from_rust) = mpsc::unbounded_channel();
        let (to_rust, from_js) = mpsc::unbounded_channel();
        self.bridges
            .lock()
            .unwrap()
            .push_back(ModelBridge { to_js, from_js });
        Peer { to_rust, from_rust }
    }
    pub(super) fn open(&self, config: Config) -> ModelBridge {
        self.configs.lock().unwrap().push(config);
        self.bridges
            .lock()
            .unwrap()
            .pop_front()
            .expect("explicit model bridge only")
    }
}
pub(super) struct ModelBridge {
    to_js: mpsc::UnboundedSender<Value>,
    from_js: mpsc::UnboundedReceiver<Value>,
}
impl ModelBridge {
    pub(super) async fn recv(&mut self) -> Option<Value> {
        self.from_js.recv().await
    }
    pub(super) fn send(&self, v: Value) -> Result<(), ()> {
        self.to_js.send(v).map_err(|_| ())
    }
}
pub(crate) struct Peer {
    pub to_rust: mpsc::UnboundedSender<Value>,
    pub from_rust: mpsc::UnboundedReceiver<Value>,
}

// Observes the actual ReadyControl body/listener. This is not a native mount,
// trusted gesture, capability, or fulfilled clipboard receipt.
#[derive(Clone, Default)]
pub(crate) struct Rendered(pub std::rc::Rc<std::cell::RefCell<Option<VNode>>>);
pub(crate) fn record(element: &Element) {
    if let Some(c) = dioxus::core::try_consume_context::<Rendered>() {
        *c.0.borrow_mut() = element.as_ref().ok().cloned();
    }
}
