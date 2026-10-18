//! Shared retained reservation for existing scheduled-message mutations.
//! The guard outlives a dialog; dropping an unfinished dispatched write retains
//! uncertainty instead of unlocking a second mutation.
use super::{
    Context,
    attachments::{
        Lease, Operations, Owner,
        other::{Action, Outcome, Phase},
    },
};
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::{
    scheduled::{
        ScheduledMessage,
        edit::{complete_readback, confirms_submission, matches_reviewed},
    },
    slack::{Error, PreparedRequest, api},
};

pub(crate) struct Run {
    pub context: Context,
    pub lease: Lease,
    operations: Signal<Operations>,
    dispatched: bool,
    settled: bool,
}

impl Run {
    pub fn detached(state: Signal<ShellState>, id: &str, action: Action) -> Result<Self, Error> {
        let context = Context::get(&state).ok_or_else(stale)?;
        let owner = Owner::capture(&state.peek(), id).ok_or_else(stale)?;
        let mut operations = try_consume_context::<Signal<Operations>>().ok_or_else(stale)?;
        let lease = operations
            .write()
            .reserve_other(owner)
            .map_err(|_| busy())?;
        if operations.write().begin_other(&lease, action).is_err() {
            let _ = operations.write().release_other(&lease);
            return Err(busy());
        }
        let _ = operations.write().detach_other(&lease);
        Ok(Self {
            context,
            lease,
            operations,
            dispatched: false,
            settled: false,
        })
    }

    /// Called in the event handler before eval/preflight/upload awaits.
    pub fn editor(
        state: Signal<ShellState>,
        mut operations: Signal<Operations>,
        lease: Lease,
        action: Action,
    ) -> Result<Self, Error> {
        let context = Context::get(&state).ok_or_else(stale)?;
        if !lease.current(&state.peek(), &context) {
            return Err(stale());
        }
        operations
            .write()
            .begin_other(&lease, action)
            .map_err(|_| busy())?;
        Ok(Self {
            context,
            lease,
            operations,
            dispatched: false,
            settled: false,
        })
    }

    pub fn check(&self, state: Signal<ShellState>) -> Result<(), Error> {
        if !self.lease.current(&state.peek(), &self.context)
            || !self
                .operations
                .peek()
                .other_snapshot(&self.lease)
                .is_ok_and(|snapshot| snapshot.phase == Phase::Running)
        {
            return Err(stale());
        }
        Ok(())
    }

    pub fn expect(&mut self, request: api::ScheduleMessage) -> Result<(), Error> {
        self.operations
            .write()
            .expect_other(&self.lease, request)
            .map_err(|_| busy())
    }

    pub async fn preflight(
        &self,
        state: Signal<ShellState>,
        expected: &ScheduledMessage,
    ) -> Result<ScheduledMessage, Error> {
        self.check(state)?;
        crate::forward::scheduled::allow_existing(state, &expected.client_msg_id).await?;
        self.check(state)?;
        let request = api::drafts_info(&self.context.client, &self.context.session, &expected.id)?;
        let response = self.context.transport.execute(request).await?;
        self.check(state)?;
        let info = api::decode_drafts_info(&expected.id, response)?;
        self.preflight_response(state, expected, &info)
    }

    fn preflight_response(
        &self,
        mut state: Signal<ShellState>,
        expected: &ScheduledMessage,
        info: &api::DraftInfo,
    ) -> Result<ScheduledMessage, Error> {
        self.check(state)?;
        let current = complete_readback(info.raw(), &expected.id)
            .ok_or_else(|| Error::Api("draft_info_unresolved".into()))?;
        if !state.peek().core.scheduled.review_is_current(expected) {
            // Preserve the queue's newer/changed/retired evidence; this info
            // response cannot replace it merely by matching the old editor.
            return Err(Error::Api("draft_has_conflict".into()));
        }
        if !matches_reviewed(info.raw(), expected)
            || !state
                .write()
                .core
                .scheduled
                .verify_revision(expected, current.clone())
        {
            // Publish the positive, complete remote row only to this captured
            // queue. The immutable reviewed baseline still controls admission.
            state.write().core.scheduled.accept_remote(current);
            return Err(Error::Api("draft_has_conflict".into()));
        }
        Ok(current)
    }

