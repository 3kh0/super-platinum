//! Mandatory purpose-scoped offline reads/history holds; no live fallback.
use super::*;
use std::collections::VecDeque;
use super_platinum_core::slack::api::thread_timestamp::{Kind, Request};
use tokio::sync::oneshot;
#[derive(Default)]
pub(crate) struct Io {
    queue: Mutex<VecDeque<(Kind, oneshot::Receiver<Result<Value, Error>>)>>,
    history: Mutex<VecDeque<oneshot::Receiver<()>>>,
    before_history: Mutex<VecDeque<oneshot::Receiver<()>>>,
    pub(crate) calls: Mutex<Vec<(Kind, String, String)>>,
    pub(crate) history_calls: Mutex<Vec<(String, String, u64, u64)>>,
    pub(crate) completed: AtomicU64,
    pub(crate) handoffs: AtomicU64,
    pub(crate) work_completed: AtomicU64,
}
impl Io {
    pub(crate) fn expect(&self, kind: Kind) -> oneshot::Sender<Result<Value, Error>> {
        let (tx, rx) = oneshot::channel();
        self.queue.lock().unwrap().push_back((kind, rx));
        tx
    }
    pub(crate) fn expect_history(&self) -> oneshot::Sender<()> {
        let (tx, rx) = oneshot::channel();
        self.history.lock().unwrap().push_back(rx);
        tx
    }
    pub(crate) fn hold_before_history(&self) -> oneshot::Sender<()> {
        let (tx, rx) = oneshot::channel();
        self.before_history.lock().unwrap().push_back(rx);
        tx
    }
    pub(super) async fn run_work(&self, state: Signal<ShellState>, work: target::Work) {
        let gate = self.before_history.lock().unwrap().pop_front();
        self.handoffs.fetch_add(1, Ordering::AcqRel);
        if let Some(gate) = gate {
            gate.await.expect("controlled first-poll scheduling hold");
        }
        work.run_with(state, |state| self.history(state)).await;
        self.work_completed.fetch_add(1, Ordering::AcqRel);
    }
    pub(super) async fn request(&self, request: Request) -> Result<Value, Error> {
        let (kind, rx) = self
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .expect("unplanned timestamp-purpose request");
        assert_eq!(kind, request.kind());
        self.calls.lock().unwrap().push((
            kind,
            request.channel_id().into(),
            request.timestamp().into(),
        ));
        let result = rx.await.expect("test retains original timestamp response");
        self.completed.fetch_add(1, Ordering::AcqRel);
        result
    }
    pub(super) async fn history(&self, state: Signal<ShellState>) {
        let rx = self
            .history
            .lock()
            .unwrap()
            .pop_front()
            .expect("unplanned timestamp history handoff");
        let s = state.peek();
        assert_eq!(s.main_view, MainView::Home);
        assert!(s.overlay.is_none());
        assert!(s.thread_root.is_none());
        let Some((channel, super_platinum_core::domain::PendingScrollTarget::Message(ts))) =
            s.core.pending_scroll_to.as_ref()
        else {
            panic!("exact original message anchor")
        };
        assert_eq!(s.core.active_channel.as_deref(), Some(channel.as_str()));
        self.history_calls.lock().unwrap().push((
            channel.clone(),
            ts.clone(),
            s.channel_generation,
            s.thread_generation,
        ));
        drop(s);
        rx.await.expect("test owns controlled history completion");
    }
}
