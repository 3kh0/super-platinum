//! Actual Reply custody; a canceled async waiter never owns the sole result.
use super::*;
use tokio::sync::watch;

pub(super) struct ReplyJob<T> {
    state: Tracked<State<T>>,
    changed: watch::Sender<bool>,
}
struct State<T> {
    reply: Option<Waiter<T>>,
    _owned: Option<Arc<dyn std::any::Any + Send + Sync>>,
    running: bool,
    callback_done: bool,
    result: Option<Result<T, durable::Error>>,
    retirement_error: Option<durable::owned_store::AdmissionError>,
    task: Option<tokio::task::JoinHandle<()>>,
}
enum Waiter<T> {
    Legacy(durable::Reply<T>),
    Owned(
        Box<
            dyn FnOnce() -> (
                    Result<T, durable::Error>,
                    Option<durable::owned_store::AdmissionError>,
                ) + Send,
        >,
    ),
}
impl<T: Send + 'static> Waiter<T> {
    fn wait(
        self,
    ) -> (
        Result<T, durable::Error>,
        Option<durable::owned_store::AdmissionError>,
    ) {
        match self {
            Self::Legacy(reply) => (reply.wait(), None),
            Self::Owned(wait) => wait(),
        }
    }
}
impl<T: Clone + Send + 'static> ReplyJob<T> {
    pub(super) fn owned<I: Send + Sync + 'static>(
        admitted: Result<
            durable::owned_store::StorageJob<I, T>,
            durable::owned_store::AdmissionRefused<I>,
        >,
    ) -> Arc<Self> {
        // Retain both actual job and exact refused input before the blocking
        // waiter starts. No ordinary Client, new executor, or success inference.
        let held = Arc::new(admitted);
        let retained = held.clone();
        let wait = Box::new(move || {
            let result = (|| {
                let job = retained
                    .as_ref()
                    .as_ref()
                    .map_err(|_| durable::Error::Unsupported)?;
                job.submit().map_err(|_| durable::Error::Unsupported)?;
                job.wait().map_err(|_| durable::Error::Unsupported)?;
                job.result().ok_or(durable::Error::Pending)?
            })();
            // Reclamation cannot replace an actual committed Stream result.
            // Even if a fence blocks acknowledgment, run the issued callback
            // and retain the job plus diagnostic for its actual cleanup path.
            let retirement_error = if result.is_ok() {
                let job = retained.as_ref().as_ref().expect("successful actual job");
                match job.observe_success() {
                    Ok((_, ack)) => ack.acknowledge().err().map(|(error, _)| error),
                    Err(error) => Some(error),
                }
            } else {
                None
            };
            (result, retirement_error)
        });
        Arc::new(Self {
            state: Tracked::new(State {
                reply: Some(Waiter::Owned(wait)),
                _owned: Some(held),
                running: false,
                callback_done: false,
                result: None,
                retirement_error: None,
                task: None,
            }),
            changed: watch::channel(false).0,
        })
    }

    pub(super) fn new(reply: durable::Reply<T>) -> Arc<Self> {
        Arc::new(Self {
            state: Tracked::new(State {
                reply: Some(Waiter::Legacy(reply)),
                _owned: None,
                running: false,
                callback_done: false,
                result: None,
                retirement_error: None,
                task: None,
            }),
            changed: watch::channel(false).0,
        })
    }
    pub(super) fn start(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        finished: impl FnOnce(Result<T, durable::Error>) + Send + 'static,
    ) -> Result<(), durable::Error> {
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if state.running || state.result.is_some() {
            return Err(durable::Error::Pending);
        }
        state.running = true;
        let job = self.clone();
        // Reply stays in the holder until this actual blocking task starts. A
        // stopped executor cannot silently discard an unpolled receiver.
        state.task = Some(runtime.spawn_blocking(move || {
            let reply = job
                .state
                .lock()
                .unwrap()
                .reply
                .take()
                .expect("one reply waiter");
            let (result, retirement_error) = reply.wait();
            {
                let mut state = job.state.lock().unwrap();
                state.result = Some(result.clone());
                state.retirement_error = retirement_error;
            }
            finished(result);
            job.state.lock().unwrap().callback_done = true;
            job.changed.send_replace(true);
        }));
        Ok(())
    }
    pub(super) fn result(&self) -> Result<Option<Result<T, durable::Error>>, durable::Error> {
        let state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        Ok(if state.callback_done {
            state.result.clone()
        } else {
            None
        })
    }
    pub(super) async fn ready(&self) -> Result<T, durable::Error> {
        let mut changes = self.changed.subscribe();
        loop {
            if let Some(result) = self.result()? {
                return result;
            }
            changes
                .changed()
                .await
                .map_err(|_| durable::Error::Storage)?;
        }
    }
    pub(super) fn stamp(&self) -> Result<ownership::Stamp, ownership::Error> {
        self.state.observe(|_| ()).map(|v| v.0)
    }
    pub(super) fn validate(&self, stamp: &ownership::Stamp) -> Result<(), ownership::Error> {
        self.state.validate(stamp)
    }
}
