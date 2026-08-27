use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api;
use super_platinum_core::slack::realtime::{self, ConnectParams};

use super::common::{credentials, persist_workspace, refresh_history};
use crate::state::ShellState;

pub async fn refresh(mut state: Signal<ShellState>) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };

    for workspace_session in workspaces {
        let team = workspace_session.team_id.clone();
        match api::fetch_user_boot(&transport, &client, &workspace_session).await {
            Ok(boot) => {
                let mut shell = state.write();
                if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                    workspace.apply_boot(boot);
                }
                shell.core.screen = super_platinum_core::state::Screen::Main;
                shell.loading = false;
                shell.refresh_from_core();
            }
            Err(error) => {
                let mut shell = state.write();
                shell.loading = false;
                shell.toast = Some(format!("Workspace refresh failed: {error}"));
                continue;
            }
        }
        if let Ok(counts) = api::fetch_counts(&transport, &client, &workspace_session).await
            && let Some(workspace) = state.write().core.workspaces.get_mut(&team)
        {
            workspace.apply_counts(counts);
        }
        if let Ok(dnd) = api::fetch_dnd_info(&transport, &client, &workspace_session).await
            && let Some(workspace) = state.write().core.workspaces.get_mut(&team)
        {
            workspace.self_dnd = dnd;
        }
        if let Ok(dms) = api::fetch_sidebar_dms(&transport, &client, &workspace_session).await
            && let Some(workspace) = state.write().core.workspaces.get_mut(&team)
        {
            workspace.apply_sidebar_dms(dms);
        }
        if let Ok(sections) =
            api::fetch_channel_sections(&transport, &client, &workspace_session).await
            && let Some(workspace) = state.write().core.workspaces.get_mut(&team)
        {
            workspace.apply_channel_sections(sections);
        }
        state.write().refresh_from_core();
        hydrate_surface_users(&mut state, &transport, &client, &workspace_session, &team).await;
        dioxus::prelude::spawn(refresh_media(state, transport.clone()));

        let active_channel = state.read().core.active_channel.clone().or_else(|| {
            state
                .read()
                .channels
                .first()
                .map(|channel| channel.id.clone())
        });
        if let Some(channel) = active_channel {
            refresh_history(
                &mut state,
                &transport,
                &client,
                &workspace_session,
                &team,
                channel,
            )
            .await;
            hydrate_surface_users(&mut state, &transport, &client, &workspace_session, &team).await;
            dioxus::prelude::spawn(refresh_media(state, transport.clone()));
        }
        persist_workspace(&state, &team);

        let params = ConnectParams {
            team: team.clone(),
            ws_url: realtime::flannel_url(&workspace_session.token, &team),
            d_cookie: state
                .read()
                .core
                .session
                .as_ref()
                .map(|session| session.d_cookie.clone())
                .unwrap_or_default(),
            user_agent: super_platinum_core::slack::xparams::Identity::from_capture().user_agent,
        };
        dioxus::prelude::spawn(crate::realtime::worker(state, params));
    }
}

pub(crate) async fn hydrate_current_surface(mut state: Signal<ShellState>) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
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
    hydrate_activity_messages(&mut state, &transport, &client, &workspace_session).await;
    hydrate_surface_emojis(&mut state, &transport, &client, &workspace_session, &team).await;
    hydrate_surface_channels(&mut state, &transport, &client, &workspace_session, &team).await;
    hydrate_surface_users(&mut state, &transport, &client, &workspace_session, &team).await;
    register_surface_avatars(&state, &team);
    refresh_media(state, transport).await;
}

