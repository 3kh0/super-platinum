//! Read-only present-state observation for the separately gated restart checker.
use super::*;

#[derive(serde::Serialize)]
pub(in crate::file_share) struct Quiescence {
    pub jobs: usize,
    pub scopes: usize,
    pub running: usize,
}
impl Quiescence {
    pub(in crate::file_share) fn idle(&self) -> bool {
        self.jobs == 0 && self.scopes == 0 && self.running == 0
    }
}
impl Queue {
    pub(in crate::file_share) fn diagnostic(&self) -> Result<Quiescence, Refusal> {
        let inner = self.0.lock().map_err(|_| Refusal::Storage)?;
        Ok(Quiescence {
            jobs: inner.jobs.len(),
            scopes: inner.scopes.len(),
            running: inner.running.len(),
        })
    }
}
