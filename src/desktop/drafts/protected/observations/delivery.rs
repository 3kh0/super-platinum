//! Closed result handling: exact native paired projection or guarded hidden book.
use super::*;
use crate::state::ComposerTarget;
use crate::view::composer::protected_projection as native;
use dioxus::prelude::WritableExt;

pub(crate) struct Delivery {
    queue: Arc<ObservationQueue>,
    completed: Arc<Completed>,
}
impl ObservationQueue {
    pub fn take_delivery(self: &Arc<Self>) -> Option<Delivery> {
        let mut state = self.state.lock().ok()?;
        let completed = state.results.pop_front()?;
        self.source.inventory.clock.change();
        state.delivered.push(completed.clone());
        Some(Delivery {
            queue: self.clone(),
            completed,
        })
    }
}
impl Delivery {
    /// No caller-supplied acknowledgment, boolean, Review or copied receipt.
    /// Failure keeps the original Work and result in delivered inventory.
    pub async fn apply(self, mut state: Signal<ShellState>) -> Result<(), durable::Error> {
        let work = self.completed.work.as_ref().ok_or(durable::Error::Stale)?;
        let observed = self.completed.result.as_ref().map_err(|e| *e)?;
        if work.owner.account.owned.is_none() {
            return Err(durable::Error::Unsupported);
        }
        let (target, registry) = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            if !self.queue.source.current(&shell)
                || shell.core.active_account.as_deref()
                    != Some(work.owner.scope().account_id.as_str())
            {
                return Err(durable::Error::Stale);
            }
            let target = [ComposerTarget::Channel, ComposerTarget::Thread]
                .into_iter()
                .find(|target| {
                    shell.composer_key(*target).as_ref() == Some(&work.owner.scope().key)
                });
            (target, super::super::production::registry(&shell)?)
        };
        if let Some(barrier) = &observed.barrier {
            let editor = work.editor.as_ref().ok_or(durable::Error::Stale)?;
            let target = target.ok_or(durable::Error::Stale)?;
            let ticket = editor.prepare_projection(barrier.id()).await?;
            let attempt =
                match native::capture_projection(state, target, editor.clone(), ticket, registry)
                    .await
                {
                    native::ProjectionResult::Ready(value) => value,
                    native::ProjectionResult::Changed(value) => {
                        value.preserve(editor).await?;
                        return Err(durable::Error::ReviewRequired);
                    }
                    native::ProjectionResult::Deferred(_) => {
                        return Err(durable::Error::ReviewRequired);
                    }
                };
            let applied = match attempt.apply().await {
                native::ProjectionResult::Ready(value) => value,
                native::ProjectionResult::Changed(value) => {
                    value.preserve(editor).await?;
                    return Err(durable::Error::ReviewRequired);
                }
                native::ProjectionResult::Deferred(_) => {
                    return Err(durable::Error::ReviewRequired);
                }
            };
            editor.acknowledge_projection(applied.install()?).await?;
        } else if !same_local(observed.before.local(), observed.after.local()) {
            // An unregistered visible field needs actual adoption/native proof;
            // never overwrite it merely because this event was accepted by storage.
            if target.is_some() {
                return Err(durable::Error::ReviewRequired);
            }
            let mut shell = state.write();
            if !self.queue.source.current(&shell)
                || [ComposerTarget::Channel, ComposerTarget::Thread]
                    .into_iter()
                    .any(|target| {
                        shell.composer_key(target).as_ref() == Some(&work.owner.scope().key)
                    })
            {
                return Err(durable::Error::Stale);
            }
            let book = shell
                .core
                .drafts
                .items
                .get(&work.owner.scope().key)
                .cloned();
            if book
                .as_ref()
                .is_some_and(|v| !same_local(v, observed.before.local()))
                || (book.is_none() && !observed.before.local().empty())
            {
                return Err(durable::Error::Stale);
            }
            shell
                .core
                .drafts
                .project_clean(
                    &work.owner.scope().key,
                    book.as_ref(),
                    observed.after.local(),
                )
                .map_err(|_| durable::Error::Stale)?;
        }
        // No-op means local content did not change; it is not native-clear proof.
        // Other cases reached actual paired installation above. A subsequent view
        // change does not undo that handling, but core owner/fence still gates ack.
        let mut queue = self
            .queue
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        let index = queue
            .delivered
            .iter()
            .position(|v| Arc::ptr_eq(v, &self.completed))
            .ok_or(durable::Error::Stale)?;
        owned::acknowledge(&self.queue, work, observed)?;
        self.queue.source.inventory.clock.change();
        queue.delivered.remove(index);
        Ok(())
    }
}
