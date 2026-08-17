use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api;

use super::common::{credentials, persist_workspace, refresh_history_at};
use crate::state::ShellState;

pub async fn refresh_selected_channel(mut state: Signal<ShellState>) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let (team, channel) = {
        let state = state.read();
        let (Some(team), Some(channel)) = (
            state.core.active_team.clone(),
            state.core.active_channel.clone(),
        ) else {
            return;
        };
        (team, channel)
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    let pending = state.read().core.pending_scroll_to.clone();
    let latest = pending.as_ref().and_then(|(pending_channel, target)| {
        (pending_channel == &channel)
            .then(|| match target {
                super_platinum_core::domain::PendingScrollTarget::Message(ts)
                | super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(ts) => {
                    Some(ts.clone())
                }
                super_platinum_core::domain::PendingScrollTarget::Latest => None,
            })
            .flatten()
    });
    refresh_history_at(
        &mut state,
        &transport,
        &client,
        &workspace_session,
        &team,
        channel.clone(),
        latest,
    )
    .await;
    super::session::hydrate_current_surface(state).await;
    persist_workspace(&state, &team);
    if let Some((pending_channel, target)) = pending
        && pending_channel == channel
    {
        scroll_to_target(&target).await;
        state.write().core.pending_scroll_to = None;
    }
}

pub async fn load_older(mut state: Signal<ShellState>) {
    let (team, channel, oldest, has_more) = {
        let shell = state.read();
        let (Some(team), Some(channel)) = (
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
        ) else {
            return;
        };
        let list = shell
            .core
            .workspaces
            .get(&team)
            .and_then(|workspace| workspace.messages.get(&channel));
        (
            team,
            channel,
            list.and_then(|list| list.messages.first())
                .and_then(|message| message.ts.clone()),
            list.is_some_and(|list| list.has_more_older),
        )
    };
    if !has_more || state.read().loading_older {
        return;
    }
    let Some(oldest) = oldest else { return };
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    state.write().loading_older = true;
    let previous_height =
        document_number("document.getElementById('message-timeline')?.scrollHeight ?? 0").await;
    refresh_history_at(
        &mut state,
        &transport,
        &client,
        &workspace_session,
        &team,
        channel,
        Some(oldest),
    )
    .await;
    super::session::hydrate_current_surface(state).await;
    state.write().loading_older = false;
    dioxus::document::eval(&format!(
        "requestAnimationFrame(() => {{ const t=document.getElementById('message-timeline'); if(t) t.scrollTop += t.scrollHeight - {previous_height}; }});"
    ));
    persist_workspace(&state, &team);
}

pub async fn scroll_to_pending(target: super_platinum_core::domain::PendingScrollTarget) {
    scroll_to_target(&target).await;
}

async fn scroll_to_target(target: &super_platinum_core::domain::PendingScrollTarget) {
    let selector = match target {
        super_platinum_core::domain::PendingScrollTarget::Message(ts)
        | super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(ts) => {
            // Prefer unread divider when anchoring first unread.
            if matches!(
                target,
                super_platinum_core::domain::PendingScrollTarget::FirstUnreadAfter(_)
            ) {
                ".unread-divider".to_owned()
            } else {
                format!(
                    "[data-message-id={}]",
                    serde_json::to_string(ts).unwrap_or_else(|_| "\"\"".into())
                )
            }
        }
        super_platinum_core::domain::PendingScrollTarget::Latest => String::new(),
    };
    let script = if selector.is_empty() {
        "requestAnimationFrame(() => { const t=document.getElementById('message-timeline'); if(t) t.scrollTop=t.scrollHeight; });".to_owned()
    } else {
        format!(
            "requestAnimationFrame(() => {{ const el = document.querySelector({}); if (el) el.scrollIntoView({{block:'center'}}); else {{ const t=document.getElementById('message-timeline'); if(t) t.scrollTop=t.scrollHeight; }} }});",
            serde_json::to_string(&selector).unwrap()
        )
    };
    dioxus::document::eval(&script);
}

async fn document_number(script: &str) -> f64 {
    dioxus::document::eval(&format!("dioxus.send({script});"))
        .recv::<f64>()
        .await
        .unwrap_or(0.0)
}

pub async fn mark_visible_read(mut state: Signal<ShellState>) {
    let (team, channel, latest) = {
        let shell = state.read();
        let (Some(team), Some(channel)) = (
            shell.core.active_team.clone(),
            shell.core.active_channel.clone(),
        ) else {
            return;
        };
        let Some(latest) = shell
            .core
            .workspaces
            .get(&team)
            .and_then(|workspace| workspace.messages.get(&channel))
            .and_then(|messages| messages.messages.last())
            .and_then(|message| message.ts.clone())
        else {
            return;
        };
        if shell
            .core
            .workspaces
            .get(&team)
            .and_then(|workspace| workspace.messages.get(&channel))
            .and_then(|messages| messages.last_read.as_deref())
            .is_some_and(|last| {
                !super_platinum_core::state::cmp_ts(Some(last), Some(&latest)).is_lt()
            })
        {
            return;
        }
        (team, channel, latest)
    };
    let target = super_platinum_core::domain::ReadTarget::Conversation {
        team: team.clone(),
        channel: channel.clone(),
    };
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    if !state
        .write()
        .core
        .pending_marks
        .insert((target.clone(), latest.clone()))
    {
        return;
    }
    let result = api::mark_channel(
        &transport,
        &client,
        &workspace_session,
        channel.clone(),
        latest.clone(),
    )
    .await;
    state
        .write()
        .core
        .pending_marks
        .remove(&(target, latest.clone()));
    if result.is_ok() {
        let mut shell = state.write();
        if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
            if let Some(messages) = workspace.messages.get_mut(&channel) {
                messages.last_read = Some(latest.clone());
                messages.unread_count = 0;
                messages.mention_count = 0;
            }
            if let Some(channel) = workspace.channels.get_mut(&channel) {
                channel.last_read = Some(latest);
                channel.has_unreads = false;
                channel.unread_count = Some(0);
                channel.unread_count_display = Some(0);
                channel.mention_count = Some(0);
            }
        }
        shell.refresh_from_core();
    }
}

pub async fn mark_all_read(mut state: Signal<ShellState>) {
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    let targets = state
        .read()
        .core
        .workspaces
        .get(&team)
        .map(|workspace| {
            workspace
                .messages
                .iter()
                .filter_map(|(channel, list)| {
                    let latest = list.messages.last()?.ts.clone()?;
                    (list.unread_count > 0 || list.mention_count > 0)
                        .then(|| (channel.clone(), latest))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (channel, latest) in &targets {
        if let Err(error) = api::mark_channel(
            &transport,
            &client,
            &workspace_session,
            channel.clone(),
            latest.clone(),
        )
        .await
        {
            state.write().toast = Some(format!("Mark read failed: {error}"));
            return;
        }
    }
    let mut shell = state.write();
    if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
        for (channel_id, latest) in targets {
            if let Some(messages) = workspace.messages.get_mut(&channel_id) {
                messages.last_read = Some(latest.clone());
                messages.unread_count = 0;
                messages.mention_count = 0;
            }
            if let Some(channel) = workspace.channels.get_mut(&channel_id) {
                channel.last_read = Some(latest);
                channel.has_unreads = false;
                channel.unread_count = Some(0);
                channel.unread_count_display = Some(0);
                channel.mention_count = Some(0);
            }
        }
    }
    shell.refresh_from_core();
    drop(shell);
    persist_workspace(&state, &team);
}
