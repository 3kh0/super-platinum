use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api;

use super::session::remove_account;
use crate::state::{PresenceVm, ShellState};

pub(crate) fn minutes_until_local_hour(now_unix: i64, tz_offset: i32, hour: i32) -> u32 {
    let local = now_unix + i64::from(tz_offset);
    let secs_in_day = local.rem_euclid(86_400);
    let target = i64::from(hour) * 3_600;
    let remaining = if secs_in_day < target {
        target - secs_in_day
    } else {
        86_400 - secs_in_day + target
    };
    (remaining / 60).max(1) as u32
}

/// Self-menu completions belong to the exact account, session and viewer that
/// supplied the request. The active menu may belong to someone else after await.
#[derive(Clone, Default)]
pub(crate) struct SelfMenuMutations {
    presence: u64,
    dnd: u64,
    status: u64,
}
#[derive(Clone, Copy)]
enum MutationKind {
    Presence,
    Dnd,
    Status,
}
impl SelfMenuMutations {
    fn counter(&mut self, kind: MutationKind) -> &mut u64 {
        match kind {
            MutationKind::Presence => &mut self.presence,
            MutationKind::Dnd => &mut self.dnd,
            MutationKind::Status => &mut self.status,
        }
    }
    fn generation(&self, kind: MutationKind) -> u64 {
        match kind {
            MutationKind::Presence => self.presence,
            MutationKind::Dnd => self.dnd,
            MutationKind::Status => self.status,
        }
    }
}
pub(crate) struct SelfMenuContext(crate::scheduled::Context, MutationKind, u64);
impl SelfMenuContext {
    pub(crate) fn begin_status(shell: &mut ShellState) -> Option<Self> {
        Self::begin(shell, MutationKind::Status)
    }
    pub(crate) fn request(&self) -> &crate::scheduled::Context {
        &self.0
    }

    fn begin(shell: &mut ShellState, kind: MutationKind) -> Option<Self> {
        let generation = shell.self_menu_mutations.counter(kind);
        *generation = generation.wrapping_add(1);
        let generation = *generation;
        let context = Self(crate::scheduled::Context::capture(shell)?, kind, generation);
        context.current(shell).then_some(context)
    }

    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        let request = &self.0;
        request.current(shell)
            && shell.self_menu_mutations.generation(self.1) == self.2
            && shell.self_account.user_id == request.session.user_id
            && shell
                .core
                .workspaces
                .get(&request.session.team_id)
                .is_some_and(|workspace| {
                    workspace.team_id == request.session.team_id
                        && workspace.self_user_id == request.session.user_id
                })
            && shell
                .core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&request.session.team_id))
                .is_some_and(|session| {
                    session.team_id == request.session.team_id
                        && session.user_id == request.session.user_id
                        && session.token == request.session.token
                        && session.url == request.session.url
                })
    }

    fn apply_profile(
        &self,
        shell: &mut ShellState,
        profile: super_platinum_core::slack::models::UserProfile,
    ) {
        if !self.current(shell) {
            return;
        }
        if let Some(user) = shell
            .core
            .workspaces
            .get_mut(&self.0.session.team_id)
            .and_then(|workspace| workspace.users.get_mut(&self.0.session.user_id))
        {
            let current = user.profile.get_or_insert_with(Default::default);
            // A realtime status set after our optimistic clear wins over an
            // older HTTP acknowledgement. Unrelated profile fields always stay.
            if !current
                .status_text
                .as_deref()
                .unwrap_or_default()
                .is_empty()
                || !current
                    .status_emoji
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty()
                || current.status_expiration.unwrap_or_default() != 0
            {
                return;
            }
            current.status_text = profile.status_text;
            current.status_emoji = profile.status_emoji;
            current.status_expiration = profile.status_expiration;
        }
    }
}

