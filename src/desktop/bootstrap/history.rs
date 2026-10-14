use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api;

use super::common::{credentials, persist_workspace, refresh_history_at};
use crate::realtime::activity_counts::{CursorOwner, confirm_channel_read, confirm_thread_read};
use crate::state::ShellState;

pub async fn refresh_selected_channel(mut state: Signal<ShellState>) {
    let (team, channel, generation) = {
        let state = state.read();
        let (Some(team), Some(channel)) = (
            state.core.active_team.clone(),
            state.core.active_channel.clone(),
        ) else {
            return;
        };
        (team, channel, state.channel_generation)
    };
    // Park the conversation before any network work. The scroll container is
    // shared, so a newly opened channel otherwise keeps the offset of the one
    // left behind — on a shorter transcript that is an empty pane, and on a
    // longer one it is a random position in the middle of history.
    let opening = state.read().core.pending_scroll_to.clone();
    let mark_on_open = opening.as_ref().is_some_and(|(_, target)| {
        !matches!(
            target,
            super_platinum_core::domain::PendingScrollTarget::Message(_)
        )
    });
    if let Some((pending_channel, target)) = opening
        && pending_channel == channel
        && scroll_to_target(state, &target).await
        && state.read().channel_generation == generation
        // An explicit message can be present in cache before the surrounding
        // history arrives. Keep its identity through that merge: prepended
        // rows change both virtual indices and spacer heights.
        && (!matches!(target, super_platinum_core::domain::PendingScrollTarget::Message(_))
            || std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some())
    {
        state.write().core.pending_scroll_to = None;
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    if state.read().channel_generation != generation {
        return;
    }
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
    // The reader moved on while this was in flight; the conversation on screen
    // now has its own refresh, and scrolling or marking it here would act on the
    // wrong one.
    if state.read().channel_generation != generation {
        return;
    }
    let is_connect = state
        .read()
        .core
        .workspaces
        .get(&team)
        .and_then(|workspace| workspace.channels.get(&channel))
        .is_some_and(|channel| channel.is_ext_shared);
    if is_connect {
        let result =
            api::fetch_conversation_teams(&transport, &client, &workspace_session, channel.clone())
                .await;
        // Do not hold a signal read guard in a match guard and then acquire a
        // write guard in its arm. Dioxus keeps that temporary alive for the
        // whole arm, which deadlocks the serial UI dispatcher.
        if state.read().channel_generation != generation {
            return;
        }
        match result {
            Ok(teams) => {
                let mut shell = state.write();
                if let Some(channel) = shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .and_then(|workspace| workspace.channels.get_mut(&channel))
                {
                    channel.connected_teams = teams;
                }
                shell.refresh_from_core();
            }
            Err(error) => {
                eprintln!("super-platinum: Slack Connect team hydration failed: {error}");
            }
        }
    }
    if state.read().channel_generation != generation {
        return;
    }
    super::session::hydrate_current_surface(state).await;
    persist_workspace(&state, &team);
    // Re-read the anchor instead of replaying the copy taken before the fetch.
    // It may already have been applied, or dropped because the reader scrolled
    // away — replaying it there is what yanked a reader at the bottom of a busy
    // channel back up to the unread divider seconds after opening it.
    let pending = state.read().core.pending_scroll_to.clone();
    if let Some((pending_channel, target)) = pending
        && pending_channel == channel
        && scroll_to_target(state, &target).await
        && state.read().channel_generation == generation
    {
        state.write().core.pending_scroll_to = None;
    }
    if state.read().channel_generation != generation {
        return;
    }
    mark_channel_read(state, mark_on_open).await;
}

pub async fn load_older(mut state: Signal<ShellState>) {
    let (team, channel, oldest, has_more, generation, epoch) = {
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
            shell.channel_generation,
            shell.core.account_epoch,
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
    refresh_history_at(
        &mut state,
        &transport,
        &client,
        &workspace_session,
        &team,
        channel.clone(),
        Some(oldest),
    )
    .await;
    // A history response may warm its original conversation's cache, but must
    // not hydrate or change the loading flag of a newer navigation/account.
    let current = {
        let shell = state.read();
        shell.channel_generation == generation
            && shell.core.account_epoch == epoch
            && shell.core.active_team.as_ref() == Some(&team)
            && shell.core.active_channel.as_ref() == Some(&channel)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|active| std::sync::Arc::ptr_eq(active, &transport))
    };
    if !current {
        return;
    }
    state.write().loading_older = false;
    persist_workspace(&state, &team);
    // timeline_anchor.js preserves the visible row across the prepend. A
    // second scrollHeight adjustment here would apply the insertion twice,
    // and could also override scrolling performed during the network wait.
    super::session::hydrate_current_surface(state).await;
}

/// Returns whether the anchor actually landed on its row. A pending anchor is
/// only spent once it does: on a cold channel the rows do not exist yet, and
/// dropping the anchor there would leave the reader wherever the fallback put
/// them.
pub async fn scroll_to_pending(
    state: Signal<ShellState>,
    target: super_platinum_core::domain::PendingScrollTarget,
) -> bool {
    scroll_to_target(state, &target).await
}

async fn scroll_to_target(
    state: Signal<ShellState>,
    target: &super_platinum_core::domain::PendingScrollTarget,
) -> bool {
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
    let args = {
        let shell = state.read();
        serde_json::json!({
            "scope":shell.timeline_scope(),
            "selector":selector,
            "latestId":shell.channel_rows.last().map(|message| &message.id),
        })
    };
    // Keep the evaluation alive through the animation frames. Returning before
    // the async navigation completes can dispose its result channel too early.
    let script = format!(
        "return await ({})({args});",
        include_str!("../timeline_navigation.js")
    );
    dioxus::document::eval(&script)
        .await
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub async fn mark_visible_read(state: Signal<ShellState>) {
    mark_channel_read(state, false).await;
}

async fn mark_channel_read(mut state: Signal<ShellState>, opening: bool) {
    if crate::fixture::is_fixture() {
        return;
    }
    let (team, channel, latest, previous, revision, scope) = {
        let shell = state.read();
        let focused = crate::read_visibility::focused();
        if crate::unread::blocks_auto_read(&shell) {
            return;
        }
        // Opening a conversation retains the existing mark-on-open behavior.
        // Later refreshes must not consume new messages below a history reader.
        if !shell.reading_latest(focused)
            && !(opening
                && focused
                && shell.transcript_visible()
                && shell.core.pending_scroll_to.is_none())
        {
            return;
        }
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
        let previous = shell
            .core
            .workspaces
            .get(&team)
            .and_then(|workspace| workspace.read_cursor(&channel))
            .map(str::to_owned);
        let revision = shell
            .core
            .workspaces
            .get(&team)
            .map_or(0, |w| w.read_revision(&channel));
        (
            team,
            channel,
            latest,
            previous,
            revision,
            shell.timeline_scope(),
        )
    };
    let target = super_platinum_core::domain::ReadTarget::Conversation {
        team: team.clone(),
        channel: channel.clone(),
    };
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let epoch = state.read().core.account_epoch;
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    let Some(owner) = CursorOwner::capture(&state.peek().core, &transport, &workspace_session)
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
    if state.peek().core.account_epoch != epoch
        || !state
            .peek()
            .core
            .transport
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &transport))
    {
        return;
    }
    state
        .write()
        .core
        .pending_marks
        .remove(&(target, latest.clone()));
    if !owner.current(&state.peek().core) {
        return;
    }
    if result.is_ok() {
        let mut shell = state.write();
        // Pin the divider before the mark moves `last_read` past it. A channel
        // opened cold had no messages when it was selected, so this is the first
        // moment its read position is known.
        if shell.timeline_scope() == scope
            && shell.divider_at(&channel).is_none()
            && let Some(previous) = shell
                .core
                .workspaces
                .get(&team)
                .and_then(|workspace| workspace.messages.get(&channel))
                .and_then(|messages| messages.last_read.clone())
        {
            shell.unread_anchor = Some((channel.clone(), previous));
        }
        confirm_channel_read(
            &mut shell.core,
            &team,
            &channel,
            &latest,
            previous.as_deref(),
            revision,
        );
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
        if !shell.reading_thread(crate::read_visibility::focused(), &channel, &root_ts) {
            return;
        }
        let Some(team) = shell.core.active_team.clone() else {
            return;
        };
        let Some(latest) = shell
            .thread_messages
            .iter()
            .rev()
            .find(|message| !message.pending)
            .map(|message| message.ts.clone())
        else {
            return;
        };
        (team, latest)
    };
    let key = (team.clone(), channel.clone(), root_ts.clone());
    let previous = state.peek().core.thread_cursor(&key).map(str::to_owned);
    let revision = state.peek().core.thread_read_revision(&key);
    if previous.as_deref().is_some_and(|cursor| {
        !super_platinum_core::state::cmp_ts(Some(cursor), Some(&latest)).is_lt()
    }) {
        return;
    }
    crate::thread_unread::remember_anchor(&mut state.write());
    let target = super_platinum_core::domain::ReadTarget::Thread {
        team: team.clone(),
        channel: channel.clone(),
        root_ts: root_ts.clone(),
    };
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let epoch = state.read().core.account_epoch;
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        return;
    };
    let Some(owner) = CursorOwner::capture(&state.peek().core, &transport, &workspace_session)
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
    if state.peek().core.account_epoch != epoch
        || !state
            .peek()
            .core
            .transport
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &transport))
    {
        return;
    }
    let mut shell = state.write();
    shell.core.pending_marks.remove(&(target, latest.clone()));
    if !owner.current(&shell.core) {
        return;
    }
    if result.is_ok() {
        confirm_thread_read(&mut shell.core, &key, &latest, revision);
        shell.refresh_from_core();
    } else if let Err(error) = result {
        shell.report_failure(&error, "", || "Could not update thread read status.".into());
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
    let epoch = state.peek().core.account_epoch;
    for (channel, latest) in targets {
        // Explicit unread and bulk read operations share the same ordering as
        // automatic marks. A batch must not overwrite an unread still in flight.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while crate::unread::busy_in(&state.peek(), epoch, &team, &channel) {
            if state.peek().core.account_epoch != epoch || std::time::Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let (previous, revision, scope) = {
            let shell = state.peek();
            if shell.core.account_epoch != epoch
                || !shell
                    .core
                    .transport
                    .as_ref()
                    .is_some_and(|active| std::sync::Arc::ptr_eq(active, &transport))
            {
                return;
            }
            (
                shell
                    .core
                    .workspaces
                    .get(&team)
                    .and_then(|w| w.read_cursor(&channel))
                    .map(str::to_owned),
                shell
                    .core
                    .workspaces
                    .get(&team)
                    .map_or(0, |w| w.read_revision(&channel)),
                shell.timeline_scope(),
            )
        };
        let target = super_platinum_core::domain::ReadTarget::Conversation {
            team: team.clone(),
            channel: channel.clone(),
        };
        let Some(owner) = CursorOwner::capture(&state.peek().core, &transport, &workspace_session)
        else {
            return;
        };
        if !state
            .write()
            .core
            .pending_marks
            .insert((target.clone(), latest.clone()))
        {
            continue;
        }
        let result = api::mark_channel(
            &transport,
            &client,
            &workspace_session,
            channel.clone(),
            latest.clone(),
        )
        .await;
        let mut shell = state.write();
        if shell.core.account_epoch != epoch
            || !shell
                .core
                .transport
                .as_ref()
                .is_some_and(|active| std::sync::Arc::ptr_eq(active, &transport))
        {
            return;
        }
        shell.core.pending_marks.remove(&(target, latest.clone()));
        if !owner.current(&shell.core) {
            return;
        }
        if let Err(error) = result {
            shell.report_failure(&error, "", || {
                "Could not mark conversations as read. Please try again.".into()
            });
            return;
        }
        confirm_channel_read(
            &mut shell.core,
            &team,
            &channel,
            &latest,
            previous.as_deref(),
            revision,
        );
        if shell.timeline_scope() == scope && shell.core.active_channel.as_ref() == Some(&channel) {
            crate::unread::release(&mut shell);
        }
        shell.refresh_from_core();
    }
    persist_workspace(&state, &team);
}
