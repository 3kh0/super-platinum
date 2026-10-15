//! One retained cold load per physical owned route. No Current cutover permit.
use super::*;
use crate::drafts::store::protected::{LoadMode, Loaded, commands::Action};

type ResultValue = Result<Arc<Loaded>, Error>;
struct State {
    receiver: Option<mpsc::Receiver<Result<Loaded, Error>>>,
    result: Option<ResultValue>,
}
pub(in crate::drafts::store) struct ColdJob {
    state: Mutex<State>,
    admission_worker: std::sync::Weak<Worker>,
}
#[derive(Default)]
pub(super) struct Control {
    pub(super) binding: Option<Binding>,
    prior_activity: bool,
}
pub(super) struct Binding {
    identity: Arc<()>,
    job: Arc<ColdJob>,
}
impl ColdJob {
    fn observe_loaded(&self, result: &ResultValue) {
        if result.is_ok() {
            if let Some(worker) = self.admission_worker.upgrade() {
                if let Some(admission) = &mut worker.pending.lock().unwrap().admission {
                    admission.cold_ready = true;
                }
            }
        }
    }
    pub(in crate::drafts::store) fn wait(&self) -> ResultValue {
        let mut state = self.state.lock().map_err(|_| Error::Storage)?;
        if let Some(value) = &state.result {
            return value.clone();
        }
        let result = state
            .receiver
            .take()
            .ok_or(Error::Storage)?
            .recv()
            .map_err(|_| Error::Storage)
            .and_then(|value| value.map(Arc::new));
        self.observe_loaded(&result);
        state.result = Some(result.clone());
        result
    }
    pub(in crate::drafts::store) fn poll(&self) -> Result<Option<Arc<Loaded>>, Error> {
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(_) => return Err(Error::Storage),
        };
        if let Some(value) = &state.result {
            return value.clone().map(Some);
        }
        let result = match state.receiver.as_ref().ok_or(Error::Storage)?.try_recv() {
            Ok(value) => value.map(Arc::new),
            Err(mpsc::TryRecvError::Empty) => return Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Error::Storage),
        };
        state.receiver = None;
        self.observe_loaded(&result);
        state.result = Some(result.clone());
        result.map(Some)
    }
}
impl Queue {
    pub(super) fn admit_protected(&self, command: ProtectedCommand) {
        // An old Client is not a retained C2 caller, even if B refuses it before
        // enqueue. Preserve original action/revocation behavior; no false fence.
        if let Some(admission) = &mut self.worker.pending.lock().unwrap().admission {
            admission.unsupported = true;
        }
        if let Action::Load {
            mode: LoadMode::AfterRestart,
            reply,
        } = &command.action
        {
            let _ = reply.send(Err(Error::Stale));
            return;
        }
        let Ok(mut control) = self.initialization.lock() else {
            return;
        };
        if control.binding.is_none()
            && !matches!(
                &command.action,
                Action::Load {
                    mode: LoadMode::Current,
                    ..
                } | Action::Review { .. }
                    | Action::ReviewLocal { .. }
            )
        {
            // A cold claim must precede actual live admission, not merely be
            // the first use of initialize_cold(). Conservatively remember even
            // failed/uncertain submissions; no later wrapper can infer a drain.
            control.prior_activity = true;
        }
        // Serialize the enqueue itself with cold() so a command cannot pass
        // the history check, then arrive behind a falsely admitted cold load.
        let _ = self.submit(Request::Protected(Box::new(command)));
    }

    pub(in crate::drafts::store) fn cold(&self, identity: &Arc<()>) -> Result<Arc<ColdJob>, Error> {
        // Admission and retention precede enqueue. A failed enqueue or uncertain
        // load is terminal in this slot; no wrapper can mint another restart.
        let mut bound = self.initialization.lock().map_err(|_| Error::Storage)?;
        if let Some(old) = &bound.binding {
            return if Arc::ptr_eq(&old.identity, identity) {
                Ok(old.job.clone())
            } else {
                Err(Error::Stale)
            };
        }
        if self.proof.get().is_none() || bound.prior_activity {
            return Err(Error::Stale);
        }
        let (reply, receiver) = mpsc::channel();
        let job = Arc::new(ColdJob {
            admission_worker: Arc::downgrade(&self.worker),
            state: Mutex::new(State {
                receiver: Some(receiver),
                result: None,
            }),
        });
        bound.binding = Some(Binding {
            identity: identity.clone(),
            job: job.clone(),
        });
        // Only this private path can submit AfterRestart on an Owned route.
        let _ = self.submit(Request::Protected(Box::new(ProtectedCommand {
            account: self.account().to_owned(),
            action: Action::Load {
                mode: LoadMode::AfterRestart,
                reply,
            },
        })));
        Ok(job)
    }
}
