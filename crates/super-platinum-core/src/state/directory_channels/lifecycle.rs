//! Runtime-only native full-bootstrap observation. Metadata recovery is not boot.
//! The caller owns the original account/Transport lease; this subordinate token
//! prevents an older completion from clearing a newer full bootstrap.
use super::{Error, Result, WorkspaceSession, session_equal};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[derive(Clone)]
struct Owner {
    account: String,
    session: WorkspaceSession,
    epoch: u64,
}
impl std::fmt::Debug for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BootOwner")
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}
impl Owner {
    fn matches(&self, account: &str, session: &WorkspaceSession, epoch: u64) -> bool {
        self.account == account && self.epoch == epoch && session_equal(&self.session, session)
    }
}

/// Not serialized. Restored queued batches retain their already-observed bool,
/// but a restored cache cannot certify the current process's bootstrap phase.
#[derive(Clone, Debug, Default)]
pub(super) struct Lifecycle {
    owner: Option<Owner>,
    attempt: Option<Arc<AtomicU8>>,
}

/// Minted only at actual normal bootstrap entry, never by a picker or renderer.
pub struct BootLease {
    owner: Owner,
    attempt: Arc<AtomicU8>,
}

#[derive(Clone)]
pub(super) struct CapturedPhase {
    owner: Owner,
    attempt: Arc<AtomicU8>,
    in_flight: bool,
}
impl CapturedPhase {
    pub(super) fn value(&self) -> bool {
        self.in_flight
    }
}
impl Drop for BootLease {
    fn drop(&mut self) {
        // A cancelled native task supplies no completion observation. Retire
        // only its own attempt; a replacement owns a different atomic cell.
        let _ = self
            .attempt
            .compare_exchange(1, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

impl Lifecycle {
    pub(super) fn capture(
        &self,
        account: &str,
        session: &WorkspaceSession,
        epoch: u64,
    ) -> Result<CapturedPhase> {
        let in_flight = self.phase(account, session, epoch)?;
        Ok(CapturedPhase {
            owner: self.owner.clone().ok_or(Error::MissingObservation)?,
            attempt: self.attempt.clone().ok_or(Error::MissingObservation)?,
            in_flight,
        })
    }
    pub(super) fn accepts(
        &self,
        captured: &CapturedPhase,
        account: &str,
        session: &WorkspaceSession,
        epoch: u64,
    ) -> bool {
        captured.owner.matches(account, session, epoch)
            && self
                .owner
                .as_ref()
                .is_some_and(|v| v.matches(account, session, epoch))
            && self
                .attempt
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, &captured.attempt))
            && matches!(captured.attempt.load(Ordering::Acquire), 1 | 2)
    }
    pub(super) fn begin(
        &mut self,
        account: &str,
        session: &WorkspaceSession,
        epoch: u64,
    ) -> Result<BootLease> {
        if account.is_empty() || session.team_id.is_empty() || session.user_id.is_empty() {
            return Err(Error::MissingObservation);
        }
        let owner = Owner {
            account: account.into(),
            session: session.clone(),
            epoch,
        };
        let attempt = Arc::new(AtomicU8::new(1));
        self.owner = Some(owner.clone());
        self.attempt = Some(attempt.clone());
        Ok(BootLease { owner, attempt })
    }
    pub(super) fn finish(
        &mut self,
        lease: BootLease,
        account: &str,
        session: &WorkspaceSession,
        epoch: u64,
    ) -> bool {
        if !lease.owner.matches(account, session, epoch)
            || !self
                .owner
                .as_ref()
                .is_some_and(|o| o.matches(account, session, epoch))
            || !self
                .attempt
                .as_ref()
                .is_some_and(|n| Arc::ptr_eq(n, &lease.attempt))
        {
            return false;
        }
        lease.attempt.store(2, Ordering::Release);
        true
    }
    pub(super) fn phase(
        &self,
        account: &str,
        session: &WorkspaceSession,
        epoch: u64,
    ) -> Result<bool> {
        if !self
            .owner
            .as_ref()
            .is_some_and(|o| o.matches(account, session, epoch))
        {
            return Err(Error::MissingObservation);
        }
        match self.attempt.as_ref().map(|v| v.load(Ordering::Acquire)) {
            Some(1) => Ok(true),
            Some(2) => Ok(false),
            _ => Err(Error::MissingObservation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> WorkspaceSession {
        WorkspaceSession {
            team_id: "TONE".into(),
            user_id: "USELF".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            url: "https://fixture.slack.com".into(),
            token: "synthetic".into(),
        }
    }
    #[test]
    fn missing_process_observation_is_not_completed_boot() {
        assert!(Lifecycle::default().phase("A", &session(), 1).is_err());
    }
    #[test]
    fn older_completion_cannot_clear_a_newer_boot() {
        let mut s = Lifecycle::default();
        let w = session();
        let old = s.begin("A", &w, 1).unwrap();
        let new = s.begin("A", &w, 1).unwrap();
        assert!(!s.finish(old, "A", &w, 1));
        assert_eq!(s.phase("A", &w, 1), Ok(true));
        assert!(s.finish(new, "A", &w, 1));
        assert_eq!(s.phase("A", &w, 1), Ok(false));
    }
    #[test]
    fn account_epoch_and_credentials_retire_phase_observation() {
        let mut s = Lifecycle::default();
        let w = session();
        let lease = s.begin("A", &w, 1).unwrap();
        assert!(s.phase("B", &w, 1).is_err());
        assert!(s.phase("A", &w, 2).is_err());
        let mut replaced = w.clone();
        replaced.token = "rotated-synthetic".into();
        assert!(s.phase("A", &replaced, 1).is_err());
        assert!(!s.finish(lease, "A", &replaced, 1));
        assert!(s.phase("A", &w, 1).is_err());
    }
    #[test]
    fn cancelled_attempt_becomes_unknown_without_retiring_replacement() {
        let mut s = Lifecycle::default();
        let w = session();
        let cancelled = s.begin("A", &w, 1).unwrap();
        drop(cancelled);
        assert!(s.phase("A", &w, 1).is_err());
        let old = s.begin("A", &w, 1).unwrap();
        let current = s.begin("A", &w, 1).unwrap();
        drop(old);
        assert_eq!(s.phase("A", &w, 1), Ok(true));
        assert!(s.finish(current, "A", &w, 1));
        assert_eq!(s.phase("A", &w, 1), Ok(false));
    }
}
