//! Adapter at the existing Editor actor's command boundary. No second actor.
use super::owned_route::{Held, Route, o};
use super::ownership::Tracked as Mutex;
use super::*;

pub(super) struct Write {
    pub original: editor::Command,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    read: Option<Arc<Held<Scope, durable::Review>>>,
    checkpoint: Option<Arc<Held<o::CheckpointInput, durable::WriteResult>>>,
    preserve: Option<Arc<Held<o::PreserveInput, durable::WriteResult>>>,
    refusal: Option<o::AdmissionError>,
    read_acknowledged: bool,
    checkpoint_acknowledged: Option<durable::Review>,
}
enum CheckpointAcknowledgment {
    Clean(o::SuccessAcknowledgment<o::CheckpointInput, durable::WriteResult>),
    Settled(o::WriterSettlementAcknowledgment<o::CheckpointInput, durable::WriteResult>),
}
impl CheckpointAcknowledgment {
    fn acknowledge(self) -> Result<(), durable::Error> {
        match self {
            Self::Clean(receipt) => receipt
                .acknowledge()
                .map_err(|(e, _)| super::owned_route::ui(e)),
            Self::Settled(receipt) => receipt
                .acknowledge()
                .map_err(|(e, _)| super::owned_route::ui(e)),
        }
    }
}
impl Write {
    /// Called only after the actor handled the exact applied/current receipt.
    pub(super) fn acknowledge_applied(
        &self,
        expected: &durable::Review,
    ) -> Result<(), durable::Error> {
        let mut state = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if let Some(prior) = &state.checkpoint_acknowledged {
            return if prior.same(expected) {
                Ok(())
            } else {
                Err(durable::Error::Stale)
            };
        }
        let checkpoint = state.checkpoint.clone().ok_or(durable::Error::Pending)?;
        let job = checkpoint.job().map_err(super::owned_route::ui)?;
        let (actual, receipt) = match job.observe_checkpoint() {
            Ok((actual, receipt)) => (actual, CheckpointAcknowledgment::Clean(receipt)),
            Err(_) => {
                // A repaired write or a checkpoint during pending Send is not
                // clean success. Observe its exact history through the closed
                // writer receipt; every other refusal remains an error.
                let (actual, _history, receipt) = job
                    .observe_checkpoint_settlement()
                    .map_err(super::owned_route::ui)?;
                (actual, CheckpointAcknowledgment::Settled(receipt))
            }
        };
        if !actual.same(expected) {
            return Err(durable::Error::Stale);
        }
        if !state.read_acknowledged {
            if let Some(read) = &state.read {
                let (_, acknowledgment) = read
                    .job()
                    .map_err(super::owned_route::ui)?
                    .observe_success()
                    .map_err(super::owned_route::ui)?;
                acknowledgment
                    .acknowledge()
                    .map_err(|(e, _)| super::owned_route::ui(e))?;
            }
            state.read_acknowledged = true;
        }
        receipt.acknowledge()?;
        state.checkpoint_acknowledged = Some(actual);
        Ok(())
    }

    /// Desktop holder membership only. Core job progression is a separate edge.
    pub(super) fn ownership_stamp(
        &self,
    ) -> Result<super::ownership::Stamp, super::ownership::Error> {
        self.state.observe(|_| ()).map(|(stamp, ())| stamp)
    }
    pub(super) fn validate_ownership(
        &self,
        stamp: &super::ownership::Stamp,
    ) -> Result<(), super::ownership::Error> {
        self.state.validate(stamp)
    }

