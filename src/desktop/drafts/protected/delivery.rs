//! Explicit, detached Send/Schedule handoff. This module is not a selector.
//! All storage authority stays opaque; only the existing pure message/queue
//! projection helpers are shared with the legacy controller.
use super::Action;
use super::*;
use crate::{
    drafts::{dispatch::Outcome, network::Binding},
    state::{MainView, PendingSend, ShellState},
    view::composer::protected_actions::Prepared,
};
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc};
use super_platinum_core::drafts::sync::{ConsumeIntent, ConsumeKind};

pub(crate) use super::operation::Request;
pub(crate) struct Finished {
    pub dispatched: Dispatched,
    pub inventory_published: bool,
    pub queue_published: bool,
    pub local_handoff: Result<(), durable::Error>,
}
/// A caller must supply the protected session and its registered Editor. No
/// conversion from the legacy managed_drafts or draft_writes maps exists.
pub(crate) struct Delivery {
    state: Signal<ShellState>,
    session: Rc<RefCell<Session>>,
    registry: Arc<registry::Registry>,
    editor: Arc<editor::Editor>,
    source: Arc<Source>,
    binding: Binding,
    epoch: u64,
    view: MainView,
    channel_generation: u64,
    thread_generation: u64,
    queue_generation: u64,
    queue_scoped: bool,
    initial: View,
    operation: Option<operation::Operation>,
}
impl Delivery {
    pub fn capture(
        state: Signal<ShellState>,
        session: Rc<RefCell<Session>>,
        registry: Arc<registry::Registry>,
        editor: Arc<editor::Editor>,
        source: Arc<Source>,
        binding: Binding,
    ) -> Result<Self, durable::Error> {
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let epoch = shell.core.account_epoch;
        let scope = editor.scope();
        let snapshot = editor.snapshot();
        if !source.current(&shell)
            || !binding.current(&shell)
            || !registry.current(&scope.account_id, epoch, &scope.key, &editor)
            || session
                .try_borrow()
                .map_err(|_| durable::Error::Stale)?
                .entry(&scope.account_id, epoch, &scope.key)
                .is_none_or(|view| !view.same(&snapshot.view))
        {
            return Err(durable::Error::Stale);
        }
        let captured = Self {
            state,
            session,
            registry,
            source,
            binding,
            epoch,
            view: shell.main_view,
            channel_generation: shell.channel_generation,
            thread_generation: shell.thread_generation,
            queue_generation: shell.core.scheduled.generation,
            queue_scoped: shell.core.scheduled.matches_scope(epoch, &scope.key.team),
            editor: editor.clone(),
            initial: snapshot.view,
            operation: None,
        };
        Ok(captured)
    }
    /// The whole action, including consumption, HTTP and result handoff, belongs
    /// to the root task. Dropping a dialog waiter cannot cancel these steps.
    // Keep construction of the large delivery future outside its caller's poll
    // frame. The existing root still owns and polls this exact future.
    #[inline(never)]
    pub(crate) fn run_owned(
        self,
        prepared: Prepared,
        request: Request,
        now: fn() -> u64,
    ) -> std::pin::Pin<Box<impl std::future::Future<Output = Result<Arc<Finished>, durable::Error>>>>
    {
        Box::pin(self.run_inner(prepared, request, now))
    }
    async fn run_inner(
        mut self,
        prepared: Prepared,
        request: Request,
        now: fn() -> u64,
    ) -> Result<Arc<Finished>, durable::Error> {
        let operation = prepared.operation().clone();
        self.operation = Some(operation.clone());
        if !prepared.belongs_to(&self.editor)
            || !self
                .state
                .try_peek()
                .is_ok_and(|shell| self.visit_current(&shell))
        {
            return Err(durable::Error::Stale);
        }
        let local = prepared.visible().clone();
        let action = match request {
            Request::Send => Action::Consume(ConsumeKind::Send),
            Request::Schedule { snapshot, post_at } => Action::Schedule { snapshot, post_at },
        };
        let applied = prepared
            .perform(
                action,
                Some(self.source.clone()),
                Some(self.binding.clone()),
            )
            .await?;
        #[cfg(feature = "protected-native-fixture")]
        super::owned_native::after_clear(&operation, &applied).await?;
        let intent = applied
            .after
            .row()
            .state()
            .consumption()
            .cloned()
            .ok_or(durable::Error::Pending)?;
        if intent.scope != *self.editor.scope() {
            return Err(durable::Error::Stale);
        }
        let initial_published = self.publish_snapshot(Some(self.initial.clone()));
        operation.retain(|s| s.publication.initial_inventory = Some(initial_published))?;
        let pending = if intent.kind == ConsumeKind::Send {
            let mut state = self.state;
            let mut shell = state.write();
            self.owner_current(&shell)
                .then(|| crate::drafts::send::protected_append_pending(&mut shell, &intent, &local))
                .flatten()
        } else {
            None
        };
        operation.retain(|s| s.publication.pending_send = Some(pending.is_some()))?;
        let result = self
            .editor
            .owner
            .dispatch_owned(
                self.state,
                applied.after,
                self.source.clone(),
                self.binding.clone(),
                now,
                operation.clone(),
            )
            .await;
        let dispatched = match result {
            Ok(result) => result,
            Err(error) => {
                // Storage uncertainty is not a clean rejection, and this path
                // cannot call dispatch again. The account retains its receipt.
                let invoked = self.project_send(
                    pending.as_ref(),
                    &intent,
                    &local,
                    &Outcome::Uncertain,
                    false,
                );
                operation.retain(|s| s.publication.send_helper_invoked = Some(invoked))?;
                return Err(error);
            }
        };
        let invoked = self.project_send(
            pending.as_ref(),
            &intent,
            &local,
            &dispatched.attempt.outcome,
            true,
        );
        operation.retain(|s| s.publication.send_helper_invoked = Some(invoked))?;
        let mut inventory_published = false;
        operation.phase(operation::Phase::Publication)?;
        let mut queue_published = false;
        let predecessor = self.snapshot_predecessor();
        if self.editor.refresh_receipt().await.is_ok() {
            inventory_published = self.publish_snapshot(predecessor);
            if inventory_published {
                queue_published = self.project_queue(&intent, &dispatched);
            }
        }
        operation.retain(|s| {
            s.publication.inventory = Some(inventory_published);
            s.publication.queue = Some(queue_published);
        })?;
        // Only the exact accepted Send receipt may release into local writing.
        // Update/Schedule and unconfirmed/clean rejection never take this path.
        let local_handoff = if matches!(dispatched.attempt.outcome, Outcome::Sent { .. }) {
            let predecessor = self.snapshot_predecessor();
            let result = self
                .editor
                .release_completion_owned(
                    dispatched.review.clone(),
                    intent.clone(),
                    operation.clone(),
                )
                .await;
            let published = result.is_ok() && self.publish_snapshot(predecessor);
            inventory_published |= published;
            operation.retain(|s| s.publication.after_release_inventory = Some(published))?;
            result
        } else {
            Ok(())
        };
        Ok(Arc::new(Finished {
            dispatched,
            inventory_published,
            queue_published,
            local_handoff,
        }))
    }
    fn owner_current(&self, shell: &ShellState) -> bool {
        let scope = self.editor.scope();
        self.operation
            .as_ref()
            .is_some_and(operation::Operation::current)
            && shell.core.active_account.as_deref() == Some(&scope.account_id)
            && shell.core.account_epoch == self.epoch
            && self
                .registry
                .current(&scope.account_id, self.epoch, &scope.key, &self.editor)
            && self.source.current(shell)
            && self.binding.current(shell)
    }
    fn visit_current(&self, shell: &ShellState) -> bool {
        self.owner_current(shell)
            && shell.main_view == self.view
            && shell.channel_generation == self.channel_generation
            && shell.thread_generation == self.thread_generation
    }
    fn snapshot_predecessor(&self) -> Option<View> {
        let shell = self.state.try_peek().ok()?;
        if !self.owner_current(&shell) {
            return None;
        }
        let scope = self.editor.scope();
        self.session
            .try_borrow()
            .ok()?
            .entry(&scope.account_id, self.epoch, &scope.key)
            .cloned()
    }
    fn publish_snapshot(&self, before: Option<View>) -> bool {
        let Some(before) = before else { return false };
        let Ok(shell) = self.state.try_peek() else {
            return false;
        };
        if !self.owner_current(&shell) {
            return false;
        }
        let scope = self.editor.scope();
        let Ok(mut session) = self.session.try_borrow_mut() else {
            return false;
        };
        // Snapshot is read at the final synchronous publication boundary. A
        // previously returned background snapshot never overwrites new input.
        let latest = self.editor.snapshot();
        session.apply(&scope.account_id, self.epoch, &before, latest.view)
    }
    fn project_send(
        &self,
        pending: Option<&PendingSend>,
        intent: &ConsumeIntent,
        local: &LocalDraft,
        outcome: &Outcome,
        accepted: bool,
    ) -> bool {
        let Some(pending) = pending else { return false };
        let mut state = self.state;
        let Ok(mut shell) = state.try_write() else {
            return false;
        };
        if self.owner_current(&shell) {
            crate::drafts::send::protected_apply_outcome(
                &mut shell, pending, intent, local, outcome, accepted,
            );
            true
        } else {
            false
        }
    }
    fn project_queue(&self, intent: &ConsumeIntent, dispatched: &Dispatched) -> bool {
        let mut state = self.state;
        let Ok(mut shell) = state.try_write() else {
            return false;
        };
        let latest = self.editor.snapshot();
        let Ok(review) = latest.view.inventory().managed() else {
            return false;
        };
        if !self.visit_current(&shell)
            || !dispatched.review.row().same_enrollment(review.row())
            || review.row().state().local_preservation_intent() != Some(intent)
            || shell.core.scheduled.generation != self.queue_generation
            || shell
                .core
                .scheduled
                .matches_scope(self.epoch, &intent.scope.key.team)
                != self.queue_scoped
        {
            return false;
        }
        let Some(message) = crate::drafts::schedule::protected_scheduled_message(
            dispatched.receipt.operation(),
            &dispatched.attempt,
        ) else {
            return false;
        };
        if !crate::drafts::schedule::protected_advances_queue(
            shell.core.scheduled.items.get(&message.id),
            &message,
        ) {
            return false;
        }
        shell
            .core
            .scheduled
            .ensure_scope(self.epoch, &intent.scope.key.team);
        shell.core.scheduled.accept_remote(message);
        true
    }
}
