//! Explicit status reads retain one-use admissions and completion-only retries.
use super::*;
use crate::{
    drafts::network::{Binding, Block, Readback},
    state::ShellState,
};
use dioxus::prelude::{ReadableExt, Signal};

pub(crate) enum CheckRun {
    Finished(Checked),
    Deferred { until: u64 },
}
pub(crate) struct Checked {
    pub completion: durable::CheckCompletion,
    pub receipt: durable::CheckReceipt,
    pub read: Arc<Readback>,
}
pub(super) struct DeferredCheck {
    ticket: durable::CheckTicket,
    recovery: durable::Recovery,
    binding: Binding,
    receipt: durable::Receipt,
}
impl Owner {
    /// Caller must surface this as an explicit recovery action. Loaded pending
    /// summaries alone never invoke it, and active local work is not abandoned.
    pub async fn take_over(&self) -> Result<durable::Recovery, durable::Error> {
        self.account.initialize().await?;
        let team = self.scope.key.team.clone();
        {
            let journal = self
                .account
                .journal
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if journal.busy_team(&team) {
                return Err(durable::Error::Pending);
            }
        }
        let current = account::wait(self.client()?.recovery_state(team.clone())).await?;
        let reply = {
            let journal = self
                .account
                .journal
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if journal.busy_team(&team) {
                return Err(durable::Error::Pending);
            }
            self.client()?.take_over(team, current)
        };
        account::wait(reply).await
    }
    pub async fn status_receipt(
        &self,
        review: durable::Review,
    ) -> Result<durable::Receipt, durable::Error> {
        self.account.initialize().await?;
        self.check(&review)?;
        let operation = review
            .row()
            .state()
            .pending()
            .cloned()
            .ok_or(durable::Error::Pending)?;
        account::wait(self.client()?.receipt(review.row().clone(), operation)).await
    }
    pub async fn check_status(
        &self,
        state: Signal<ShellState>,
        expected: durable::Review,
        receipt: durable::Receipt,
        recovery: durable::Recovery,
        binding: Binding,
        now: fn() -> u64,
    ) -> Result<CheckRun, durable::Error> {
        let active = journal::Active::begin(self)?;
        let owner = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let _active = active;
            let result = owner
                .check_inner(state, expected, receipt, recovery, binding, now)
                .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    async fn check_inner(
        &self,
        state: Signal<ShellState>,
        expected: durable::Review,
        receipt: durable::Receipt,
        recovery: durable::Recovery,
        binding: Binding,
        now: fn() -> u64,
    ) -> Result<CheckRun, durable::Error> {
        self.account.initialize().await?;
        self.check(&expected)?;
        if receipt.scope() != self.scope()
            || receipt.enrollment() != expected.row().lineage().enrollment()
        {
            return Err(durable::Error::Stale);
        }
        let key = (self.scope.key.clone(), receipt.operation().id.clone());
        {
            let journal = self
                .account
                .journal
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if journal.checks.contains_key(&key) || journal.deferred_checks.contains_key(&key) {
                return Err(durable::Error::Pending);
            }
        }
        if !state.try_peek().is_ok_and(|shell| binding.current(&shell)) {
            return Err(durable::Error::Stale);
        }
        let ticket = account::wait(self.client()?.reserve_check(
            recovery.clone(),
            expected,
            receipt.clone(),
            now(),
        ))
        .await?;
        self.claim_check(
            state,
            DeferredCheck {
                ticket,
                recovery,
                binding,
                receipt,
            },
            now,
        )
        .await
    }
    /// Explicit second action reuses the same unspent admission and captured
    /// authentication. There is no deadline timer which launches this method.
    pub async fn resume_check(
        &self,
        state: Signal<ShellState>,
        operation: &str,
        now: fn() -> u64,
    ) -> Result<CheckRun, durable::Error> {
        let (active, deferred) = journal::Active::resume_check(self, operation)?;
        let owner = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let _active = active;
            let result = owner.claim_check(state, deferred, now).await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    async fn claim_check(
        &self,
        state: Signal<ShellState>,
        deferred: DeferredCheck,
        now: fn() -> u64,
    ) -> Result<CheckRun, durable::Error> {
        let DeferredCheck {
            ticket,
            recovery,
            binding,
            receipt,
        } = deferred;
        let key = (self.scope.key.clone(), receipt.operation().id.clone());
        match account::wait(self.client()?.claim_check(recovery.clone(), ticket, now())).await? {
            durable::CheckClaim::Deferred { ticket, until } => {
                self.account
                    .journal
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .deferred_checks
                    .insert(
                        key,
                        DeferredCheck {
                            ticket,
                            recovery,
                            binding,
                            receipt,
                        },
                    );
                Ok(CheckRun::Deferred { until })
            }
            durable::CheckClaim::Ready {
                operation,
                completion,
            } => {
                if completion.scope() != self.scope()
                    || receipt.operation() != &operation
                    || completion.operation() != &operation
                    || completion.enrollment() != receipt.enrollment()
                {
                    return Err(durable::Error::Stale);
                }
                let current = recovery.is_live()
                    && state.try_peek().is_ok_and(|shell| binding.current(&shell));
                let read = Arc::new(if current {
                    binding
                        .check_protected_operation(&operation, now(), || {
                            recovery.is_live()
                                && state.try_peek().is_ok_and(|shell| binding.current(&shell))
                        })
                        .await
                } else {
                    Readback::blocked(Block::Session)
                });
                let completion = Arc::new(journal::Check {
                    completion,
                    result: outcome::check(&read),
                    read,
                });
                self.account
                    .journal
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .checks
                    .insert(key, completion.clone());
                self.commit_check(completion).await.map(CheckRun::Finished)
            }
        }
    }
    async fn commit_check(
        &self,
        completion: Arc<journal::Check>,
    ) -> Result<Checked, durable::Error> {
        if completion.completion.scope() != self.scope() {
            return Err(durable::Error::Stale);
        }
        let receipt = account::wait(self.client()?.complete_check(
            completion.completion.clone(),
            completion.result.clone(),
            completion.read.cooldown_until_ms.unwrap_or(0),
        ))
        .await?;
        let key = (
            self.scope.key.clone(),
            completion.completion.operation().id.clone(),
        );
        let mut journal = self
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if journal
            .checks
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, &completion))
        {
            journal.checks.remove(&key);
        }
        // The persisted positive_accepted flag alone reports exact confirmation;
        // superseded late evidence may be retained without a current review.
        Ok(Checked {
            completion: completion.completion.clone(),
            receipt,
            read: completion.read.clone(),
        })
    }
    pub async fn retry_check_storage(&self, operation: &str) -> Result<Checked, durable::Error> {
        let repair = journal::Repair::begin(self, operation, journal::RetainedKind::CheckStorage)?;
        let completion = self
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .checks
            .get(&(self.scope.key.clone(), operation.to_owned()))
            .cloned()
            .ok_or(durable::Error::Pending)?;
        let owner = self.clone();
        tokio::spawn(async move {
            let _repair = repair;
            owner.commit_check(completion).await
        })
        .await
        .map_err(|_| durable::Error::Storage)?
    }
    pub async fn supersede_lost_check(
        &self,
        recovery: durable::Recovery,
        expected: durable::Review,
        lost: durable::LostCheck,
        now: u64,
    ) -> Result<u64, durable::Error> {
        self.check(&expected)?;
        let reply = {
            let journal = self
                .account
                .journal
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if journal.busy_scope(self.scope()) {
                return Err(durable::Error::Pending);
            }
            self.client()?
                .supersede_check(recovery, expected, lost, now)
        };
        account::wait(reply).await
    }
    pub async fn accept_late_check(
        &self,
        expected: durable::Review,
        lost: durable::LostCheck,
    ) -> Result<durable::Review, durable::Error> {
        self.check(&expected)?;
        account::wait(self.client()?.accept_late_positive(expected, lost)).await
    }
}
