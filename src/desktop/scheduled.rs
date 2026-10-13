//! Account guarded scheduled-message orchestration.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    scheduled::{ScheduledMessage, ScheduledState},
    slack::{SlackClient, Transport, api},
};

pub struct Context {
    pub transport: Arc<Transport>,
    pub client: SlackClient,
    pub session: WorkspaceSession,
    pub epoch: u64,
}
impl Context {
    pub fn get(state: &Signal<ShellState>) -> Option<Self> {
        Self::capture(&state.read())
    }
    /// Capture credentials from an existing snapshot without subscribing a
    /// caller's effect to unrelated shell updates.
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.as_ref()?;
        Some(Self {
            transport: shell.core.transport.clone()?,
            client: shell.core.client.clone(),
            session: shell.core.session.as_ref()?.workspaces.get(team)?.clone(),
            epoch: shell.core.account_epoch,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.session.team_id)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
    }
}
pub fn visible(shell: &ShellState) -> Option<&ScheduledState> {
    shell
        .core
        .scheduled
        .matches_scope(shell.core.account_epoch, shell.core.active_team.as_deref()?)
        .then_some(&shell.core.scheduled)
}
pub async fn load(mut state: Signal<ShellState>, more: bool) {
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    let Some((generation, limit)) = ({
        let mut shell = state.write();
        shell
            .core
            .scheduled
            .ensure_scope(ctx.epoch, &ctx.session.team_id);
        shell.core.scheduled.begin(more)
    }) else {
        return;
    };
    let result =
        api::fetch_scheduled_messages(&ctx.transport, &ctx.client, &ctx.session, limit).await;
    if !ctx.current(&state.peek()) {
        return;
    }
    let mut shell = state.write();
    if shell.core.scheduled.generation != generation {
        return;
    }
    match result {
        Ok(page) => {
            shell.core.scheduled.apply(generation, page, limit);
        }
        Err(error) => {
            shell.core.scheduled.loading = false;
            shell.core.scheduled.error = Some(failure(&error));
        }
    }
}
pub async fn remove(mut state: Signal<ShellState>, expected: ScheduledMessage) {
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    let id = expected.id.clone();
    let limit = {
        let mut shell = state.write();
        let Some(queue) = visible(&shell) else {
            return;
        };
        if !queue.items.contains_key(&id) {
            return;
        }
        if !shell.core.scheduled.begin_mutation(&id) {
            return;
        }
        shell.core.scheduled.error = None;
        shell.core.scheduled.limit
    };
    let result = async {
        let current =
            api::fetch_active_draft(&ctx.transport, &ctx.client, &ctx.session, &id, limit).await?;
        if !ctx.current(&state.peek()) {
            return Err(super_platinum_core::slack::Error::Api(
                "draft_account_changed".into(),
            ));
        }
        if !state
            .write()
            .core
            .scheduled
            .verify_revision(&expected, current)
        {
            return Err(super_platinum_core::slack::Error::Api(
                "draft_has_conflict".into(),
            ));
        }
        api::delete_scheduled_message(&ctx.transport, &ctx.client, &ctx.session, &id).await
    }
    .await;
    if !ctx.current(&state.peek()) {
        return;
    }
    let mut shell = state.write();
    shell.core.scheduled.busy.remove(&id);
    match result {
        Ok(()) => {
            shell.core.scheduled.items.remove(&id);
            shell.show_toast("Draft deleted.");
        }
        Err(error) => shell.core.scheduled.error = Some(failure(&error)),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum DraftAction {
    Send,
    Unschedule,
}
pub async fn act(state: Signal<ShellState>, id: String, action: DraftAction) {
    act_revision(state, id, action, None).await;
}
pub async fn send_reviewed(state: Signal<ShellState>, reviewed: ScheduledMessage) {
    act_revision(
        state,
        reviewed.id.clone(),
        DraftAction::Send,
        Some(reviewed),
    )
    .await;
}
async fn act_revision(
    mut state: Signal<ShellState>,
    id: String,
    action: DraftAction,
    reviewed: Option<ScheduledMessage>,
) {
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    let draft = {
        let mut shell = state.write();
        if !shell
            .core
            .scheduled
            .matches_scope(ctx.epoch, &ctx.session.team_id)
        {
            return;
        }
        let Some(draft) = shell.core.scheduled.items.get_mut(&id) else {
            return;
        };
        if draft.client_msg_id.is_empty() {
            draft.client_msg_id =
                api::ScheduleMessage::new(String::new(), None, vec![], 0).client_msg_id;
        }
        let mut expected = reviewed.unwrap_or_else(|| draft.clone());
        expected.client_msg_id = draft.client_msg_id.clone();
        if !shell.core.scheduled.begin_mutation(&id) {
            return;
        }
        shell.core.scheduled.error = None;
        expected
    };
    let limit = state.peek().core.scheduled.limit;
    let result = async {
        let mut current =
            api::fetch_active_draft(&ctx.transport, &ctx.client, &ctx.session, &id, limit).await?;
        if !ctx.current(&state.peek()) {
            return Err(super_platinum_core::slack::Error::Api(
                "draft_account_changed".into(),
            ));
        }
        match action {
            DraftAction::Send => {
                if !state
                    .write()
                    .core
                    .scheduled
                    .verify_revision(&draft, current.clone())
                {
                    return Err(super_platinum_core::slack::Error::Api(
                        "draft_has_conflict".into(),
                    ));
                }
                current.client_msg_id = draft.client_msg_id.clone();
                api::send_draft(&ctx.transport, &ctx.client, &ctx.session, &current)
                    .await
                    .map(|_| None)
            }
            DraftAction::Unschedule => {
                api::unschedule_message(&ctx.transport, &ctx.client, &ctx.session, &current)
                    .await
                    .map(Some)
            }
        }
    }
    .await;
    if !ctx.current(&state.peek()) {
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
        Err(error) => shell.core.scheduled.error = Some(failure(&error)),
    }
}
pub fn saved(mut state: Signal<ShellState>, ctx: &Context, draft: ScheduledMessage) {
    if !ctx.current(&state.peek()) {
        return;
    }
    let mut shell = state.write();
    shell
        .core
        .scheduled
        .ensure_scope(ctx.epoch, &ctx.session.team_id);
    let scheduled = draft.scheduled();
    shell.core.scheduled.filter = if scheduled {
        super_platinum_core::scheduled::DraftFilter::Scheduled
    } else {
        super_platinum_core::scheduled::DraftFilter::Drafts
    };
    shell.core.scheduled.accept_remote(draft);
    shell.show_toast(if scheduled {
        "Message scheduled."
    } else {
        "Draft saved."
    });
}
pub fn failure(error: &super_platinum_core::slack::Error) -> String {
    use super_platinum_core::slack::Error;
    match error {
        Error::Api(code) if code=="draft_not_active"=>"This draft was already sent or deleted. Refresh the list to see its current state.".into(),
        Error::Api(code) if code=="draft_not_loaded"=>"Refresh and load more drafts before changing this message.".into(),
        Error::Api(code) if code=="draft_multiple_destinations"=>"Sending drafts to multiple destinations is not supported yet. Open this draft in Slack to send it.".into(),
        Error::Api(code) if code=="draft_missing_destination"=>"This draft needs a destination before it can be sent.".into(),
        Error::Api(code) if code == "draft_has_conflict" => {
            "This message changed in Slack. Review the updated draft before trying again.".into()
        }
        Error::Api(code) if code == "time_too_far" => {
            "Choose a time within the next 120 days.".into()
        }
        Error::Api(code) if code == "time_in_past" => "Choose a time in the future.".into(),
        Error::RateLimited { .. } => {
            "Slack is limiting requests. Wait a moment, then retry.".into()
        }
        Error::Offline(_) | Error::TransportNotConfigured => {
            "Reconnect to Slack to manage scheduled messages.".into()
        }
        Error::Api(_) => {
            "Slack could not update scheduled messages. Your message is still here.".into()
        }
        _ => {
            "The result could not be confirmed. Refresh scheduled messages before retrying.".into()
        }
    }
}