    pub fn new(command: editor::Command) -> Arc<Self> {
        Arc::new(Self {
            original: command,
            state: Mutex::default(),
        })
    }
    pub fn refusal(&self) -> Option<o::AdmissionError> {
        self.state.lock().ok()?.refusal
    }
    #[cfg(test)]
    pub fn checkpoint_state(&self) -> Option<o::JobState> {
        self.state
            .lock()
            .ok()?
            .checkpoint
            .as_ref()?
            .job()
            .ok()
            .map(|j| j.state())
    }
    pub async fn run(
        &self,
        route: &Route,
        owner: &Owner,
        command: &mut editor::Command,
        preserve: &mut bool,
        retry: bool,
    ) -> Result<Result<durable::WriteResult, durable::Error>, o::AdmissionError> {
        let result = self.run_inner(route, owner, command, preserve, retry).await;
        if let Err(reason) = result {
            self.state
                .lock()
                .map_err(|_| o::AdmissionError::Incomplete)?
                .refusal = Some(reason);
        }
        result
    }
    async fn run_inner(
        &self,
        route: &Route,
        owner: &Owner,
        command: &mut editor::Command,
        preserve: &mut bool,
        retry: bool,
    ) -> Result<Result<durable::WriteResult, durable::Error>, o::AdmissionError> {
        if self.original.serial != command.serial
            || self.original.writer.id() != command.writer.id()
            || self.original.writer.enrollment() != command.writer.enrollment()
            || !same_local(&self.original.local, &command.local)
        {
            return Err(o::AdmissionError::Stale);
        }
        if retry {
            let state = self
                .state
                .lock()
                .map_err(|_| o::AdmissionError::Incomplete)?;
            if state.checkpoint.is_none() && state.preserve.is_none() {
                // A read failure did not admit a writer command. This slice has
                // no read retry family; do not silently invent a checkpoint.
                return match &state.read {
                    Some(read) => match read.job()?.result() {
                        Some(Err(error)) => Ok(Err(error)),
                        _ => Err(state.refusal.unwrap_or(o::AdmissionError::Incomplete)),
                    },
                    None => Err(state.refusal.unwrap_or(o::AdmissionError::Incomplete)),
                };
            }
        }
        if !*preserve && !retry {
            let read = {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| o::AdmissionError::Incomplete)?;
                state
                    .read
                    .get_or_insert_with(|| {
                        Held::new(route.operations.review(owner.scope().clone()))
                    })
                    .clone()
            };
            // Completed domain errors may select Preserve. Queue refusal never
            // does: the outer ? retains this exact read/job and caller input.
            match read.value().await? {
                Ok(fresh)
                    if editor_proof::can_rebase(&command.expected, &fresh, &command.writer) =>
                {
                    command.expected = fresh;
                }
                Ok(_) => *preserve = true,
                Err(durable::Error::Storage) => return Ok(Err(durable::Error::Storage)),
                Err(_) => *preserve = true,
            }
        }
        let mut result = if *preserve {
            self.preserve(route, command, retry).await?
        } else {
            self.checkpoint(route, command, retry).await?
        };
        if !*preserve
            && result
                .as_ref()
                .is_err_and(|e| *e != durable::Error::Storage)
        {
            *preserve = true;
            result = self.preserve(route, command, false).await?;
        }
        Ok(result)
    }
    async fn checkpoint(
        &self,
        route: &Route,
        command: &editor::Command,
        retry: bool,
    ) -> Result<Result<durable::WriteResult, durable::Error>, o::AdmissionError> {
        let held = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| o::AdmissionError::Incomplete)?;
            state
                .checkpoint
                .get_or_insert_with(|| {
                    Held::new(route.operations.checkpoint(o::CheckpointInput {
                        writer: command.writer.clone(),
                        serial: command.serial,
                        expected: command.expected.clone(),
                        local: command.local.clone(),
                        direct: command.direct.clone(),
                    }))
                })
                .clone()
        };
        let job = held.job()?;
        if retry && job.state() == o::JobState::Failed(durable::Error::Storage) {
            job.retry_storage()?;
        }
        held.submit()?;
        held.wait().await?;
        job.write_result()
            .ok_or(o::AdmissionError::Incomplete)
            .map(|v| v.map(write_result))
    }
    async fn preserve(
        &self,
        route: &Route,
        command: &editor::Command,
        retry: bool,
    ) -> Result<Result<durable::WriteResult, durable::Error>, o::AdmissionError> {
        let held = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| o::AdmissionError::Incomplete)?;
            state
                .preserve
                .get_or_insert_with(|| {
                    Held::new(route.operations.preserve(o::PreserveInput {
                        writer: command.writer.clone(),
                        serial: command.serial,
                        local: command.local.clone(),
                    }))
                })
                .clone()
        };
        let job = held.job()?;
        if retry && job.state() == o::JobState::Failed(durable::Error::Storage) {
            job.retry_storage()?;
        }
        held.submit()?;
        held.wait().await?;
        job.write_result()
            .ok_or(o::AdmissionError::Incomplete)
            .map(|v| v.map(write_result))
    }
}
fn write_result(value: o::WriteOutcome) -> durable::WriteResult {
    match value {
        o::WriteOutcome::Applied(v) => durable::WriteResult::Applied(v),
        o::WriteOutcome::Preserved(v) => durable::WriteResult::Preserved(v),
        o::WriteOutcome::Retired => durable::WriteResult::Retired,
    }
}