pub async fn set_self_presence(mut state: Signal<ShellState>, away: bool) {
    let context = SelfMenuContext::begin(&mut state.write(), MutationKind::Presence);
    let presence = if away {
        PresenceVm::Away
    } else {
        PresenceVm::Active
    };
    state.write().apply_self_presence(presence);
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some(context) = context else {
        return;
    };
    let value = if away { "away" } else { "auto" };
    let request = &context.0;
    let result = api::set_presence(
        &request.transport,
        &request.client,
        &request.session,
        value.into(),
    )
    .await;
    if !context.current(&state.read()) {
        return;
    }
    if let Err(error) = result {
        state
            .write()
            .report_failure(&error, "Presence needs a connection", || {
                format!("Could not update presence: {error}")
            });
    }
}

pub async fn pause_notifications(mut state: Signal<ShellState>, minutes: u32) {
    let context = SelfMenuContext::begin(&mut state.write(), MutationKind::Dnd);
    {
        let mut shell = state.write();
        shell.self_menu_notifications_open = false;
        shell.apply_self_snooze_minutes(Some(minutes));
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some(context) = context else {
        return;
    };
    let request = &context.0;
    let result = api::set_snooze(
        &request.transport,
        &request.client,
        &request.session,
        minutes,
    )
    .await;
    if !context.current(&state.read()) {
        return;
    }
    match result {
        Ok(dnd) => state.write().merge_self_snooze(dnd),
        Err(error) => state
            .write()
            .report_failure(&error, "Snooze needs a connection", || {
                format!("Could not pause notifications: {error}")
            }),
    }
}

pub async fn resume_notifications(mut state: Signal<ShellState>) {
    let context = SelfMenuContext::begin(&mut state.write(), MutationKind::Dnd);
    {
        let mut shell = state.write();
        shell.self_menu_notifications_open = false;
        shell.apply_self_snooze_minutes(None);
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some(context) = context else {
        return;
    };
    let request = &context.0;
    let result = api::end_snooze(&request.transport, &request.client, &request.session).await;
    if !context.current(&state.read()) {
        return;
    }
    match result {
        Ok(dnd) => state.write().merge_self_dnd(dnd),
        Err(error) => state
            .write()
            .report_failure(&error, "Snooze needs a connection", || {
                format!("Could not resume notifications: {error}")
            }),
    }
}

pub async fn clear_self_status(mut state: Signal<ShellState>) {
    let context = SelfMenuContext::begin(&mut state.write(), MutationKind::Status);
    state.write().clear_self_status();
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some(context) = context else {
        return;
    };
    let profile = serde_json::json!({
        "status_text": "",
        "status_emoji": "",
        "status_expiration": 0
    })
    .to_string();
    let request = &context.0;
    let result = api::set_user_profile(
        &request.transport,
        &request.client,
        &request.session,
        profile,
    )
    .await;
    if !context.current(&state.read()) {
        return;
    }
    match result {
        Ok(profile) => context.apply_profile(&mut state.write(), profile),
        Err(error) => {
            state
                .write()
                .report_failure(&error, "Status change needs a connection", || {
                    format!("Could not clear status: {error}")
                })
        }
    }
}

pub async fn sign_out_workspace(mut state: Signal<ShellState>) {
    let (account_id, name) = {
        let shell = state.read();
        let name = if shell.self_account.workspace_name.is_empty() {
            "this workspace".into()
        } else {
            shell.self_account.workspace_name.clone()
        };
        let id = shell
            .accounts
            .iter()
            .find(|account| account.active)
            .map(|account| account.id.clone());
        (id, name)
    };
    {
        let mut shell = state.write();
        shell.overlay = None;
        shell.self_menu_notifications_open = false;
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        state.write().show_toast(format!("Signed out of {name}"));
        return;
    }
    let Some(account_id) = account_id else {
        state
            .write()
            .show_toast(format!("Could not sign out of {name}"));
        return;
    };
    remove_account(state, account_id).await;
}

#[cfg(test)]
mod tests;
