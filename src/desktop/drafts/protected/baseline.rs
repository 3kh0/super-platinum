//! Baseline read completion precedes a later separately guarded human choice.
use super::*;
use crate::{
    drafts::network::{BaselineRead, Binding},
    state::ShellState,
};
use dioxus::prelude::{ReadableExt, Signal};
pub(crate) enum BaselineRun {
    Evidence(Evidence),
    Nonpositive(Arc<BaselineRead>),
    Deferred { until: u64 },
}
#[derive(Clone)]
pub(crate) struct Evidence {
    pub(super) evidence: durable::BaselineEvidence,
    pub(super) binding: Binding,
    pub(super) source: Arc<source::Source>,
}
pub(super) struct DeferredBaseline {
    ticket: durable::BaselineTicket,
    binding: Binding,
    source: Arc<source::Source>,
    enrollment: String,
}
pub(super) struct Completion {
    completion: durable::BaselineCompletion,
    read: Arc<BaselineRead>,
    binding: Binding,
    source: Arc<source::Source>,
}
impl Owner {
    pub async fn read_baseline(
        &self,
        state: Signal<ShellState>,
        review: durable::Review,
        binding: Binding,
        source: Arc<source::Source>,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        let active = journal::Active::begin(self)?;
        let owner = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let _active = active;
            let result = owner
                .baseline_inner(state, review, binding, source, now)
                .await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    async fn baseline_inner(
        &self,
        state: Signal<ShellState>,
        review: durable::Review,
        binding: Binding,
        source: Arc<source::Source>,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        self.account.initialize().await?;
        self.check(&review)?;
        let stream = {
            let shell = state.try_peek().map_err(|_| durable::Error::Stale)?;
            if !binding.current(&shell) {
                return Err(durable::Error::Stale);
            }
            source.stream(&shell)?
        };
        self.check_stream(&stream)?;
        let enrollment = review.row().lineage().enrollment().to_owned();
        {
            let journal = self
                .account
                .journal
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if journal
                .deferred_baselines
                .contains_key(&(self.scope.key.clone(), enrollment.clone()))
                || journal
                    .baselines
                    .keys()
                    .any(|(key, _)| key == &self.scope.key)
            {
                return Err(durable::Error::Pending);
            }
        }
        let ticket = account::wait(self.client()?.reserve_baseline(stream, review, now())).await?;
        self.claim_baseline(
            state,
            DeferredBaseline {
                ticket,
                binding,
                source,
                enrollment,
            },
            now,
        )
        .await
    }
    async fn claim_baseline(
        &self,
        state: Signal<ShellState>,
        deferred: DeferredBaseline,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        let DeferredBaseline {
            ticket,
            binding,
            source,
            enrollment,
        } = deferred;
        match account::wait(self.client()?.claim_baseline(ticket, now())).await? {
            durable::BaselineClaim::Deferred { ticket, until } => {
                self.account
                    .journal
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .deferred_baselines
                    .insert(
                        (self.scope.key.clone(), enrollment.clone()),
                        DeferredBaseline {
                            ticket,
                            binding,
                            source,
                            enrollment,
                        },
                    );
                Ok(BaselineRun::Deferred { until })
            }
            durable::BaselineClaim::Ready {
                request,
                completion,
            } => {
                if request.scope() != self.scope()
                    || completion.scope() != self.scope()
                    || completion.enrollment() != enrollment
                {
                    return Err(durable::Error::Stale);
                }
                let current = state
                    .try_peek()
                    .is_ok_and(|shell| binding.current(&shell) && source.current(&shell));
                let read = Arc::new(if current {
                    binding
                        .read_protected_baseline(request, now(), || {
                            state.try_peek().is_ok_and(|shell| {
                                binding.current(&shell) && source.current(&shell)
                            })
                        })
                        .await
                } else {
                    BaselineRead::not_requested(None)
                });
                let key = (self.scope.key.clone(), completion.admission_id().to_owned());
                let completion = Arc::new(Completion {
                    completion,
                    read,
                    binding,
                    source,
                });
                self.account
                    .journal
                    .lock()
                    .map_err(|_| durable::Error::Storage)?
                    .baselines
                    .insert(key, completion.clone());
                self.commit_baseline(completion).await
            }
        }
    }
    async fn commit_baseline(
        &self,
        completion: Arc<Completion>,
    ) -> Result<BaselineRun, durable::Error> {
        if completion.completion.scope() != self.scope() {
            return Err(durable::Error::Stale);
        }
        let result = account::wait(self.client()?.retain_baseline(
            completion.completion.clone(),
            completion.read.response.clone(),
            completion.read.cooldown_until_ms.unwrap_or(0),
        ))
        .await?;
        let key = (
            self.scope.key.clone(),
            completion.completion.admission_id().to_owned(),
        );
        let mut journal = self
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?;
        if journal
            .baselines
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, &completion))
        {
            journal.baselines.remove(&key);
        }
        Ok(match result {
            durable::BaselineResult::Retained(evidence) => BaselineRun::Evidence(Evidence {
                evidence,
                binding: completion.binding.clone(),
                source: completion.source.clone(),
            }),
            durable::BaselineResult::Nonpositive => {
                BaselineRun::Nonpositive(completion.read.clone())
            }
        })
    }
    pub async fn resume_baseline(
        &self,
        state: Signal<ShellState>,
        enrollment: &str,
        now: fn() -> u64,
    ) -> Result<BaselineRun, durable::Error> {
        let (active, deferred) = journal::Active::resume_baseline(self, enrollment)?;
        let owner = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        dioxus::core::spawn_forever(async move {
            let _active = active;
            let result = owner.claim_baseline(state, deferred, now).await;
            let _ = send.send(result);
        });
        receive.await.map_err(|_| durable::Error::Storage)?
    }
    pub async fn retry_baseline_storage(
        &self,
        admission: &str,
    ) -> Result<BaselineRun, durable::Error> {
        let repair =
            journal::Repair::begin(self, admission, journal::RetainedKind::BaselineStorage)?;
        let completion = self
            .account
            .journal
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .baselines
            .get(&(self.scope.key.clone(), admission.to_owned()))
            .cloned()
            .ok_or(durable::Error::Pending)?;
        let owner = self.clone();
        tokio::spawn(async move {
            let _repair = repair;
            owner.commit_baseline(completion).await
        })
        .await
        .map_err(|_| durable::Error::Storage)?
    }
    pub async fn supersede_lost_baseline(
        &self,
        recovery: durable::Recovery,
        review: durable::Review,
        lost: durable::LostBaseline,
        now: u64,
    ) -> Result<u64, durable::Error> {
        self.check(&review)?;
        let reply = {
            let journal = self
                .account
                .journal
                .lock()
                .map_err(|_| durable::Error::Storage)?;
            if journal.active.values().any(|scope| scope == self.scope())
                || journal
                    .baselines
                    .keys()
                    .any(|(key, _)| key == &self.scope.key)
                || journal
                    .deferred_baselines
                    .keys()
                    .any(|(key, _)| key == &self.scope.key)
            {
                return Err(durable::Error::Pending);
            }
            self.client()?
                .supersede_baseline(recovery, review, lost, now)
        };
        account::wait(reply).await
    }
}
