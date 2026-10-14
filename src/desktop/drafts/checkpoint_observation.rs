//! Serialized durable observations, deliberately disconnected from the DOM/RT.
use super::super::observation::{Observed, Prepared};
use super::*;

/// An immutable proposal, never proof that the native composer accepted it.
/// No API clears this barrier yet. A future typed native receipt must validate
/// its identity, scope, serial, storage revision and the actual native field.
#[derive(Debug)]
pub(crate) struct ProjectionBarrier {
    pub id: String,
    pub scope: Scope,
    pub serial: u64,
    pub before: View,
    /// None means the storage handoff failed and its resulting base is unknown.
    pub after: Option<View>,
}

fn same_predecessor(left: &View, right: &View) -> bool {
    same_local(&left.local, &right.local)
        && match (&left.stored, &right.stored) {
            (Some(left), Some(right)) => {
                left.state.scope() == right.state.scope()
                    && left.state.local_revision() == right.state.local_revision()
            }
            _ => false,
        }
}

impl Editor {
    /// Persist to the captured owner and finish the editor handoff even if the
    /// foreground waiter disappears. A returned observation is not permission
    /// to replace the visible field, clear the barrier, or dispatch a mutation.
    pub async fn observe_event(self: &Arc<Self>, prepared: Prepared) -> Result<Observed, Fault> {
        self.observe_using(move |owner| async move { owner.observe_event(prepared).await })
            .await
    }

    // Private injection boundary for deterministic await/cancellation tests.
    async fn observe_using<F, Fut>(self: &Arc<Self>, apply: F) -> Result<Observed, Fault>
    where
        F: FnOnce(Owner) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Observed, Fault>> + Send,
    {
        let editor = self.clone();
        self.runtime
            .spawn(async move { editor.observe_inner(apply).await })
            .await
            .map_err(|_| Fault::Storage)?
    }

    async fn observe_inner<F, Fut>(&self, apply: F) -> Result<Observed, Fault>
    where
        F: FnOnce(Owner) -> Fut,
        Fut: Future<Output = Result<Observed, Fault>>,
    {
        let (guard, before, serial) = loop {
            self.flush().await?;
            let guard = self.gate.lock().await;
            let captured = {
                let mut state = self.state.lock().unwrap();
                if state.running {
                    None
                } else {
                    if state.released
                        || state.settling
                        || state.return_handoff.is_some()
                        || state.projection.is_some()
                        || state.error.is_some()
                    {
                        return Err(state.error.unwrap_or(Fault::Stale));
                    }
                    state.observing = true;
                    Some((state.base.clone(), state.serial))
                }
            };
            if let Some((before, serial)) = captured {
                break (guard, before, serial);
            }
            // A run started between flush and gate acquisition. Let it finish.
            drop(guard);
        };
        self.changed.notify_waiters();
        let result = apply(self.owner.clone()).await;
        {
            let mut state = self.state.lock().unwrap();
            state.observing = false;
            let unchanged = result.as_ref().is_ok_and(|observed| {
                same_predecessor(&before, &observed.before)
                    && same_predecessor(&observed.before, &observed.after)
            });
            if let Ok(observed) = &result {
                state.base = observed.after.clone();
                state.direct_local = state
                    .direct_local
                    .take()
                    .filter(|intent| {
                        observed
                            .after
                            .stored
                            .as_ref()
                            .is_some_and(|stored| stored.state.accepts_direct_local(intent))
                    })
                    .or_else(|| {
                        observed
                            .after
                            .stored
                            .as_ref()
                            .and_then(local::direct_intent)
                    });
            }
            if !unchanged {
                state.projection = Some(Arc::new(ProjectionBarrier {
                    id: super_platinum_core::drafts::store::LocalWriter::default()
                        .id()
                        .into(),
                    scope: self.owner.scope.clone(),
                    serial,
                    before,
                    after: result.as_ref().ok().map(|observed| observed.after.clone()),
                }));
                state.direct_local = None;
                state.error = Some(result.as_ref().err().copied().unwrap_or(Fault::Stale));
                // The existing retained run saves pending input under this
                // editor's writer identity. The error also parks submissions
                // arriving AFTER this await: the DOM still has the old value.
            }
        }
        drop(guard);
        self.changed.notify_waiters();
        result
    }
}

#[cfg(test)]
#[path = "checkpoint_observation_tests.rs"]
mod tests;
