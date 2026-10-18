//! Local admission/submit refusal is distinct from an in-flight worker job.
use super::*;

#[derive(Default)]
pub(super) struct Retry {
    refused: bool,
}
impl Retry {
    pub(super) fn admission<I, T>(&mut self, held: &Held<I, T>) {
        self.refused = held.job().is_err();
    }
    pub(super) fn submission(
        &mut self,
        result: Result<(), o::AdmissionError>,
    ) -> Result<(), o::AdmissionError> {
        self.refused = result.is_err();
        result
    }
    pub(super) fn available<I: Send + Sync + 'static, T: Send + 'static>(
        &self,
        held: Option<&Arc<Held<I, T>>>,
    ) -> bool {
        held.is_some_and(|held| {
            self.refused
                || held.job().is_ok_and(|job| {
                    matches!(
                        job.state(),
                        o::JobState::Ready | o::JobState::Failed(durable::Error::Storage)
                    )
                })
        })
    }
    pub(super) fn readmit<I: Clone, T>(
        &mut self,
        original: &Arc<Held<I, T>>,
        admit: impl FnOnce(I) -> Result<o::StorageJob<I, T>, o::AdmissionRefused<I>>,
    ) -> Arc<Held<I, T>> {
        match &original.admitted {
            Ok(_) => original.clone(),
            // A refused admission never issued a worker job. The original
            // operations owner still revalidates scope/fence on this exact input.
            Err(refused) => {
                let next = Held::new(admit(refused.input.clone()));
                self.admission(&next);
                next
            }
        }
    }
}
