//! Closed lease-bound local FIFO writes, never a Release/renewal constructor.
use super::*;
#[derive(Clone)]
pub struct LocalWriteInput {
    pub lease: p::LocalWriterLease,
    pub predecessor: u64,
    pub serial: u64,
    pub expected: LocalDraft,
    pub local: LocalDraft,
}
#[derive(Clone)]
pub enum LocalOutcome {
    Applied(p::LocalInventory),
    Preserved(p::LocalInventory),
    Retired,
}
fn request(
    i: &LocalWriteInput,
    reply: mpsc::Sender<Result<p::LocalWriteResult, p::Error>>,
) -> Action {
    Action::WriteLocal {
        lease: i.lease.clone(),
        predecessor_serial: i.predecessor,
        serial: i.serial,
        expected_local: i.expected.clone(),
        local: i.local.clone(),
        reply,
    }
}
impl Step<LocalWriteInput, p::LocalWriteResult> {
    pub(in crate::drafts::store::worker) fn local_result(
        &self,
    ) -> Option<Result<LocalOutcome, p::Error>> {
        let mut state = self.held.status.lock().unwrap();
        state.poll();
        state.result.as_ref().map(|v| match v {
            Ok(p::LocalWriteResult::Applied(i)) => Ok(LocalOutcome::Applied(i.clone())),
            Ok(p::LocalWriteResult::Preserved(i)) => Ok(LocalOutcome::Preserved(i.clone())),
            Ok(p::LocalWriteResult::Retired) => Ok(LocalOutcome::Retired),
            Err(e) => Err(*e),
        })
    }
}
impl Worker {
    pub(in crate::drafts::store::worker) fn hold_local_write(
        &self,
        input: LocalWriteInput,
    ) -> Result<Step<LocalWriteInput, p::LocalWriteResult>, Refused<LocalWriteInput>> {
        if !bounded(
            &input.lease.scope().account_id,
            Some(&input.lease.scope().key),
        ) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.lease.scope().clone();
        self.step(&scope, input, "protected-write-local", request)
    }
    pub(in crate::drafts::store::worker) fn retry_issued_local_write(
        &self,
        step: &Step<LocalWriteInput, p::LocalWriteResult>,
    ) -> Result<(), Reason> {
        self.retry_local_storage(step, request)
    }
}
