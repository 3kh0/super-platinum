//! Explicit successful Release/StartLocal observation, independent of Send.
use super::*;
pub(in super::super) fn same_lease(
    a: &durable::LocalWriterLease,
    b: &durable::LocalWriterLease,
) -> bool {
    a.scope() == b.scope()
        && a.writer_id() == b.writer_id()
        && a.generation() == b.generation()
        && a.initial_serial() == b.initial_serial()
}
impl Release {
    pub(in super::super) fn acknowledge_handoff(
        &self,
        expected: &durable::LocalWriterLease,
    ) -> Result<(), durable::Error> {
        let mut state = self.child.lock().map_err(|_| durable::Error::Storage)?;
        if let Some(acknowledged) = &state.acknowledged {
            return if same_lease(acknowledged, expected) {
                Ok(())
            } else {
                Err(durable::Error::Stale)
            };
        }
        if state.acknowledgment.is_none() {
            let completion = state.completion.as_ref().ok_or(durable::Error::Pending)?;
            let (actual, receipt) = completion.observe_handoff().map_err(ui)?;
            if !same_lease(&actual, expected) {
                return Err(durable::Error::Stale);
            }
            state.acknowledgment = Some(receipt);
        }
        let receipt = state.acknowledgment.take().ok_or(durable::Error::Pending)?;
        match receipt.acknowledge() {
            Ok(()) => {
                state.acknowledged = Some(expected.clone());
                Ok(())
            }
            Err((error, receipt)) => {
                state.acknowledgment = Some(receipt);
                Err(ui(error))
            }
        }
    }
}
