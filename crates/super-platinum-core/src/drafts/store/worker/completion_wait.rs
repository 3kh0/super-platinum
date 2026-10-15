//! Exact completed attempt stays retained even when its waiting task is detached.
use super::*;

impl Completion {
    fn wait_reply(&self) -> Result<(u64, Arc<Finished>), Reason> {
        let mut s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
        loop {
            let id = s.submitted.ok_or(Reason::Incomplete)?;
            if let Some(result) = &s.result {
                return Ok((id, result.clone()));
            }
            if s.disconnected {
                return Err(Reason::Incomplete);
            }
            if let Some(receiver) = s.receiver.take() {
                drop(s);
                let result = receiver.wait();
                s = self.held.status.lock().map_err(|_| Reason::Incomplete)?;
                match result {
                    Ok(result) => {
                        let result = Arc::new(result);
                        s.attempts.push((id, result.clone()));
                        s.result = Some(result);
                    }
                    Err(_) => s.disconnected = true,
                }
                self.held.changed.notify_all();
            } else {
                s = self.held.changed.wait(s).map_err(|_| Reason::Incomplete)?;
            }
        }
    }
}
impl Worker {
    pub(in crate::drafts::store::worker) fn wait_completion(
        &self,
        completion: &Completion,
    ) -> Result<Arc<Finished>, Reason> {
        {
            let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
            self.exact_completion(&pending, completion)?;
        }
        let (id, result) = completion.wait_reply()?;
        self.wait_accounted(&completion.key, id)?;
        Ok(result)
    }
}