/// Every Slack message currently projected onto a visible surface. Emoji, user,
/// and channel hydration all need the same set.
fn visit_surface_messages(
    shell: &ShellState,
    team: &str,
    visit: &mut impl FnMut(&super_platinum_core::slack::models::Message),
) {
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return;
    };
    if let Some(messages) = shell
        .core
        .active_channel
        .as_ref()
        .and_then(|channel| workspace.messages.get(channel))
    {
        for message in &messages.messages {
            visit(message);
        }
    }
    if let Some(messages) = shell.thread_root.as_ref().and_then(|root| {
        shell.core.active_channel.as_ref().and_then(|channel| {
            shell
                .core
                .threads
                .get(&(team.to_owned(), channel.clone(), root.clone()))
        })
    }) {
        for message in &messages.messages {
            visit(message);
        }
    }
    for message in shell.core.activity.hydrated.values() {
        visit(message);
    }
    for item in &shell.core.threads_view.items {
        for message in std::iter::once(&item.root_msg)
            .chain(&item.unread_replies)
            .chain(&item.latest_replies)
        {
            visit(message);
        }
    }
}

async fn hydrate_surface_emojis(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    team: &str,
) {
    let names = {
        let shell = state.read();
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return;
        };
        let mut names = std::collections::HashSet::new();
        visit_surface_messages(&shell, team, &mut |message| {
            super_platinum_core::state::collect_message_emoji_names(message, &mut names);
        });
        names.retain(|name| {
            !workspace.custom_emoji.contains_key(name)
                // Names Slack has already told us it does not know must not be
                // re-requested on every projection.
                && !shell
                    .core
                    .emoji_hydrated
                    .contains(&(team.to_owned(), name.clone()))
        });
        names.into_iter().collect::<Vec<_>>()
    };
    if names.is_empty() {
        return;
    }
    state
        .write()
        .core
        .emoji_hydrated
        .extend(names.iter().map(|name| (team.to_owned(), name.clone())));
    let mut loaded = Vec::new();
    for chunk in names.chunks(100) {
        match api::fetch_emojis_info(transport, client, workspace_session, chunk.to_vec()).await {
            Ok(emojis) => loaded.extend(emojis),
            Err(error) => {
                eprintln!("super-platinum: custom emoji hydration failed: {error}");
                // Allow a retry on the next projection.
                let mut shell = state.write();
                for name in chunk {
                    shell
                        .core
                        .emoji_hydrated
                        .remove(&(team.to_owned(), name.clone()));
                }
            }
        }
    }
    if loaded.is_empty() {
        return;
    }
    let mut shell = state.write();
    if let Some(workspace) = shell.core.workspaces.get_mut(team) {
        workspace.apply_emojis(loaded);
    }
    shell.refresh_from_core();
}

/// Unfurl footers and `#channel` chips print a raw id until the conversation is
/// known, so pull metadata for the ones the visible messages point at.
async fn hydrate_surface_channels(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    team: &str,
) {
    let channels = {
        let shell = state.read();
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return;
        };
        let mut ids = std::collections::HashSet::new();
        visit_surface_messages(&shell, team, &mut |message| {
            super_platinum_core::state::collect_message_channel_ids(message, &mut ids);
        });
        ids.retain(|id| {
            !workspace
                .channels
                .get(id)
                .is_some_and(|channel| channel.name.is_some())
                && !shell
                    .core
                    .channel_hydrated
                    .contains(&(team.to_owned(), id.clone()))
        });
        ids.into_iter().collect::<Vec<_>>()
    };
    if channels.is_empty() {
        return;
    }
    state.write().core.channel_hydrated.extend(
        channels
            .iter()
            .map(|channel| (team.to_owned(), channel.clone())),
    );
    let mut loaded = Vec::new();
    for chunk in channels.chunks(50) {
        match api::fetch_channels_info(transport, client, workspace_session, chunk.to_vec()).await {
            Ok(channels) => loaded.extend(channels),
            Err(error) => eprintln!("super-platinum: unfurl channel hydration failed: {error}"),
        }
    }
    if loaded.is_empty() {
        return;
    }
    let mut shell = state.write();
    if let Some(workspace) = shell.core.workspaces.get_mut(team) {
        workspace.apply_channels_info(loaded);
    }
    shell.refresh_from_core();
}