    pub async fn dispatch(
        &mut self,
        state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<serde_json::Value, Error> {
        self.check(state)?;
        self.dispatched = true;
        let response = self.context.transport.execute_draft_mutation(request).await;
        if matches!(&response, Err(Error::Api(code)) if code == "draft_mutation_not_sent_offline") {
            // The structured transport never converts a server error member into
            // this locally generated pre-dispatch receipt.
            self.dispatched = false;
        }
        if let Ok(raw) = &response {
            if let Some(code) = super_platinum_core::scheduled::edit::definite_rejection(raw) {
                let _ = self
                    .operations
                    .write()
                    .settle_other(&self.lease, Outcome::Rejected);
                self.settled = true;
                return Err(Error::Api(code.into()));
            }
        }
        if let Err(Error::RateLimited { retry_after_secs }) = &response {
            let seconds = retry_after_secs.unwrap_or(30);
            let _ = self
                .operations
                .write()
                .other_review_cooldown(&self.lease, seconds);
        }
        response
    }

    /// Forwarding's durable journal must receive the original response envelope.
    /// Keep the same exact shared owner and single-attempt transport boundary.
    pub(crate) async fn dispatch_retained(
        &mut self,
        state: Signal<ShellState>,
        request: PreparedRequest,
    ) -> Result<serde_json::Value, Error> {
        self.check(state)?;
        self.dispatched = true;
        let response =
            crate::forward::scheduled::fixture::execute(&self.context, request, true).await;
        if matches!(&response, Err(Error::Api(code)) if code == "draft_mutation_not_sent_offline") {
            self.dispatched = false;
        }
        response
    }

    pub fn confirmed(&mut self) {
        let _ = self
            .operations
            .write()
            .settle_other(&self.lease, Outcome::Confirmed);
        self.settled = true;
    }

    /// A durable forwarding receipt settles the shared fence without claiming
    /// that a rejected or never-dispatched operation succeeded.
    pub(crate) fn settle_retained(&mut self, confirmed: bool) {
        let outcome = if confirmed {
            Outcome::Confirmed
        } else if self.dispatched {
            Outcome::Rejected
        } else {
            Outcome::NotDispatched
        };
        let _ = self.operations.write().settle_other(&self.lease, outcome);
        self.settled = true;
    }

    pub(crate) fn resolved_retained_conflict(&mut self) {
        let _ = self
            .operations
            .write()
            .settle_other(&self.lease, Outcome::ReviewedConflict);
        self.settled = true;
    }

    pub fn unknown(&self) -> bool {
        self.dispatched && !self.settled
    }

    fn clear_busy(&self, mut state: Signal<ShellState>) {
        let mut shell = state.write();
        if self.lease.matches_shell(&shell)
            && shell
                .core
                .scheduled
                .matches_scope(self.context.epoch, &self.context.session.team_id)
        {
            shell.core.scheduled.busy.remove(self.lease.draft_id());
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if !self.settled {
            super::attachments::cleanup::run(
                self.operations,
                self.lease.clone(),
                super::attachments::cleanup::Action::Settle(if self.dispatched {
                    Outcome::Unknown
                } else {
                    Outcome::NotDispatched
                }),
            );
        }
    }
}

fn ensure_send_identity(draft: &mut ScheduledMessage) {
    if draft.client_msg_id.is_empty() {
        draft.client_msg_id =
            api::ScheduleMessage::new(String::new(), None, vec![], 0).client_msg_id;
    }
}

pub(crate) fn positive(response: &serde_json::Value) -> bool {
    response.get("ok") == Some(&serde_json::Value::Bool(true))
        && response.get("error").is_none_or(serde_json::Value::is_null)
}
pub(crate) fn unconfirmed() -> Error {
    Error::Api("draft_result_unconfirmed".into())
}
fn stale() -> Error {
    Error::Api("draft_account_changed".into())
}
fn busy() -> Error {
    Error::Api("draft_mutation_reserved".into())
}

#[cfg(test)]
mod tests;

pub(super) async fn remove(mut state: Signal<ShellState>, expected: ScheduledMessage) {
    let id = expected.id.clone();
    if !super::visible(&state.peek()).is_some_and(|queue| queue.items.contains_key(&id)) {
        return;
    }
    let mut run = match Run::detached(state, &id, Action::Delete) {
        Ok(run) => run,
        Err(error) => {
            state.write().core.scheduled.error = Some(super::failure(&error));
            return;
        }
    };
    if !state.write().core.scheduled.begin_mutation(&id) {
        return;
    }
    let result = async {
        run.preflight(state, &expected).await?;
        let request = api::drafts_delete(&run.context.client, &run.context.session, &id);
        let response = run.dispatch(state, request).await?;
        if !positive(&response) {
            return Err(unconfirmed());
        }
        run.confirmed();
        Ok(())
    }
    .await;
    let current = run.lease.current(&state.peek(), &run.context);
    let unknown = run.unknown();
    run.clear_busy(state);
    drop(run); // settle the captured reservation even if its account is no longer visible
    if !current {
        return;
    }
    let mut shell = state.write();
    shell.core.scheduled.busy.remove(&id);
    match result {
        Ok(()) => {
            shell.core.scheduled.items.remove(&id);
            shell.show_toast("Draft deleted.");
        }
        Err(error) => {
            let error = if unknown { unconfirmed() } else { error };
            shell.core.scheduled.error = Some(super::failure(&error));
        }
    }
}

pub(super) async fn act(
    mut state: Signal<ShellState>,
    id: String,
    action: super::DraftAction,
    reviewed: Option<ScheduledMessage>,
) {
    let Some(expected) = super::visible(&state.peek())
        .and_then(|queue| queue.items.get(&id))
        .cloned()
    else {
        return;
    };
    let expected = reviewed.unwrap_or(expected);
    let mut run = match Run::detached(
        state,
        &id,
        match action {
            super::DraftAction::Send => Action::Send,
            super::DraftAction::Unschedule => Action::Unschedule,
        },
    ) {
        Ok(run) => run,
        Err(error) => {
            state.write().core.scheduled.error = Some(super::failure(&error));
            return;
        }
    };
    if !state.write().core.scheduled.begin_mutation(&id) {
        return;
    }
    let result = async {
        let mut current = run.preflight(state, &expected).await?;
        match action {
            super::DraftAction::Send => {
                ensure_send_identity(&mut current);
                let request =
                    api::drafts_send(&run.context.client, &run.context.session, &current)?;
                let response = run.dispatch(state, request).await?;
                if !positive(&response)
                    || !response
                        .get(if current.file_ids.is_empty() {
                            "ts"
                        } else {
                            "file_msg_ts"
                        })
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|ts| !ts.is_empty())
                {
                    return Err(unconfirmed());
                }
                run.confirmed();
                Ok(None)
            }
            super::DraftAction::Unschedule => {
                let mut payload = api::ScheduleMessage::from_draft(&current);
                payload.post_at = 0;
                run.expect(payload.clone())?;
                let request = api::drafts_schedule(
                    &run.context.client,
                    &run.context.session,
                    &payload,
                    Some(&id),
                );
                let response = run.dispatch(state, request).await?;
                if !confirms_submission(&response, &id, &payload) {
                    return Err(unconfirmed());
                }
                let updated: ScheduledMessage =
                    serde_json::from_value(response["draft"].clone()).map_err(|_| unconfirmed())?;
                run.confirmed();
                Ok(Some(updated))
            }
        }
    }
    .await;
    let current = run.lease.current(&state.peek(), &run.context);
    let unknown = run.unknown();
    run.clear_busy(state);
    drop(run);
    if !current {
        return;
    }
    let mut shell = state.write();
    shell.core.scheduled.busy.remove(&id);
    match result {
        Ok(Some(draft)) => {
            shell.core.scheduled.accept_remote(draft);
            shell.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Drafts;
            shell.show_toast("Schedule canceled. Message saved to drafts.");
        }
        Ok(None) => {
            shell.core.scheduled.items.remove(&id);
            shell.show_toast("Message sent.");
        }
        Err(error) => {
            let error = if unknown { unconfirmed() } else { error };
            shell.core.scheduled.error = Some(super::failure(&error));
        }
    }
}
