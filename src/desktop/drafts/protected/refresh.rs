//! Receipt refresh cannot silently replace a writer or its visible input.
use super::*;
use super::{
    editor::{Editor, Snapshot},
    editor_proof::ReviewLease,
};
use super_platinum_core::drafts::sync::ConsumeIntent;

impl Editor {
    pub async fn refresh_receipt(self: &Arc<Self>) -> Result<Snapshot, durable::Error> {
        let editor = self.clone();
        self.runtime
            .spawn(async move {
                let _gate = editor.gate.lock().await;
                let before = editor.capture(Visit::new())?;
                let fresh = editor.owner.review().await?;
                let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                if state.serial != before.serial
                    || state.writer.id() != before.writer.id()
                    || !state.review.same(&before.review)
                    || state.running
                    || state.busy
                    || state.parked
                    || state.released.is_some()
                    || !editor_proof::can_rebase(&before.review, &fresh, &before.writer)
                {
                    return Err(durable::Error::Stale);
                }
                state.review = fresh;
                state.direct = state
                    .review
                    .row()
                    .state()
                    .local_preservation_intent()
                    .cloned();
                drop(state);
                Ok(editor.snapshot())
            })
            .await
            .map_err(|_| durable::Error::Storage)?
    }

    /// Explicit Continue editing locally preserves a completion's collision
    /// history. It is not a remote dispatch or a generic release shortcut.
    pub async fn keep_editing<G>(
        self: &Arc<Self>,
        lease: ReviewLease,
        intent: ConsumeIntent,
        guard: G,
    ) -> Result<(), durable::Error>
    where
        G: FnOnce() -> Result<(), durable::Error> + 'static,
    {
        let editor = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = async {
                editor.flush().await?;
                let _gate = editor.gate.lock().await;
                let fresh = editor.owner.review().await?;
                let captured = editor.capture(lease.visit())?;
                if !fresh.same(lease.review()) {
                    return Err(durable::Error::Stale);
                }
                let reply = {
                    let mut state = editor.state.lock().map_err(|_| durable::Error::Storage)?;
                    if state.serial != captured.serial
                        || state.running
                        || state.busy
                        || !state.review.same(&captured.review)
                    {
                        return Err(durable::Error::Stale);
                    }
                    guard()?;
                    let review = lease.claim(&captured)?;
                    let reply = editor.owner.keep_editing(review, intent)?;
                    state.busy = true;
                    reply
                };
                let result = account::wait(reply).await;
                let mut state = editor.state.lock().unwrap();
                state.busy = false;
                if let Err(error) = result {
                    state.error = Some(error);
                }
                drop(state);
                editor.notify();
                result
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
}
