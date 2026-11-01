//! Scoped inert mounted renderer IO. Model messages can never report native decode.
use super::isolated::Config;
use dioxus::prelude::*;
use serde_json::Value;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use tokio::sync::mpsc;

#[derive(Clone, Default)]
pub(crate) struct Io(Rc<RefCell<State>>);
#[derive(Default)]
struct State {
    bridges: VecDeque<ModelBridge>,
    configs: Vec<Config>,
    rendered: Option<VNode>,
}
impl Io {
    pub(crate) fn bridge(&self) -> Peer {
        let (to_js, from_rust) = mpsc::unbounded_channel();
        let (to_rust, from_js) = mpsc::unbounded_channel();
        self.0
            .borrow_mut()
            .bridges
            .push_back(ModelBridge { to_js, from_js });
        Peer { to_rust, from_rust }
    }
    pub(crate) fn configs(&self) -> Vec<Config> {
        self.0.borrow().configs.clone()
    }
    pub(crate) fn rendered(&self) -> Option<VNode> {
        self.0.borrow().rendered.clone()
    }
    pub(super) fn open(&self, config: Config) -> ModelBridge {
        let mut state = self.0.borrow_mut();
        state.configs.push(config);
        state
            .bridges
            .pop_front()
            .expect("explicit isolated PDF model bridge")
    }
}
pub(crate) struct Peer {
    pub to_rust: mpsc::UnboundedSender<Value>,
    pub from_rust: mpsc::UnboundedReceiver<Value>,
}
pub(super) struct ModelBridge {
    to_js: mpsc::UnboundedSender<Value>,
    from_js: mpsc::UnboundedReceiver<Value>,
}
impl ModelBridge {
    pub(super) async fn recv(&mut self) -> Option<Value> {
        self.from_js.recv().await
    }
    pub(super) fn send(&self, value: Value) -> Result<(), ()> {
        self.to_js.send(value).map_err(|_| ())
    }
}
pub(super) fn record(element: &Element) {
    if let Some(io) = dioxus::core::try_consume_context::<Io>() {
        io.0.borrow_mut().rendered = element.as_ref().ok().cloned();
    }
}
