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
    // Re-read the anchor instead of replaying the copy taken before the fetch.
    // It may already have been applied, or dropped because the reader scrolled
    // away — replaying it there is what yanked a reader at the bottom of a busy
    // channel back up to the unread divider seconds after opening it.
    let pending = state.read().core.pending_scroll_to.clone();
    if let Some((pending_channel, target)) = pending
        && pending_channel == channel
        && scroll_to_target(&target).await
    {
        state.write().core.pending_scroll_to = None;
    }
    mark_visible_read(state).await;
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

/// Returns whether the anchor actually landed on its row. A pending anchor is
/// only spent once it does: on a cold channel the rows do not exist yet, and
/// dropping the anchor there would leave the reader wherever the fallback put
/// them.
pub async fn scroll_to_pending(target: super_platinum_core::domain::PendingScrollTarget) -> bool {
    scroll_to_target(&target).await
}

async fn scroll_to_target(target: &super_platinum_core::domain::PendingScrollTarget) -> bool {
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
        "requestAnimationFrame(() => { const t=document.getElementById('message-timeline'); if(t) t.scrollTop=t.scrollHeight; }); dioxus.send(true);".to_owned()
    } else {
        format!(
            "const el = document.querySelector({});
             if (el) requestAnimationFrame(() => el.scrollIntoView({{block:'center'}}));
             else requestAnimationFrame(() => {{ const t=document.getElementById('message-timeline'); if(t) t.scrollTop=t.scrollHeight; }});
             dioxus.send(Boolean(el));",
            serde_json::to_string(&selector).unwrap()
        )
    };
    dioxus::document::eval(&script)
        .recv::<bool>()
        .await
        .unwrap_or(false)
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
        // Pin the divider before the mark moves `last_read` past it. A channel
        // opened cold had no messages when it was selected, so this is the first
        // moment its read position is known.
        if shell.divider_at(&channel).is_none()
            && let Some(previous) = shell
                .core
                .workspaces
                .get(&team)
                .and_then(|workspace| workspace.messages.get(&channel))
                .and_then(|messages| messages.last_read.clone())
        {
            shell.unread_anchor = Some((channel.clone(), previous));
        }
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
        // Reading the channel covers its mentions, keywords and reactions in the
        // Activity feed. Thread items keep their own unread state.
        if shell.core.activity.mark_channel_read(&channel) {
            let unread = shell.core.activity.unread_count();
            if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                workspace.activity_unread_count = Some(unread);
            }
        }
        shell.refresh_from_core();
    }
}

/// Marks an opened thread read. Slack tracks thread unreads separately from the
/// channel, so `conversations.mark` never clears a thread's badge — only
/// `subscriptions.thread.mark` does.
pub async fn mark_thread_read(mut state: Signal<ShellState>, channel: String, root_ts: String) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let (team, latest) = {
        let shell = state.read();
        let Some(team) = shell.core.active_team.clone() else {
            return;
        };
        let Some(latest) = shell
            .core
            .threads
            .get(&(team.clone(), channel.clone(), root_ts.clone()))
            .and_then(|messages| messages.messages.last())
            .and_then(|message| message.ts.clone())
        else {
            return;
        };
        (team, latest)
    };
    let target = super_platinum_core::domain::ReadTarget::Thread {
        team: team.clone(),
        channel: channel.clone(),
        root_ts: root_ts.clone(),
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
    let result = api::mark_thread(
        &transport,
        &client,
        &workspace_session,
        channel,
        root_ts,
        latest.clone(),
    )
    .await;
    state.write().core.pending_marks.remove(&(target, latest));
    if let Err(error) = result {
        eprintln!("super-platinum: thread mark failed: {error}");
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
            // Offline is silent: the read mark is retried on the next visit,
            // and the rail is already saying why nothing is landing.
            state
                .write()
                .report_failure(&error, "", || format!("Mark read failed: {error}"));
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