fn register_surface_avatars(state: &Signal<ShellState>, team: &str) {
    let shell = state.read();
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return;
    };
    for item in &shell.core.activity.items {
        let user = item.author().or_else(|| {
            let channel = item.channel()?;
            let message = item
                .preview_ts()
                .and_then(|ts| {
                    shell
                        .core
                        .activity
                        .hydrated
                        .get(&(channel.to_owned(), ts.to_owned()))
                })
                .or_else(|| {
                    item.ts().and_then(|ts| {
                        shell
                            .core
                            .activity
                            .hydrated
                            .get(&(channel.to_owned(), ts.to_owned()))
                    })
                })?;
            message.user.as_deref()
        });
        let Some((user, url)) = user.and_then(|user| Some((user, workspace.avatar_url(user)?)))
        else {
            continue;
        };
        shell.media.register_avatar(user, &url);
    }
    for item in &shell.core.threads_view.items {
        for message in std::iter::once(&item.root_msg)
            .chain(&item.unread_replies)
            .chain(&item.latest_replies)
        {
            let Some((user, url)) = message
                .user
                .as_deref()
                .and_then(|user| Some((user, workspace.avatar_url(user)?)))
            else {
                continue;
            };
            shell.media.register_avatar(user, &url);
        }
    }
}

async fn hydrate_activity_messages(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
) {
    let message_ids = {
        let shell = state.read();
        let mut groups = std::collections::HashMap::<String, Vec<String>>::new();
        for item in &shell.core.activity.items {
            let Some(channel) = item.channel() else {
                continue;
            };
            for ts in item.request_ts() {
                if shell
                    .core
                    .activity
                    .hydrated
                    .contains_key(&(channel.to_owned(), ts.clone()))
                {
                    continue;
                }
                let bucket = groups.entry(channel.to_owned()).or_default();
                if !bucket.contains(&ts) {
                    bucket.push(ts);
                }
            }
        }
        groups.into_iter().collect::<Vec<_>>()
    };
    if message_ids.is_empty() {
        return;
    }

    // Slack rejects messages.list requests spanning too many conversations.
    // Activity pages can easily cover dozens, so hydrate them in bounded batches.
    for chunk in message_ids.chunks(20) {
        let page =
            match api::fetch_messages_list(transport, client, workspace_session, chunk.to_vec())
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    eprintln!("super-platinum: activity message hydration batch failed: {error}");
                    continue;
                }
            };
        let mut shell = state.write();
        for (channel, data) in page.messages_data {
            for message in data.messages {
                if let Some(ts) = message.ts.clone() {
                    shell
                        .core
                        .activity
                        .hydrated
                        .insert((channel.clone(), ts), message);
                }
            }
        }
    }
}

async fn hydrate_surface_users(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    team: &str,
) {
    let requested = {
        let shell = state.read();
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return;
        };
        let mut ids = std::collections::HashSet::new();
        for section in &shell.sidebar_sections {
            for index in &section.channel_indices {
                if let Some(user) = shell
                    .channels
                    .get(*index)
                    .and_then(|channel| channel.user_id.as_ref())
                {
                    ids.insert(user.clone());
                }
            }
        }
        // Mentions inside Block Kit and unfurl authors need display names and
        // avatars too, not just the message authors.
        visit_surface_messages(&shell, team, &mut |message| {
            super_platinum_core::state::collect_message_user_ids(message, &mut ids);
        });
        if shell.main_view == crate::state::MainView::Unreads {
            for channel in workspace
                .channels
                .values()
                .filter(|channel| workspace.unread_total(channel) > 0)
            {
                if let Some(messages) = workspace.messages.get(&channel.id) {
                    ids.extend(
                        messages
                            .messages
                            .iter()
                            .rev()
                            .take(3)
                            .filter_map(|message| message.user.clone()),
                    );
                }
            }
        }
        for entry in &shell.core.dms.entries {
            if let Some(user) = entry
                .channel
                .as_ref()
                .and_then(super_platinum_core::state::dm_user_id)
            {
                ids.insert(user.to_owned());
            }
            if let Some(user) = entry
                .message
                .as_ref()
                .and_then(|message| message.user.clone())
            {
                ids.insert(user);
            }
        }
        ids.extend(
            shell
                .core
                .activity
                .items
                .iter()
                .filter_map(|item| item.author().map(str::to_owned)),
        );
        ids.extend(
            shell
                .core
                .activity
                .hydrated
                .values()
                .filter_map(|message| message.user.clone()),
        );
        for item in &shell.core.threads_view.items {
            ids.extend(
                std::iter::once(&item.root_msg)
                    .chain(&item.unread_replies)
                    .chain(&item.latest_replies)
                    .filter_map(|message| message.user.clone()),
            );
        }
        if let Some(user) = shell.profile_user.as_ref() {
            ids.insert(user.clone());
        }
        ids.insert(workspace.self_user_id.clone());
        ids.retain(|id| {
            if id.trim().is_empty() || shell.core.avatar_profile_hydrated.contains(id) {
                return false;
            }
            workspace.users.get(id).is_none_or(|user| {
                workspace.avatar_url(id).is_none()
                    || super_platinum_core::state::display_name(Some(user), id) == *id
            })
        });
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort();
        ids
    };
    if requested.is_empty() {
        return;
    }

    state
        .write()
        .core
        .avatar_profile_hydrated
        .extend(requested.iter().cloned());
    let mut loaded = Vec::new();
    let mut failed = false;
    for chunk in requested.chunks(100) {
        match api::fetch_users_info(transport, client, workspace_session, chunk.to_vec()).await {
            Ok(users) => loaded.extend(users),
            Err(error) => {
                failed = true;
                eprintln!("super-platinum: visible user hydration failed: {error}");
            }
        }
    }

    let mut shell = state.write();
    if failed {
        for id in &requested {
            shell.core.avatar_profile_hydrated.remove(id);
        }
    }
    if let Some(workspace) = shell.core.workspaces.get_mut(team) {
        for user in loaded {
            workspace.users.insert(user.id.clone(), user);
        }
    }
    shell.refresh_from_core();
}

