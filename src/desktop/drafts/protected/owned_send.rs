//! Same renderer-root operation, actual prebound Consume/Prepare/Claim/Record.
use super::owned_route::{Held, Route, o, ui};
use super::ownership::Tracked as Mutex;
use super::*;
use crate::drafts::network::{Binding, RegisteredMutation};

type Prepared = o::StorageJob<Arc<o::SendInput>, durable::DispatchTicket>;
type Claimed = o::StorageJob<Arc<o::SendInput>, durable::ClaimedDispatch>;
pub(super) struct Send {
    pub consume: Arc<Held<o::SendInput, durable::Review>>,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    prepared: Option<Prepared>,
    claimed: Option<Claimed>,
    adapter: Option<RegisteredMutation>,
    refusal: Option<o::AdmissionError>,
}
impl Send {
    /// Desktop holder membership only. Core job progression is a separate edge.
    pub(super) fn ownership_stamp(
        &self,
    ) -> Result<super::ownership::Stamp, super::ownership::Error> {
        self.state.observe(|_| ()).map(|(stamp, ())| stamp)
    }
    pub(super) fn validate_ownership(
        &self,
        stamp: &super::ownership::Stamp,
    ) -> Result<(), super::ownership::Error> {
        self.state.validate(stamp)
    }

    pub fn new(route: &Route, input: o::SendInput) -> Arc<Self> {
        Arc::new(Self {
            consume: Held::new(route.operations.consume_send(input)),
            state: Mutex::default(),
        })
    }
    pub async fn consumed(&self) -> Result<durable::Review, durable::Error> {
        self.consume.wait().await.map_err(ui)?;
        self.consume
            .job()
            .map_err(ui)?
            .result()
            .ok_or(durable::Error::Pending)?
    }
    async fn prepare(&self) -> Result<RegisteredMutation, o::AdmissionError> {
        let prepared = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| o::AdmissionError::Incomplete)?;
            if state.prepared.is_none() {
                state.prepared = Some(self.consume.job()?.prepare()?);
            }
            state
                .prepared
                .clone()
                .ok_or(o::AdmissionError::Incomplete)?
        };
        prepared.submit()?;
        wait_ticket(prepared.clone()).await?;
        let claimed = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| o::AdmissionError::Incomplete)?;
            if state.claimed.is_none() {
                state.claimed = Some(prepared.claim()?);
            }
            state.claimed.clone().ok_or(o::AdmissionError::Incomplete)?
        };
        claimed.submit()?;
        wait_ticket(claimed.clone()).await?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| o::AdmissionError::Incomplete)?;
        if state.adapter.is_none() {
            state.adapter = Some(RegisteredMutation::hold(claimed.completion()?));
        }
        state.adapter.clone().ok_or(o::AdmissionError::Incomplete)
    }
    pub async fn dispatch<F: Fn() -> bool>(
        &self,
        parent: &operation::Operation,
        binding: &Binding,
        now: fn() -> u64,
        current: F,
    ) -> Result<dispatch::Dispatched, durable::Error> {
        if !parent.current() {
            return Err(durable::Error::Stale);
        }
        let adapter = self.prepare().await.map_err(|e| {
            if let Ok(mut state) = self.state.lock() {
                state.refusal = Some(e);
            }
            ui(e)
        })?;
        let receipt = adapter.completion().receipt().map_err(ui)?;
        parent.retain(|s| s.receipt = Some(receipt.clone()))?;
        parent.phase(operation::Phase::Http)?;
        // Adapter is already retained in this parent's Send before first poll.
        // Its real ClaimedDispatch is handed out once; current is conjunctive.
        let run = adapter
            .run(binding, now(), || parent.current() && current())
            .await;
        if let Some(attempt) = adapter.attempt() {
            parent.retain(|s| s.attempt = Some(attempt))?;
        }
        run.map_err(ui)?;
        self.commit(parent, &adapter).await
    }
    async fn commit(
        &self,
        parent: &operation::Operation,
        adapter: &RegisteredMutation,
    ) -> Result<dispatch::Dispatched, durable::Error> {
        // Completion storage does not require fresh network/renderer authority.
        adapter.commit().map_err(ui)?;
        let completion = adapter.completion().clone();
        let review = tokio::task::spawn_blocking(move || completion.wait())
            .await
            .map_err(|_| durable::Error::Storage)?
            .map_err(ui)??;
        parent.retain(|s| s.recorded = Some(Arc::new(review.clone())))?;
        Ok(dispatch::Dispatched {
            review,
            receipt: adapter.completion().receipt().map_err(ui)?,
            attempt: adapter.attempt().ok_or(durable::Error::Pending)?,
        })
    }
    pub async fn retry_storage(
        &self,
        parent: &operation::Operation,
    ) -> Result<dispatch::Dispatched, durable::Error> {
        let adapter = self
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .adapter
            .clone()
            .ok_or(durable::Error::Pending)?;
        adapter.retry_storage().map_err(ui)?;
        self.commit(parent, &adapter).await
    }
    #[cfg(test)]
    pub(super) async fn dispatch_with<F, Fut>(
        &self,
        parent: &operation::Operation,
        binding: &Binding,
        send: F,
    ) -> Result<dispatch::Dispatched, durable::Error>
    where
        F: FnOnce(super_platinum_core::slack::PreparedRequest) -> Fut,
        Fut: std::future::Future<
                Output = Result<
                    serde_json::Value,
                    super_platinum_core::slack::transport::ProtectedDraftFailure,
                >,
            >,
    {
        if !parent.current() {
            return Err(durable::Error::Stale);
        }
        let adapter = self.prepare().await.map_err(ui)?;
        parent.retain(|s| s.receipt = adapter.completion().receipt().ok())?;
        parent.phase(operation::Phase::Http)?;
        let run = adapter.run_with(binding, 10, send).await;
        if let Some(attempt) = adapter.attempt() {
            parent.retain(|s| s.attempt = Some(attempt))?;
        }
        run.map_err(ui)?;
        self.commit(parent, &adapter).await
    }
}
async fn wait_ticket<I: std::marker::Send + Sync + 'static, T: std::marker::Send + 'static>(
    job: o::StorageJob<I, T>,
) -> Result<(), o::AdmissionError> {
    match tokio::task::spawn_blocking(move || job.wait())
        .await
        .map_err(|_| o::AdmissionError::Incomplete)??
    {
        o::JobState::Ready => Ok(()),
        // Actual domain error remains retained in the typed job. No next Claim
        // or transport is created; UI receives the explicit stopped route.
        _ => Err(o::AdmissionError::Stale),
    }
}
