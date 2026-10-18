//! Human user links own one DM + profile destination, never an app/bot route.
//!
//! Retained client: B C/0c 638648 sends known humans to jhdg.AS;
//! S toUserDMAndProfile 1534985 sets {memberId, showDM:true} (export 1552561).
//! Offsets are Unicode characters in the hash-verified tmp/sb/js assets.
use super::{Visit, WorkspaceRef, resolve_team, select_target};
use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::slack::{
    Error, api,
    models::{Channel, OpenedConversation},
};

const UNAVAILABLE: &str = "This person is not available for a direct-message link.";
const WRONG_DM: &str = "Slack returned a different direct conversation. This link was not opened.";

fn human(shell: &ShellState, team: &str, user: &str) -> bool {
    shell
        .core
        .workspaces
        .get(team)
        .and_then(|workspace| workspace.users.get(user))
        .is_some_and(|member| {
            member.id == user
                && user != "USLACKBOT"
                && !member.deleted
                && !member.is_bot
                && member
                    .extra
                    .get("is_app_user")
                    .is_none_or(|value| value.as_bool() == Some(false))
        })
}

fn dm_matches(channel: &Channel, user: &str) -> bool {
    channel.id.starts_with('D')
        && channel.id.len() > 1
        && channel.id.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && channel.is_im
        && !channel.is_mpim
        && !channel.is_archived
        && channel.user.as_deref() == Some(user)
}

pub(super) struct Intent {
    visit: Visit,
    account: Option<String>,
    user: String,
    request: u64,
}

impl Intent {
    /// Validate the target before any workspace/navigation change. None is a
    /// duplicate pending open; it must neither supersede nor resend that request.
    pub(super) fn begin(
        shell: &mut ShellState,
        team: String,
        user: String,
    ) -> Result<Option<Self>, &'static str> {
        let target = WorkspaceRef::TeamId(team);
        let team = resolve_team(shell, &target)?;
        if !human(shell, &team, &user) {
            return Err(UNAVAILABLE);
        }
        if shell.core.active_team.as_ref() == Some(&team)
            && shell.profile_dm_opening.as_ref() == Some(&user)
            && shell.profile_dm_link_generation == Some(shell.slack_link_generation)
        {
            return Ok(None);
        }
        let visit = Visit::begin(shell, &target, false)?;
        shell.cancel_profile_dm();
        shell.profile_dm_opening = Some(user.clone());
        shell.profile_dm_link_generation = Some(shell.slack_link_generation);
        Ok(Some(Self {
            account: shell.core.active_account.clone(),
            visit,
            user,
            request: shell.profile_dm_generation,
        }))
    }

    fn owner_current(&self, shell: &ShellState) -> bool {
        shell.core.active_account == self.account
            && shell.core.account_epoch == self.visit.epoch
            && shell.core.active_team.as_ref() == Some(&self.visit.session.team_id)
            && shell
                .core
                .workspaces
                .get(&self.visit.session.team_id)
                .is_some_and(|workspace| workspace.self_user_id == self.visit.session.user_id)
            && shell
                .core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&self.visit.session.team_id))
                .is_some_and(|session| {
                    session.team_id == self.visit.session.team_id
                        && session.user_id == self.visit.session.user_id
                })
    }

    fn account_current(&self, shell: &ShellState) -> bool {
        let expected = &self.visit.session;
        self.owner_current(shell)
            && shell
                .core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&expected.team_id))
                .is_some_and(|session| {
                    session.team_id == expected.team_id
                        && session.user_id == expected.user_id
                        && session.token == expected.token
                        && session.url == expected.url
                        && session.enterprise_id == expected.enterprise_id
                })
            && match (&self.visit.transport, &shell.core.transport) {
                (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }

    fn current(&self, shell: &ShellState) -> bool {
        self.visit.current(shell)
            && self.account_current(shell)
            && shell.profile_dm_generation == self.request
            && shell.profile_dm_opening.as_ref() == Some(&self.user)
            && shell.profile_dm_link_generation == Some(self.visit.link)
            && human(shell, &self.visit.session.team_id, &self.user)
    }

    fn release(&self, shell: &mut ShellState) {
        // Credential rotation invalidates the response, but does not transfer
        // this reservation to another account. Clear only this exact owner's
        // pending request so a deliberately retried link can be admitted.
        if self.owner_current(shell)
            && shell.profile_dm_generation == self.request
            && shell.profile_dm_opening.as_ref() == Some(&self.user)
        {
            shell.profile_dm_opening = None;
            shell.profile_dm_link_generation = None;
        }
    }

    fn cached(&self, shell: &ShellState) -> Result<Option<Channel>, &'static str> {
        let mut candidates = shell.core.workspaces[&self.visit.session.team_id]
            .channels
            .values()
            .filter(|channel| dm_matches(channel, &self.user));
        let first = candidates.next().cloned();
        if candidates.next().is_some() {
            return Err("More than one direct conversation matches this person.");
        }
        Ok(first)
    }

    /// One serial commit preserves the outgoing draft and displays the profile
    /// beside the exact DM. A mismatched response never inserts guessed metadata.
    fn commit(&self, shell: &mut ShellState, channel: Channel) -> Result<bool, &'static str> {
        if !self.current(shell) {
            return Ok(false);
        }
        if !dm_matches(&channel, &self.user) {
            return Err(WRONG_DM);
        }
        let workspace = shell
            .core
            .workspaces
            .get_mut(&self.visit.session.team_id)
            .ok_or(UNAVAILABLE)?;
        if workspace
            .channels
            .get(&channel.id)
            .is_some_and(|existing| !dm_matches(existing, &self.user))
        {
            return Err(WRONG_DM);
        }
        let id = channel.id.clone();
        workspace.channels.entry(id.clone()).or_insert(channel);
        if !select_target(shell, &id, None) {
            return Err(UNAVAILABLE);
        }
        crate::bootstrap::show_profile(shell, self.user.clone());
        Ok(true)
    }
}

