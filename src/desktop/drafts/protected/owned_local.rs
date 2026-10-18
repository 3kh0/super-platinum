//! The existing LocalWriter FIFO retains its exact named Owned write.
use super::*;
pub(super) enum Reply {
    Legacy(durable::Reply<durable::LocalWriteResult>),
    Owned(
        Arc<
            super::super::owned_route::Held<
                super::super::owned_route::o::LocalWriteInput,
                durable::LocalWriteResult,
            >,
        >,
    ),
}
impl LocalWriter {
    pub(super) fn enqueue(
        &self,
        command: &mut Command,
        retry: bool,
    ) -> Result<Reply, durable::Error> {
        if let Some(route) = &self.account.owned {
            use super::super::owned_route::{Held, o, ui};
            if command.owned.is_none() {
                if retry {
                    return Err(durable::Error::Stale);
                }
                command.owned = Some(Held::new(route.operations.write_local(
                    o::LocalWriteInput {
                        lease: self.lease.clone(),
                        predecessor: command.predecessor,
                        serial: command.serial,
                        expected: command.expected.clone(),
                        local: command.local.clone(),
                    },
                )));
            }
            // Command custody precedes admission/submit. An exact refusal stays
            // attached to this command and cannot become a fresh root on retry.
            let held = command.owned.as_ref().expect("retained command");
            let job = held.job().map_err(ui)?;
            if retry && job.state() == o::JobState::Failed(durable::Error::Storage) {
                job.retry_storage().map_err(ui)?;
            }
            held.submit().map_err(ui)?;
            return Ok(Reply::Owned(held.clone()));
        }
        Ok(Reply::Legacy(self.account.client()?.write_local(
            self.lease.clone(),
            command.predecessor,
            command.serial,
            command.expected.clone(),
            command.local.clone(),
        )))
    }
    pub(super) async fn current_inventory(
        &self,
    ) -> Result<durable::LocalInventory, durable::Error> {
        if let Some(route) = &self.account.owned {
            return route.local(self.lease.scope().clone()).await;
        }
        super::super::account::wait(
            self.account
                .client()?
                .review_local(self.lease.scope().clone()),
        )
        .await
    }
}
pub(super) async fn wait(reply: Reply) -> Result<durable::LocalWriteResult, durable::Error> {
    match reply {
        Reply::Legacy(reply) => super::super::account::wait(reply).await,
        Reply::Owned(held) => {
            use super::super::owned_route::{o, ui};
            held.wait().await.map_err(ui)?;
            match held
                .job()
                .map_err(ui)?
                .local_result()
                .ok_or(durable::Error::Pending)??
            {
                o::LocalOutcome::Applied(v) => Ok(durable::LocalWriteResult::Applied(v)),
                o::LocalOutcome::Preserved(v) => Ok(durable::LocalWriteResult::Preserved(v)),
                o::LocalOutcome::Retired => Ok(durable::LocalWriteResult::Retired),
            }
        }
    }
}
