//! Explicit disk-only retry of an already chosen baseline. No HTTP capability
//! is exercised, and an original receipt is not assumed to be current state.
use super::*;
use super::{
    actions::{Applied, ProjectionAuthority, ProjectionBarrier},
    editor::Editor,
    editor_proof::Capture,
};
use dioxus::prelude::{ReadableExt, Signal};
use super_platinum_core::drafts::store::sync::RemoteChoice;

pub(super) struct Retry {
    pub evidence: baseline::Evidence,
    pub choice: RemoteChoice,
    pub capture: Capture,
}
impl Editor {
    pub async fn retry_baseline_choice(
        self: &Arc<Self>,
        state: Signal<crate::state::ShellState>,
    ) -> Result<Box<Applied>, durable::Error> {
        let editor = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = editor.retry_choice_inner(state).await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    async fn retry_choice_inner(
        self: Arc<Self>,
        state: Signal<crate::state::ShellState>,
    ) -> Result<Box<Applied>, durable::Error> {
        let _gate = self.gate.lock().await;
        let retry = {
            let mut current = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if current.busy || current.running || current.released.is_some() || current.handoff_lost
            {
                return Err(durable::Error::Pending);
            }
            let retry = current.choice_retry.clone().ok_or(durable::Error::Stale)?;
            current.busy = true;
            retry
        };
        let result = async {
            let reply = {
                let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
                if !retry.evidence.binding.current(&shell) || !retry.evidence.source.current(&shell)
                {
                    return Err(durable::Error::Gap);
                }
                self.owner
                    .choose_baseline(retry.evidence.evidence.clone(), retry.choice.clone())?
            };
            let after = account::wait(reply).await?;
            let current = self.owner.review().await?;
            if !current.same(&after) {
                return Err(durable::Error::Stale);
            }
            Ok(after)
        }
        .await;
        let mut current = self.state.lock().unwrap();
        current.busy = false;
        let result = match result {
            Ok(after)
                if current
                    .choice_retry
                    .as_ref()
                    .is_some_and(|value| Arc::ptr_eq(value, &retry))
                    && current.writer.id() == retry.capture.writer.id()
                    && retry.capture.review.row().same_enrollment(after.row()) =>
            {
                current.review = after.clone();
                current.direct = after.row().state().local_preservation_intent().cloned();
                current.preserved = None;
                current.choice_retry = None;
                current.error = None;
                current.parked = true;
                if current
                    .pending
                    .as_ref()
                    .is_some_and(|(serial, _)| *serial == retry.capture.serial)
                {
                    current.pending = None;
                }
                let before = retry.capture.review.clone();
                let barrier = Arc::new(ProjectionBarrier {
                    capture: retry.capture.clone(),
                    before: before.clone(),
                    after: after.clone(),
                    id: barrier_id(),
                    renew_writer: true,
                    authority: Some(ProjectionAuthority {
                        source: retry.evidence.source.clone(),
                        binding: Some(retry.evidence.binding.clone()),
                    }),
                });
                current.barrier = Some(barrier.clone());
                Ok(Box::new(Applied {
                    before,
                    after,
                    serial: retry.capture.serial,
                    barrier: Some(barrier),
                    native_clear: NativeClear::NotAttempted,
                }))
            }
            Ok(_) => {
                current.error = Some(durable::Error::Stale);
                Err(durable::Error::Stale)
            }
            Err(error) => {
                current.error = Some(error);
                Err(error)
            }
        };
        drop(current);
        self.notify();
        result
    }
}
