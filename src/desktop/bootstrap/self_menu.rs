use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api;

use super::common::credentials;
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

fn active_workspace_session(
    state: &Signal<ShellState>,
) -> Option<(
    std::sync::Arc<super_platinum_core::slack::Transport>,
    super_platinum_core::slack::SlackClient,
    super_platinum_core::config::WorkspaceSession,
)> {
    let (transport, client, workspaces) = credentials(state)?;
    let team = state.read().core.active_team.clone()?;
    let workspace = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)?;
    Some((transport, client, workspace))
}

pub async fn set_self_presence(mut state: Signal<ShellState>, away: bool) {
    let presence = if away {
        PresenceVm::Away
    } else {
        PresenceVm::Active
    };
    state.write().apply_self_presence(presence);
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspace)) = active_workspace_session(&state) else {
        return;
    };
    let value = if away { "away" } else { "auto" };
    if let Err(error) = api::set_presence(&transport, &client, &workspace, value.into()).await {
        state.write().toast = Some(format!("Could not update presence: {error}"));
    }
}

pub async fn pause_notifications(mut state: Signal<ShellState>, minutes: u32) {
    {
        let mut shell = state.write();
        shell.self_menu_notifications_open = false;
        shell.apply_self_snooze_minutes(Some(minutes));
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspace)) = active_workspace_session(&state) else {
        return;
    };
    match api::set_snooze(&transport, &client, &workspace, minutes).await {
        Ok(dnd) => state.write().merge_self_snooze(dnd),
        Err(error) => state.write().toast = Some(format!("Could not pause notifications: {error}")),
    }
}

pub async fn resume_notifications(mut state: Signal<ShellState>) {
    {
        let mut shell = state.write();
        shell.self_menu_notifications_open = false;
        shell.apply_self_snooze_minutes(None);
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspace)) = active_workspace_session(&state) else {
        return;
    };
    match api::end_snooze(&transport, &client, &workspace).await {
        Ok(dnd) => state.write().merge_self_dnd(dnd),
        Err(error) => {
            state.write().toast = Some(format!("Could not resume notifications: {error}"))
        }
    }
}

pub async fn clear_self_status(mut state: Signal<ShellState>) {
    state.write().clear_self_status();
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspace)) = active_workspace_session(&state) else {
        return;
    };
    let profile = serde_json::json!({
        "status_text": "",
        "status_emoji": "",
        "status_expiration": 0
    })
    .to_string();
    match api::set_user_profile(&transport, &client, &workspace, profile).await {
        Ok(profile) => {
            let mut shell = state.write();
            let user = shell.self_account.user_id.clone();
            if let Some(team) = shell.core.active_team.clone()
                && let Some(workspace) = shell.core.workspaces.get_mut(&team)
                && let Some(user) = workspace.users.get_mut(&user)
            {
                user.profile = Some(profile);
            }
        }
        Err(error) => state.write().toast = Some(format!("Could not clear status: {error}")),
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
        state.write().toast = Some(format!("Signed out of {name}"));
        return;
    }
    let Some(account_id) = account_id else {
        state.write().toast = Some(format!("Could not sign out of {name}"));
        return;
    };
    remove_account(state, account_id).await;
}

#[cfg(test)]
mod tests {
    use super::minutes_until_local_hour;

    #[test]
    fn until_morning_uses_the_next_8am_in_the_local_offset() {
        let ten_am = 10 * 3_600;
        assert_eq!(minutes_until_local_hour(ten_am, 0, 8), 22 * 60);
        let seven_am = 7 * 3_600;
        assert_eq!(minutes_until_local_hour(seven_am, 0, 8), 60);
    }
}
