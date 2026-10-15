//! Named enrollment forwarding; callers cannot independently reselect Discovery.
use super::*;
use crate::view::composer::protected_enrollment::Admission;
impl Host {
    pub fn enrollment(
        &self,
        shell: &ShellState,
        key: &DraftKey,
        visit: Visit,
    ) -> Result<discovery::EnrollmentRequest, durable::Error> {
        self.discovery.enrollment(shell, key, visit)
    }
    pub fn enqueue_enrollment(
        &mut self,
        prepared: discovery::PreparedEnrollment,
        shell: &ShellState,
        admission: &Admission,
    ) -> Result<discovery::Wait<durable::Review>, durable::Error> {
        let expected = prepared.expected();
        if !admission.matches(shell, expected.scope(), expected.local()) {
            return Err(durable::Error::Stale);
        }
        prepared.enqueue(&mut self.discovery, shell, |s| admission.current(s))
    }
    pub fn pending_enrollment(&self) -> Option<discovery::Wait<durable::Review>> {
        self.discovery.pending_enrollment()
    }
    pub fn take_enrollment(
        &mut self,
        wait: &discovery::Wait<durable::Review>,
    ) -> Result<discovery::EnrollmentOutcome, durable::Error> {
        self.discovery.take_enrollment(wait)
    }
}
