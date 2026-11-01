//! Scoped offline history hold for actual navigation callbacks. No Ready flag,
//! publisher or endpoint override; only the unchanged history dependency waits.
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
use std::{cell::RefCell, collections::VecDeque};
use tokio::sync::oneshot;

#[derive(Default)]
pub(crate) struct Io {
    queue: RefCell<VecDeque<(String, String, oneshot::Receiver<()>)>>,
    pub reads: RefCell<Vec<(String, String)>>,
}
impl Io {
    pub(crate) fn expect(&self, channel: &str, root: &str) -> oneshot::Sender<()> {
        assert!(self.queue.borrow().len() < 4);
        let (send, receive) = oneshot::channel();
        self.queue
            .borrow_mut()
            .push_back((channel.into(), root.into(), receive));
        send
    }
    pub(crate) async fn refresh(&self, state: Signal<ShellState>) {
        let (channel, root, receive) = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unplanned navigation history read");
        {
            let s = state.peek();
            assert_eq!(s.core.active_channel.as_deref(), Some(channel.as_str()));
            assert_eq!(s.main_view, MainView::Home);
            assert!(s.thread_root.is_none(), "history precedes thread start");
            assert!(s.overlay.is_none());
            assert!(
                matches!(s.core.pending_scroll_to.as_ref(),Some((c,super_platinum_core::domain::PendingScrollTarget::Message(ts))) if c==&channel&&ts==&root),
                "the original navigation anchors the exact root"
            );
        }
        self.reads.borrow_mut().push((channel, root));
        receive
            .await
            .expect("test owns controlled history completion");
    }
}
