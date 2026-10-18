//! One final claim and captured authentication check before one network attempt.
use super::*;
use crate::{
    drafts::{
        dispatch::Outcome,
        network::{Attempt, Binding, Block, Issue},
    },
    state::ShellState,
};
use dioxus::prelude::{ReadableExt, Signal};

pub(crate) struct Dispatched {
    pub review: durable::Review,
    pub receipt: durable::Receipt,
    pub attempt: Arc<Attempt>,
}
impl Owner {
    pub async fn dispatch_owned(
        &self,
        state: Signal<ShellState>,
        expected: durable::Review,
        source: Arc<source::Source>,
        binding: Binding,
        now_ms: fn() -> u64,
        operation: operation::Operation,
    ) -> Result<Dispatched, durable::Error> {
        if !operation.belongs_owner(self) {
            return Err(durable::Error::Stale);
        }
        operation.phase(operation::Phase::Dispatch)?;
        let _active = journal::Active::begin(self)?;
        self.dispatch_inner(state, expected, source, binding, now_ms, operation)
            .await
    }
    async fn dispatch_inner(
        &self,
        state: Signal<ShellState>,
        expected: durable::Review,
        source: Arc<source::Source>,
        binding: Binding,
        now_ms: fn() -> u64,
        parent: operation::Operation,
    ) -> Result<Dispatched, durable::Error> {
        self.account.initialize().await?;
        self.check(&expected)?;
        if self.account.owned.is_some() {
            let held = parent
                .record()
                .evidence
                .lock()
                .map_err(|_| durable::Error::Storage)?
                .owned_send
                .clone()
                .ok_or(durable::Error::Unsupported)?;
            let consumed = held
                .consume
                .job()
                .map_err(super::owned_route::ui)?
                .result()
                .ok_or(durable::Error::Pending)??;
            if !consumed.same(&expected) {
                return Err(durable::Error::Stale);
            }
            return held
                .dispatch(&parent, &binding, now_ms, || {
                    state
                        .try_peek()
                        .is_ok_and(|shell| binding.current(&shell) && source.current(&shell))
                })
                .await;
        }
        // Typing beside a frozen Send/Schedule is independently durable. Only
        // that exact retained intent may rebase preparation onto the newer
        // local inventory; ordinary Create/Update/Delete stay exact-review.
        let expected = if let Some(intent) = expected.row().state().consumption() {
            let fresh = self.review().await?;
            if !expected.row().same_enrollment(fresh.row())
                || fresh.row().state().consumption() != Some(intent)
                || fresh.row().state().pending().is_some()
                || !editor_proof::clean(&fresh)
            {
                return Err(durable::Error::Stale);
            }
            fresh
        } else {
            expected
        };
        let stream = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            if !binding.current(&shell) {
                return Err(durable::Error::Gap);
            }
            source.stream(&shell)?
        };
        self.check_stream(&stream)?;
        let ticket = account::wait(self.client()?.prepare(stream, expected)).await?;
        let claimed = account::wait(self.client()?.claim_dispatch(ticket)).await?;
        let (operation, receipt) = claimed.into_parts();
        if receipt.scope() != self.scope() || receipt.operation() != &operation {
            return Err(durable::Error::Stale);
        }
        parent.retain(|s| s.receipt = Some(receipt.clone()))?;
        parent.phase(operation::Phase::Http)?;
        // No await between this final captured-source/Binding admission and
        // entering the existing one-attempt network adapter.
        let current = state.try_peek().is_ok_and(|shell| {
            parent.current() && binding.current(&shell) && source.current(&shell)
        });
        let attempt = Arc::new(if current {
            binding
                .execute_protected(&operation, now_ms(), || {
                    state.try_peek().is_ok_and(|shell| {
                        parent.current() && binding.current(&shell) && source.current(&shell)
                    })
                })
                .await
        } else {
            Attempt {
                outcome: Outcome::Uncertain,
                cooldown_until_ms: None,
                issue: Some(Issue::Blocked(Block::Session)),
                dispatched: false,
                raw: None,
            }
        });
        parent.retain(|s| s.attempt = Some(attempt.clone()))?;
        // An in-flight original response is retained even after root retirement.
        let completion = Arc::new(journal::Mutation {
            receipt,
            attempt,
            parent: Some(parent.clone()),
        });
        let key = (self.scope.key.clone(), operation.id);
        self.account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .mutations
            .insert(key, completion.clone());
        self.commit_mutation(completion).await
    }
    async fn commit_mutation(
        &self,
        completion: Arc<journal::Mutation>,
    ) -> Result<Dispatched, durable::Error> {
        if completion.receipt.scope() != self.scope() {
            return Err(durable::Error::Stale);
        }
        let review = account::wait(self.client()?.record(
            completion.receipt.clone(),
            outcome::mutation(&completion.attempt.outcome),
            completion.attempt.cooldown_until_ms.unwrap_or(0),
        ))
        .await?;
        if let Some(parent) = &completion.parent {
            parent.retain(|s| s.recorded = Some(Arc::new(review.clone())))?;
        }
        // Core Ok means this exact receipt/outcome applied; it never substitutes
        // an unrelated Idle row. The returned proof still is not DOM authority.
        let key = (
            self.scope.key.clone(),
            completion.receipt.operation().id.clone(),
        );
        let mut journal = self
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if journal
            .mutations
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, &completion))
        {
            journal.mutations.remove(&key);
        }
        Ok(Dispatched {
            review,
            receipt: completion.receipt.clone(),
            attempt: completion.attempt.clone(),
        })
    }
    pub async fn retry_mutation_storage(
        &self,
        operation_id: &str,
    ) -> Result<Dispatched, durable::Error> {
        if self.account.owned.is_some() {
            for parent in self.account.retained_operations()? {
                let held = {
                    let record = parent.record();
                    let evidence = record
                        .evidence
                        .lock()
                        .map_err(|_| durable::Error::Storage)?;
                    if record.scope == self.scope
                        && evidence
                            .receipt
                            .as_ref()
                            .is_some_and(|r| r.operation().id == operation_id)
                    {
                        evidence.owned_send.clone()
                    } else {
                        None
                    }
                };
                if let Some(held) = held {
                    return held.retry_storage(&parent).await;
                }
            }
            return Err(durable::Error::Pending);
        }
        let repair =
            journal::Repair::begin(self, operation_id, journal::RetainedKind::MutationStorage)?;
        let completion = self
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .mutations
            .get(&(self.scope.key.clone(), operation_id.to_owned()))
            .cloned()
            .ok_or(durable::Error::Pending)?;
        let owner = self.clone();
        // Entire disk commit is detached; this closure contains no Binding or
        // HTTP execution capability, even when the caller disappears again.
        tokio::spawn(async move {
            let _repair = repair;
            owner.commit_mutation(completion).await
        })
        .await
        .map_err(|_| durable::Error::Storage)?
    }
}
