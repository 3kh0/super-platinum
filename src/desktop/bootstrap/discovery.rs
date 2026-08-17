use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api;

use super::common::{credentials, persist_workspace};
use super::session::refresh_media;
use crate::state::MainView;
use crate::state::ShellState;

pub async fn search(mut state: Signal<ShellState>) {
    let query = state.read().search_query.trim().to_owned();
    if query.is_empty() {
        state.write().search_results.clear();
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
    state.write().search_loading = true;
    match api::fetch_search_messages(
        &transport,
        &client,
        &workspace_session,
        super_platinum_core::slack::api::SearchArgs::new(query.clone()),
    )
    .await
    {
        Ok(page) => {
            let requested_users = page
                .items
                .iter()
                .flat_map(|item| &item.messages)
                .filter_map(|message| message.user.clone())
                .collect::<std::collections::HashSet<_>>();
            let missing_users = {
                let shell = state.read();
                let workspace = shell.core.workspaces.get(&team);
                requested_users
                    .into_iter()
                    .filter(|user| {
                        workspace.is_none_or(|workspace| !workspace.users.contains_key(user))
                    })
                    .collect::<Vec<_>>()
            };
            let mut loaded_users = Vec::new();
            for chunk in missing_users.chunks(100) {
                if let Ok(users) =
                    api::fetch_users_info(&transport, &client, &workspace_session, chunk.to_vec())
                        .await
                {
                    loaded_users.extend(users);
                }
            }
            if !loaded_users.is_empty() {
                let mut shell = state.write();
                if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                    for user in loaded_users {
                        workspace.users.insert(user.id.clone(), user);
                    }
                }
            }
            if state.read().search_query.trim() != query {
                return;
            }
            let results = {
                let shell = state.read();
                let workspace = shell.core.workspaces.get(&team);
                let media = &shell.media;
                page.items
                    .into_iter()
                    .flat_map(|item| {
                        let channel_id = item
                            .channel
                            .as_ref()
                            .map(|channel| channel.id.clone())
                            .unwrap_or_default();
                        let channel_name = item
                            .channel
                            .as_ref()
                            .map(|channel| {
                                workspace
                                    .map(|workspace| {
                                        super_platinum_core::state::channel_display_name(
                                            workspace, channel,
                                        )
                                    })
                                    .unwrap_or_else(|| {
                                        super_platinum_core::state::channel_name(channel)
                                    })
                            })
                            .unwrap_or_else(|| "conversation".into());
                        item.messages.into_iter().map(move |message| {
                            let author = workspace
                                .map(|workspace| {
                                    super_platinum_core::state::message_author_name(
                                        workspace, &message,
                                    )
                                })
                                .unwrap_or_else(|| "Slack".into());
                            let text = workspace
                                .map(|workspace| {
                                    let vm =
                                        crate::message_vm::message_vm(workspace, &message, media);
                                    crate::view::rich::message_plain_text(&vm)
                                })
                                .unwrap_or_else(|| {
                                    super_platinum_core::state::message_text(&message)
                                })
                                .replace(['\u{e000}', '\u{e001}'], "");
                            crate::state::SearchResultVm {
                                channel_id: channel_id.clone(),
                                channel_name: channel_name.clone(),
                                ts: message.ts.clone().unwrap_or_default(),
                                author,
                                text,
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            };
            let mut shell = state.write();
            shell.search_results = results;
            shell.search_loading = false;
        }
        Err(error) => {
            let mut shell = state.write();
            shell.search_loading = false;
            shell.toast = Some(format!("Search failed: {error}"));
        }
    }
}

pub async fn open_profile(mut state: Signal<ShellState>, user: String) {
    {
        let mut shell = state.write();
        shell.profile_user = Some(user.clone());
        shell.overlay = Some(crate::state::Overlay::Profile);
    }
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
    if let Some(super_platinum_core::state::RealtimeStatus::Connected(connection)) = state
        .read()
        .core
        .workspaces
        .get(&team)
        .map(|workspace| &workspace.rt)
    {
        connection.send(super_platinum_core::slack::realtime::presence_query_frame(
            std::slice::from_ref(&user),
        ));
    }
    let (profile, extras) = tokio::join!(
        api::fetch_user_profile(&transport, &client, &workspace_session, user.clone()),
        api::fetch_user_profile_extras(&transport, &client, &workspace_session, user.clone()),
    );
    if state.read().profile_user.as_deref() != Some(&user) {
        return;
    }
    let mut shell = state.write();
    if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
        let member = workspace.users.entry(user.clone()).or_insert_with(|| {
            super_platinum_core::slack::models::User {
                id: user.clone(),
                ..Default::default()
            }
        });
        if let Ok(profile) = profile {
            member.real_name = profile
                .real_name
                .clone()
                .or_else(|| member.real_name.clone());
            member.profile = Some(profile);
        }
        if let Ok(extras) = extras {
            member.im_mpim_ids = extras.im_mpim_ids;
            member.has_more_mpims = extras.has_more_mpims;
        }
    }
    if let Some(url) = shell
        .core
        .workspaces
        .get(&team)
        .and_then(|workspace| workspace.avatar_url(&user))
    {
        shell.media.register(
            super_platinum_core::MediaAssetKind::Avatar,
            &url,
            "image/jpeg",
            url.contains("slack-edge.com") || url.contains("slack.com"),
        );
    }
    shell.refresh_from_core();
    drop(shell);
    persist_workspace(&state, &team);
    dioxus::prelude::spawn(refresh_media(state, transport));
}

pub async fn open_thread(mut state: Signal<ShellState>, channel: String, root_ts: String) {
    state.write().thread_root = Some(root_ts.clone());
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
    match api::fetch_replies(
        &transport,
        &client,
        &workspace_session,
        super_platinum_core::slack::api::RepliesArgs {
            channel: channel.clone(),
            ts: root_ts.clone(),
            limit: Some(200),
            ..Default::default()
        },
    )
    .await
    {
        Ok(page) => {
            let mut shell = state.write();
            if shell.thread_root.as_deref() != Some(&root_ts) {
                return;
            }
            let key = (team.clone(), channel.clone(), root_ts.clone());
            let thread_messages = {
                let messages = shell.core.threads.entry(key).or_default();
                for message in page.messages {
                    messages.upsert(super_platinum_core::state::visible_message(message));
                }
                messages.loaded = true;
                messages.messages.clone()
            };
            let workspace = shell.core.workspaces.get(&team);
            shell.thread_messages = thread_messages
                .iter()
                .map(|message| {
                    workspace
                        .map(|workspace| crate::state::message_vm(workspace, message, &shell.media))
                        .unwrap_or_else(|| crate::state::MessageVm {
                            id: message.ts.clone().unwrap_or_default(),
                            ts: message.ts.clone().unwrap_or_default(),
                            user_id: message.user.clone(),
                            author: "Slack".into(),
                            timestamp: message
                                .ts
                                .as_deref()
                                .map(super_platinum_core::state::format_ts_hm)
                                .unwrap_or_default(),
                            avatar_initials: "S".into(),
                            body: vec![crate::state::RichNode::Text(
                                super_platinum_core::state::message_text(message),
                            )],
                            edited: message.edited.is_some(),
                            reply_count: message.reply_count.unwrap_or(0),
                            ..Default::default()
                        })
                })
                .collect();
            drop(shell);
            super::session::hydrate_current_surface(state).await;
            let mut shell = state.write();
            if shell.thread_root.as_deref() == Some(&root_ts) {
                let key = (team.clone(), channel.clone(), root_ts.clone());
                let messages = shell
                    .core
                    .threads
                    .get(&key)
                    .map(|messages| messages.messages.clone())
                    .unwrap_or_default();
                if let Some(workspace) = shell.core.workspaces.get(&team) {
                    shell.thread_messages = messages
                        .iter()
                        .map(|message| crate::state::message_vm(workspace, message, &shell.media))
                        .collect();
                }
            }
        }
        Err(error) => state.write().toast = Some(format!("Thread failed: {error}")),
    }
}

pub async fn load_main_view(mut state: Signal<ShellState>, target: MainView) {
    {
        let mut shell = state.write();
        shell.main_view = target;
        if target != MainView::Activity {
            shell.activity_detail_open = false;
        }
        shell.profile_hover = None;
    }
    if target == MainView::Home || std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
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
    match target {
        MainView::Home => {}
        MainView::Unreads => {}
        MainView::Dms => {
            state.write().core.dms.loading = true;
            match api::fetch_client_dms(&transport, &client, &workspace_session, 100, None).await {
                Ok(page) => {
                    let mut shell = state.write();
                    shell.core.dms.entries.clear();
                    for entry in page.dms {
                        shell.core.dms.upsert(entry);
                    }
                    shell.core.dms.next_cursor = page
                        .response_metadata
                        .and_then(|metadata| metadata.next_cursor)
                        .filter(|cursor| !cursor.is_empty());
                    shell.core.dms.loading = false;
                    shell.core.dms.loaded = true;
                }
                Err(error) => {
                    let mut shell = state.write();
                    shell.core.dms.loading = false;
                    shell.toast = Some(format!("Direct messages failed: {error}"));
                }
            }
        }
        MainView::Threads => {
            state.write().core.threads_view.loading = true;
            match api::fetch_threads_view(&transport, &client, &workspace_session, 100, None, false)
                .await
            {
                Ok(page) => {
                    let mut shell = state.write();
                    shell.core.threads_view.items.clear();
                    for item in page.threads {
                        shell.core.threads_view.upsert(item);
                    }
                    shell.core.threads_view.max_ts = page.max_ts;
                    shell.core.threads_view.has_more = page.has_more;
                    shell.core.threads_view.loading = false;
                    shell.core.threads_view.loaded = true;
                }
                Err(error) => {
                    let mut shell = state.write();
                    shell.core.threads_view.loading = false;
                    shell.toast = Some(format!("Threads failed: {error}"));
                }
            }
        }
        MainView::Activity => {
            state.write().core.activity.loading = true;
            let unread_target = state
                .read()
                .core
                .workspaces
                .get(&team)
                .and_then(|workspace| workspace.activity_unread_count)
                .unwrap_or(0) as usize;
            let mut cursor = None;
            let mut page_index = 0;
            loop {
                let requested_cursor = cursor.clone();
                match api::fetch_activity_feed(
                    &transport,
                    &client,
                    &workspace_session,
                    20,
                    requested_cursor,
                    false,
                )
                .await
                {
                    Ok(page) => {
                        let next_cursor = page
                            .response_metadata
                            .and_then(|metadata| metadata.next_cursor)
                            .filter(|cursor| !cursor.is_empty());
                        let mut shell = state.write();
                        if page_index == 0 {
                            shell.core.activity.items.clear();
                        }
                        for item in page.items {
                            shell.core.activity.upsert(item);
                        }
                        shell.core.activity.next_cursor = next_cursor.clone();
                        shell.core.activity.loaded = true;
                        let loaded_unread = shell
                            .core
                            .activity
                            .items
                            .iter()
                            .filter(|item| item.is_unread)
                            .count();
                        page_index += 1;
                        let should_continue = next_cursor.is_some()
                            && page_index < 5
                            && unread_target > 0
                            && loaded_unread < unread_target;
                        drop(shell);
                        if !should_continue {
                            break;
                        }
                        cursor = next_cursor;
                    }
                    Err(error) => {
                        let mut shell = state.write();
                        if page_index == 0 {
                            shell.toast = Some(format!("Activity failed: {error}"));
                        } else {
                            eprintln!("super-platinum: older activity page failed: {error}");
                        }
                        break;
                    }
                }
            }
            state.write().core.activity.loading = false;
        }
    }
    super::session::hydrate_current_surface(state).await;
}
