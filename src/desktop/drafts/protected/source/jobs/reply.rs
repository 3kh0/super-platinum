//! Actual Reply custody; a canceled async waiter never owns the sole result.
use super::*;
use tokio::sync::watch;

pub(super) struct ReplyJob<T> {
    state: Tracked<State<T>>,
    changed: watch::Sender<bool>,
}
struct State<T> {
    reply: Option<durable::Reply<T>>,
    running: bool,
    callback_done: bool,
    result: Option<Result<T, durable::Error>>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl<T: Clone + Send + 'static> ReplyJob<T> {
    pub(super) fn new(reply: durable::Reply<T>) -> Arc<Self> {
        Arc::new(Self {
            state: Tracked::new(State {
                reply: Some(reply),
                running: false,
                callback_done: false,
                result: None,
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
            let result = reply.wait();
            job.state.lock().unwrap().result = Some(result.clone());
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
