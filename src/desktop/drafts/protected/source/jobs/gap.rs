//! One pre-admitted cleanup child, retaining every real attempt and result.
use super::*;
use reply::ReplyJob;
const MAX_ATTEMPTS: usize = 16;

pub(super) struct Gap {
    account: Weak<Account>,
    runtime: tokio::runtime::Handle,
    state: Tracked<State>,
}
struct State {
    input: Option<(durable::Stream, durable::GapReason, u64)>,
    attempts: Vec<Arc<ReplyJob<()>>>,
    pending_error: Option<durable::Error>,
    starting: bool,
}
impl Gap {
    pub(super) fn new(account: &Arc<Account>, runtime: tokio::runtime::Handle) -> Arc<Self> {
        Arc::new(Self {
            account: Arc::downgrade(account),
            runtime,
            state: Tracked::new(State {
                input: None,
                attempts: vec![],
                pending_error: None,
                starting: false,
            }),
        })
    }
    pub(super) fn retire(self: &Arc<Self>, stream: durable::Stream, reason: durable::GapReason) {
        stream.revoke();
        let Some(account) = self.account.upgrade() else {
            return;
        };
        {
            let mut state = self.state.lock().unwrap();
            if state.input.is_some() {
                return;
            }
            // Same actual Account barrier; historical children do not replace
            // or remove a newer generation when their result arrives late.
            let generation = account.block_team(&stream, reason);
            state.input = Some((stream, reason, generation));
        }
        let _ = self.submit();
    }
    pub(super) fn submit(self: &Arc<Self>) -> Result<(), durable::Error> {
        let account = self.account.upgrade().ok_or(durable::Error::Stale)?;
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        let (stream, reason, generation) = state.input.clone().ok_or(durable::Error::Pending)?;
        if state.starting {
            return Err(durable::Error::Pending);
        }
        if let Some(previous) = state.attempts.last() {
            match previous.result()? {
                Some(Err(durable::Error::Storage)) => (),
                Some(Err(error)) => return Err(error),
                Some(Ok(())) => return Err(durable::Error::Stale),
                None => return Err(durable::Error::Pending),
            }
        }
        if state.attempts.len() == MAX_ATTEMPTS {
            return Err(durable::Error::Pending);
        }
        let job = match &account.owned {
            Some(route) => {
                ReplyJob::owned(route.operations.mark_gap(durable::owned_store::GapInput {
                    stream: stream.clone(),
                    reason,
                }))
            }
            None => match account.client() {
                Ok(client) => ReplyJob::new(client.mark_gap(stream.clone(), reason)),
                Err(error) => {
                    state.pending_error = Some(error);
                    return Err(error);
                }
            },
        };
        state.starting = true;
        state.attempts.push(job.clone());
        state.pending_error = None;
        let retained = Arc::downgrade(self);
        let team = stream.team().to_owned();
        // Cache the exact worker result first; then stamp child completion and
        // update only this generation's barrier. No success inferred from recv.
        let result = job.start(&self.runtime, move |result| {
            if let Some(retained) = retained.upgrade() {
                retained.state.lock().unwrap().starting = false;
                account.finish_gap(&team, generation, result.is_ok());
            }
        });
        if result.is_err() {
            state.starting = false;
        }
        result
    }
    pub(super) async fn ready(&self) -> Result<(), durable::Error> {
        let job = self
            .state
            .lock()
            .map_err(|_| durable::Error::Storage)?
            .attempts
            .last()
            .cloned()
            .ok_or(durable::Error::Pending)?;
        // Actual cached result plus callback completion; no delay/poll certificate.
        job.ready().await
    }
    pub(super) fn terminal(&self) -> Result<bool, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        Ok(!state.starting
            && state
                .attempts
                .last()
                .is_some_and(|job| matches!(job.result(), Ok(Some(Ok(()))))))
    }
    pub(super) fn generation(&self, team: &str) -> Option<u64> {
        self.state
            .lock()
            .ok()?
            .input
            .as_ref()
            .filter(|v| v.0.team() == team)
            .map(|v| v.2)
    }
    #[cfg(test)]
    pub(super) fn attempts(&self) -> Vec<Arc<ReplyJob<()>>> {
        self.state.lock().unwrap().attempts.clone()
    }
    pub(super) fn snapshot(&self) -> Result<Snapshot, ownership::Error> {
        let (stamp, jobs) = self.state.observe(|state| state.attempts.clone())?;
        let jobs = jobs
            .into_iter()
            .map(|job| Ok((job.clone(), job.stamp()?)))
            .collect::<Result<_, ownership::Error>>()?;
        let snapshot = Snapshot { stamp, jobs };
        self.validate(&snapshot)?;
        Ok(snapshot)
    }
    pub(super) fn validate(&self, snapshot: &Snapshot) -> Result<(), ownership::Error> {
        self.state.validate(&snapshot.stamp)?;
        for (job, stamp) in &snapshot.jobs {
            job.validate(stamp)?;
        }
        self.state.validate(&snapshot.stamp)
    }
}
pub(super) struct Snapshot {
    stamp: ownership::Stamp,
    jobs: Vec<(Arc<ReplyJob<()>>, ownership::Stamp)>,
}