pub async fn sign_in(mut state: Signal<ShellState>) {
    let media = state.read().media.clone();
    state.write().loading = true;
    let Ok(executable) = std::env::current_exe() else {
        state.write().toast = Some("Could not locate the Super Platinum executable".into());
        return;
    };
    let status = tokio::process::Command::new(executable)
        .env("SUPER_PLATINUM_AUTH", "1")
        .env("SUPER_PLATINUM_AUTH_ADD", "1")
        .status()
        .await;
    match status {
        Ok(status) if status.success() => {
            *state.write() = ShellState::from_environment(media);
            refresh(state).await;
        }
        Ok(_) => {
            let mut shell = state.write();
            shell.loading = false;
            shell.toast = Some("Slack sign-in was not completed".into());
        }
        Err(error) => {
            let mut shell = state.write();
            shell.loading = false;
            shell.toast = Some(format!("Could not start Slack sign-in: {error}"));
        }
    }
}

pub async fn switch_account(mut state: Signal<ShellState>, account_id: String) {
    let result = tokio::task::spawn_blocking(move || {
        super_platinum_core::config::set_active_account(&account_id)
    })
    .await;
    match result {
        Ok(Ok(())) => reload_account(state).await,
        Ok(Err(error)) => state.write().toast = Some(format!("Account switch failed: {error}")),
        Err(error) => state.write().toast = Some(format!("Account switch stopped: {error}")),
    }
}

pub async fn remove_account(mut state: Signal<ShellState>, account_id: String) {
    let result = tokio::task::spawn_blocking(move || {
        super_platinum_core::config::remove_account(&account_id)
    })
    .await;
    match result {
        Ok(Ok(_)) => reload_account(state).await,
        Ok(Err(error)) => state.write().toast = Some(format!("Account removal failed: {error}")),
        Err(error) => state.write().toast = Some(format!("Account removal stopped: {error}")),
    }
}

async fn reload_account(mut state: Signal<ShellState>) {
    let media = state.read().media.clone();
    *state.write() = ShellState::from_environment(media);
    refresh(state).await;
}

pub(super) async fn refresh_media(
    state: Signal<ShellState>,
    transport: std::sync::Arc<super_platinum_core::slack::Transport>,
) {
    let media = state.read().media.clone();
    // The media generation is bumped by `runtime::ticks` as bytes land, not
    // here: a single slow host would otherwise hold back every avatar that
    // already arrived in the same batch.
    media.load_pending(transport).await;
}