pub(super) async fn navigate(mut state: Signal<ShellState>, intent: Intent) {
    let cached = {
        let shell = state.peek();
        if !intent.current(&shell) {
            drop(shell);
            intent.release(&mut state.write());
            return;
        }
        intent.cached(&shell)
    };
    let mut posting = None;
    let mut observed = None;
    let channel = match cached {
        Ok(Some(channel)) => Ok(channel),
        Err(message) => Err(Error::Api(message.into())),
        Ok(None) => {
            if let Some(transport) = intent.visit.transport.as_ref() {
                posting = Some(crate::bootstrap::posting_ingress::capture(
                    &state,
                    transport,
                    &intent.visit.session,
                ));
            }
            let client = state.peek().core.client.clone();
            let request =
                api::conversations_open(&client, &intent.visit.session, intent.user.clone());
            // Ordinary fixture boot must never fall through to authenticated I/O.
            let response = if super::user_fixture::enabled() {
                super::user_fixture::execute(state, request, &intent.user).await
            } else if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
                intent.release(&mut state.write());
                state
                    .write()
                    .show_toast("No direct message channel is loaded yet.");
                return;
            } else if let Some(transport) = intent.visit.transport.as_ref() {
                transport.execute(request).await
            } else {
                intent.release(&mut state.write());
                state
                    .write()
                    .show_toast("Sign in to open this direct message.");
                return;
            };
            // Preserve metadata from the existing request instead of discarding
            // the recipient identity through the id-only open_dm adapter.
            match response {
                Ok(value) => serde_json::from_value::<OpenedConversation>(value)
                    .map(|response| {
                        observed = Some(response.channel.clone());
                        response.channel
                    })
                    .map_err(|_| Error::Api("invalid_direct_conversation_response".into())),
                Err(error) => Err(error),
            }
        }
    };
    let committed = {
        let mut shell = state.write();
        if !intent.current(&shell) {
            intent.release(&mut shell);
            return;
        }
        match channel {
            Ok(channel) => match intent.commit(&mut shell, channel) {
                Ok(committed) => committed,
                Err(message) => {
                    intent.release(&mut shell);
                    shell.show_toast(message);
                    false
                }
            },
            Err(error) => {
                intent.release(&mut shell);
                shell.report_failure(
                    &error,
                    "Connect to Slack to open this direct message.",
                    || "Could not open this direct message. Try again.".into(),
                );
                false
            }
        }
    };
    if committed {
        if let (Some(posting), Some(channel)) = (posting, observed) {
            posting.channels(&state, std::slice::from_ref(&channel));
        }
        // Both existing hydrators capture their own destination before yielding;
        // neither is sequenced behind the other's potentially slow network work.
        tokio::join!(
            crate::bootstrap::refresh_open_profile(state, intent.user),
            crate::bootstrap::refresh_selected_channel(state),
        );
    }
}

#[cfg(test)]
mod tests;
