//! Called only after the renderer handled the durable successful result.
use super::*;
impl Send {
    pub(in super::super) fn acknowledge_sent(
        &self,
        expected: &durable::Review,
    ) -> Result<(), durable::Error> {
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if let Some(acknowledged) = &state.acknowledged {
            return if acknowledged.same(expected) {
                Ok(())
            } else {
                Err(durable::Error::Stale)
            };
        }
        if state.acknowledgment.is_none() {
            let adapter = state.adapter.as_ref().ok_or(durable::Error::Pending)?;
            let (actual, receipt) = adapter.completion().observe_sent().map_err(ui)?;
            if !actual.same(expected) {
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
