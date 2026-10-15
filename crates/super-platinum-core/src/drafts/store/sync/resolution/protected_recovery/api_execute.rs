//! Bounded FIFO dispatch frames; transaction branch bodies are unchanged.
use super::*;
#[path = "api_execute_baseline.rs"]
mod handlers_baseline;
#[path = "api_execute_local.rs"]
mod handlers_local;
#[path = "api_execute_mutations.rs"]
mod handlers_mutations;
#[path = "api_execute_observations.rs"]
mod handlers_observations;
#[path = "api_execute_queries.rs"]
mod handlers_queries;
#[path = "api_execute_recovery.rs"]
mod handlers_recovery;
#[path = "api_execute_streams.rs"]
mod handlers_streams;
#[cfg(any(test, feature = "protected-test-support"))]
#[path = "api_execute_support.rs"]
mod handlers_support;
#[path = "api_execute_writers.rs"]
mod handlers_writers;
impl Command {
    #[inline(never)]
    pub(crate) fn execute(
        self: Box<Self>,
        open: &dyn Fn(&str) -> Result<DraftStore, AppError>,
        pending: BTreeMap<DraftKey, LocalDraft>,
        isolated: bool,
    ) -> crate::drafts::store::terminal::Report {
        // Inspect the tag by reference: moving Action here would put its largest
        // payload back in this frame before the selected handler can consume it.
        match &self.action {
            Action::Load { .. } => handlers_queries::execute(self, open, pending, isolated),
            Action::Review { .. } => handlers_queries::execute(self, open, pending, isolated),
            Action::ReviewLocal { .. } => handlers_queries::execute(self, open, pending, isolated),
            Action::EnrollLocal { .. } => handlers_queries::execute(self, open, pending, isolated),
            Action::RegisterWriter { .. } => {
                handlers_queries::execute(self, open, pending, isolated)
            }
            Action::Receipt { .. } => handlers_queries::execute(self, open, pending, isolated),
            Action::Checkpoint { .. } => handlers_writers::execute(self, open, pending, isolated),
            Action::PreserveVersion { .. } => {
                handlers_writers::execute(self, open, pending, isolated)
            }
            Action::Clear { .. } => handlers_writers::execute(self, open, pending, isolated),
            Action::SettleLocal { .. } => handlers_writers::execute(self, open, pending, isolated),
            Action::StreamState { .. } => handlers_streams::execute(self, open, pending, isolated),
            Action::OpenStream { .. } => handlers_streams::execute(self, open, pending, isolated),
            Action::MarkGap { .. } => handlers_streams::execute(self, open, pending, isolated),
            Action::VerifyLocal { .. } => handlers_streams::execute(self, open, pending, isolated),
            Action::Consume { .. } => handlers_mutations::execute(self, open, pending, isolated),
            Action::ReviewedSchedule { .. } => {
                handlers_mutations::execute(self, open, pending, isolated)
            }
            Action::RejectedSchedule { .. } => {
                handlers_mutations::execute(self, open, pending, isolated)
            }
            Action::Control { .. } => handlers_mutations::execute(self, open, pending, isolated),
            Action::Prepare { .. } => handlers_mutations::execute(self, open, pending, isolated),
            #[cfg(any(test, feature = "protected-test-support"))]
            Action::PrepareConsumedSend { .. } => {
                handlers_mutations::execute(self, open, pending, isolated)
            }
            Action::ClaimDispatch { .. } => {
                handlers_mutations::execute(self, open, pending, isolated)
            }
            Action::Record { .. } => handlers_mutations::execute(self, open, pending, isolated),
            Action::RecoveryState { .. } => {
                handlers_recovery::execute(self, open, pending, isolated)
            }
            Action::TakeOver { .. } => handlers_recovery::execute(self, open, pending, isolated),
            Action::ReserveCheck { .. } => {
                handlers_recovery::execute(self, open, pending, isolated)
            }
            Action::ClaimCheck { .. } => handlers_recovery::execute(self, open, pending, isolated),
            Action::CompleteCheck { .. } => {
                handlers_recovery::execute(self, open, pending, isolated)
            }
            Action::SupersedeCheck { .. } => {
                handlers_recovery::execute(self, open, pending, isolated)
            }
            Action::AcceptLatePositive { .. } => {
                handlers_recovery::execute(self, open, pending, isolated)
            }
            Action::ReserveBaseline { .. } => {
                handlers_baseline::execute(self, open, pending, isolated)
            }
            Action::ClaimBaseline { .. } => {
                handlers_baseline::execute(self, open, pending, isolated)
            }
            Action::RetainBaseline { .. } => {
                handlers_baseline::execute(self, open, pending, isolated)
            }
            Action::FinishBaseline { .. } => {
                handlers_baseline::execute(self, open, pending, isolated)
            }
            Action::SupersedeBaseline { .. } => {
                handlers_baseline::execute(self, open, pending, isolated)
            }
            Action::ReserveObservation { .. } => {
                handlers_observations::execute(self, open, pending, isolated)
            }
            #[cfg(any(test, feature = "protected-test-support"))]
            Action::FinishRetainedObservation { .. } => {
                handlers_observations::execute(self, open, pending, isolated)
            }
            Action::FinishObservation { .. } => {
                handlers_observations::execute(self, open, pending, isolated)
            }
            Action::Release { .. } => handlers_local::execute(self, open, pending, isolated),
            Action::StartLocal { .. } => handlers_local::execute(self, open, pending, isolated),
            #[cfg(any(test, feature = "protected-test-support"))]
            Action::StartRetainedLocal { .. } => {
                handlers_local::execute(self, open, pending, isolated)
            }
            Action::RenewLocal { .. } => handlers_local::execute(self, open, pending, isolated),
            Action::WriteLocal { .. } => handlers_local::execute(self, open, pending, isolated),
            Action::KeepEditing { .. } => handlers_local::execute(self, open, pending, isolated),
            #[cfg(test)]
            Action::PausePrepared { .. } => {
                handlers_support::execute(self, open, pending, isolated)
            }
            #[cfg(any(test, feature = "protected-test-support"))]
            Action::ArmFault { .. } => handlers_support::execute(self, open, pending, isolated),
            #[cfg(any(test, feature = "protected-test-support"))]
            Action::Initialize { .. } => handlers_support::execute(self, open, pending, isolated),
        }
    }
}
