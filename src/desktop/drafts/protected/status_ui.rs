//! Explicit protected recovery host. It never manufactures a receipt from a
//! displayed operation, releases Schedule/Update, or clears a native field.
use super::Owner;
use super::*;
use crate::{
    drafts::network::{Attempt, Binding},
    state::ShellState,
};
use dioxus::prelude::*;
use std::{cell::RefCell, rc::Rc};
use super_platinum_core::drafts::sync::{ConsumeIntent, OperationKind};

pub(crate) struct Status {
    state: Signal<ShellState>,
    owner: Owner,
    expected: durable::Review,
    binding: Binding,
    editor: Option<Arc<editor::Editor>>,
    registry: Arc<registry::Registry>,
    session: Rc<RefCell<Session>>,
    epoch: u64,
    queue_generation: u64,
    queue_scoped: bool,
}
pub(crate) enum StatusResult {
    Deferred {
        until: u64,
    },
    Completed {
        checked: Checked,
        published: bool,
        queue_published: bool,
        local_handoff: Option<Result<(), durable::Error>>,
    },
}
impl Status {
    pub fn capture(
        state: Signal<ShellState>,
        owner: Owner,
        expected: durable::Review,
        binding: Binding,
        registry: Arc<registry::Registry>,
        session: Rc<RefCell<Session>>,
    ) -> Result<Self, durable::Error> {
        owner.check(&expected)?;
        let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
        let scope = owner.scope();
        let epoch = shell.core.account_epoch;
        if !binding.current(&shell)
            || shell.core.active_account.as_deref() != Some(&scope.account_id)
            || session
                .try_borrow()
                .map_err(|_| durable::Error::Stale)?
                .entry(&scope.account_id, epoch, &scope.key)
                .and_then(|view| view.inventory().managed().ok())
                .is_none_or(|current| !current.same(&expected))
        {
            return Err(durable::Error::Stale);
        }
        let editor = registry.get(&scope.account_id, epoch, &scope.key);
        Ok(Self {
            state,
            expected,
            binding,
            editor,
            registry,
            session,
            epoch,
            queue_generation: shell.core.scheduled.generation,
            queue_scoped: shell.core.scheduled.matches_scope(epoch, &scope.key.team),
            owner,
        })
    }
    /// A user invoked Check status. Takeover and all durable/HTTP completion
    /// remain root-owned if the row or caller disappears.
    pub async fn check(self, now: fn() -> u64) -> Result<StatusResult, durable::Error> {
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = async {
                let receipt = self.owner.status_receipt(self.expected.clone()).await?;
                let recovery = self.owner.take_over().await?;
                match self
                    .owner
                    .check_status(
                        self.state,
                        self.expected.clone(),
                        receipt,
                        recovery,
                        self.binding.clone(),
                        now,
                    )
                    .await?
                {
                    CheckRun::Deferred { until } => Ok(StatusResult::Deferred { until }),
                    CheckRun::Finished(checked) => self.finish(checked).await,
                }
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    /// Explicit completion-only retry has no prepare/check/transport call.
    pub async fn retry_storage(self, operation_id: String) -> Result<StatusResult, durable::Error> {
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = async {
                let checked = self.owner.retry_check_storage(&operation_id).await?;
                self.finish(checked).await
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    /// Explicitly resume the original retained admission after its deadline.
    /// The journal transfers that ticket; this path never reserves a new check.
    pub async fn resume(self, now: fn() -> u64) -> Result<StatusResult, durable::Error> {
        let operation = self
            .expected
            .row()
            .state()
            .pending()
            .ok_or(durable::Error::Pending)?
            .id
            .clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let result = async {
                match self.owner.resume_check(self.state, &operation, now).await? {
                    CheckRun::Deferred { until } => Ok(StatusResult::Deferred { until }),
                    CheckRun::Finished(checked) => self.finish(checked).await,
                }
            }
            .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    fn current(&self, shell: &ShellState) -> bool {
        let scope = self.owner.scope();
        shell.core.active_account.as_deref() == Some(&scope.account_id)
            && shell.core.account_epoch == self.epoch
            && self.binding.current(shell)
            && same_editor(
                &self.editor,
                &self.registry.get(&scope.account_id, self.epoch, &scope.key),
            )
    }
    async fn finish(self, checked: Checked) -> Result<StatusResult, durable::Error> {
        let operation = checked.completion.operation();
        if checked.completion.scope() != self.owner.scope()
            || checked.completion.enrollment() != self.expected.row().lineage().enrollment()
            || self.expected.row().state().pending() != Some(operation)
        {
            return Err(durable::Error::Stale);
        }
        // Even an accepted positive receipt is historical. Only the exact
        // current enrollment and captured optional writer may publish it.
        let predecessor = self.session.try_borrow().ok().and_then(|session| {
            session
                .entry(
                    &operation.scope.account_id,
                    self.epoch,
                    &operation.scope.key,
                )
                .cloned()
        });
        let incoming = if let Some(editor) = &self.editor {
            editor
                .refresh_receipt()
                .await
                .ok()
                .map(|snapshot| snapshot.view)
        } else {
            self.owner
                .review()
                .await
                .ok()
                .map(|review| View::new(Inventory::Managed(review), None))
        };
        let mut published = false;
        let mut queue_published = false;
        if let (Some(before), Some(incoming)) = (predecessor, incoming) {
            let mut state = self.state;
            if let Ok(mut shell) = state.try_write() {
                if self.current(&shell) {
                    // Re-read latest pending metadata synchronously; never
                    // install an earlier background snapshot over new typing.
                    let latest = self
                        .editor
                        .as_ref()
                        .map(|editor| editor.snapshot().view)
                        .unwrap_or(incoming);
                    if let Ok(review) = latest.inventory().managed() {
                        let valid = self.expected.row().same_enrollment(review.row())
                            && checked
                                .receipt
                                .review()
                                .is_some_and(|receipt| receipt.row().same_enrollment(review.row()));
                        if valid {
                            if let Ok(mut session) = self.session.try_borrow_mut() {
                                published = session.apply(
                                    &operation.scope.account_id,
                                    self.epoch,
                                    &before,
                                    latest.clone(),
                                );
                            }
                            if published && checked.receipt.positive_accepted() {
                                if operation.kind == OperationKind::Send {
                                    crate::drafts::recovery::protected_reconcile(
                                        &mut shell,
                                        operation,
                                        &checked.read,
                                    );
                                } else if matches!(operation.kind, OperationKind::Schedule { .. }) {
                                    queue_published = self.queue(&mut shell, &checked, review);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Recovery does not use generic release. A positive Send can be handed
        // off only through an existing Editor and its exact completion marker;
        // absent/replaced writers stay managed for explicit local continuation.
        let mut local_handoff = None;
        if checked.receipt.positive_accepted() && operation.kind == OperationKind::Send {
            if let (Some(editor), Some(intent), Some(accepted)) = (
                &self.editor,
                self.expected.row().state().consumption().cloned(),
                checked.receipt.review(),
            ) {
                if self
                    .state
                    .try_peek()
                    .is_ok_and(|shell| self.current(&shell))
                {
                    let predecessor = self.session.try_borrow().ok().and_then(|session| {
                        session
                            .entry(
                                &operation.scope.account_id,
                                self.epoch,
                                &operation.scope.key,
                            )
                            .cloned()
                    });
                    let result = editor.release_completion(accepted.clone(), intent).await;
                    if result.is_ok() {
                        if let (Some(before), Ok(shell), Ok(mut session)) = (
                            predecessor,
                            self.state.try_peek(),
                            self.session.try_borrow_mut(),
                        ) {
                            if self.current(&shell) {
                                published |= session.apply(
                                    &operation.scope.account_id,
                                    self.epoch,
                                    &before,
                                    editor.snapshot().view,
                                );
                            }
                        }
                    }
                    local_handoff = Some(result);
                }
            }
        }
        Ok(StatusResult::Completed {
            checked,
            published,
            queue_published,
            local_handoff,
        })
    }
    fn queue(&self, shell: &mut ShellState, checked: &Checked, current: &durable::Review) -> bool {
        let operation = checked.completion.operation();
        let Some(intent) = self.expected.row().state().consumption() else {
            return false;
        };
        if !completion_lineage(current, intent)
            || shell.core.scheduled.generation != self.queue_generation
            || shell
                .core
                .scheduled
                .matches_scope(self.epoch, &operation.scope.key.team)
                != self.queue_scoped
        {
            return false;
        }
        let Some(outcome) = checked.read.outcome.clone() else {
            return false;
        };
        let evidence = Attempt {
            outcome,
            cooldown_until_ms: checked.read.cooldown_until_ms,
            issue: checked.read.issue.clone(),
            dispatched: false,
            raw: checked.read.raw.clone(),
        };
        let Some(message) =
            crate::drafts::schedule::protected_scheduled_message(operation, &evidence)
        else {
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
            .ensure_scope(self.epoch, &operation.scope.key.team);
        shell.core.scheduled.accept_remote(message);
        true
    }
}
fn completion_lineage(review: &durable::Review, intent: &ConsumeIntent) -> bool {
    review.row().state().consumption().is_none()
        && review.row().state().local_preservation_intent() == Some(intent)
}
fn same_editor(a: &Option<Arc<editor::Editor>>, b: &Option<Arc<editor::Editor>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
