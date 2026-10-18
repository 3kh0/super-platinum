//! A pane-owned cancellation sender; the waiting task never retains the sender.
use std::sync::Arc;
use tokio::sync::watch;
#[derive(Clone)]
pub(in crate::profile_edit::sections) struct Lease {
    identity: Arc<()>,
    cancel: watch::Sender<bool>,
}
impl Lease {
    pub(super) fn new() -> (Self, Arc<()>, watch::Receiver<bool>) {
        let identity = Arc::new(());
        let (cancel, receiver) = watch::channel(false);
        (
            Self {
                identity: identity.clone(),
                cancel,
            },
            identity,
            receiver,
        )
    }
    pub(in crate::profile_edit::sections) fn retire(self) {
        self.cancel.send_replace(true);
    }
    pub(super) fn owns(&self, identity: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.identity, identity)
    }
}
pub(super) async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}
