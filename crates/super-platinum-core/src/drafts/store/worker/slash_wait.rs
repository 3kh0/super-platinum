//! Retained receiver custody followed by this exact storage accounting terminal.
use super::*;

impl SlashCompletion {
    /// Per-holder test observation only: no queue, result or admission mutation.
    #[cfg(test)]
    pub(in crate::drafts::store::worker) fn test_wait_events(
        &self,
    ) -> mpsc::Receiver<&'static str> {
        let (send, receive) = mpsc::channel();
        let mut hook = self.held.wait_test.lock().unwrap();
        assert!(hook.is_none());
        *hook = Some(send);
        receive
    }
    #[cfg(test)]
    fn test_wait_event(&self, event: &'static str) {
        if let Some(send) = self.held.wait_test.lock().unwrap().as_ref() {
            let _ = send.send(event);
        }
    }
    fn wait_reply(&self) -> Result<(StepState, Option<u64>), Reason> {
        let mut s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
        loop {
            let state = s.poll();
            if state != StepState::Waiting || s.submitted.is_none() {
                return Ok((state, s.submitted));
            }
            if let Some(receiver) = s.receiver.take() {
                let id = s.submitted.ok_or(Reason::Incomplete)?;
                #[cfg(test)]
                self.test_wait_event("receiver-owned");
                drop(s);
                let result = receiver.recv();
                s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
                // Retry cannot replace an attempt while its result is absent.
                match result {
                    Ok(result) => {
                        if let Err(error) = &result {
                            s.failures.push((id, *error));
                        }
                        s.result = Some(result);
                    }
                    Err(_) => s.disconnected = true,
                }
                self.held.changed.notify_all();
            } else {
                // A different retained waiter owns the receiver. Its temporary
                // absence is not a disconnect, and no Held lock crosses recv.
                #[cfg(test)]
                self.test_wait_event("condvar-wait");
                s = self.held.changed.wait(s).map_err(|_| Reason::Incomplete)?;
            }
        }
    }
}

impl Worker {
    pub(in crate::drafts::store::worker) fn wait_slash_completion(
        &self,
        completion: &SlashCompletion,
    ) -> Result<StepState, Reason> {
        {
            let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
            self.exact_slash(&pending, completion)?;
        }
        loop {
            let (state, id) = completion.wait_reply()?;
            if !matches!(state, StepState::Ready | StepState::Failed(_)) {
                return Ok(state);
            }
            let id = id.ok_or(Reason::Incomplete)?;
            self.wait_accounted(&completion.key, id)?;
            let mut s = completion
                .held
                .status
                .lock()
                .map_err(|_| Reason::Incomplete)?;
            if s.submitted == Some(id) {
                return Ok(s.poll());
            }
            // An explicit owner retried while accounting was awaited. Follow
            // the actual replacement attempt, never return an old success.
        }
    }
}
