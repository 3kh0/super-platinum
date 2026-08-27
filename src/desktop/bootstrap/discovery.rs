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
        shell.profile_hover = None;
        shell.profile_menu_open = false;
        shell.thread_root = None;
        shell.thread_messages.clear();
        shell.core.profile_pane = Some(super_platinum_core::domain::ProfilePaneState {
            user: user.clone(),
            loading: true,
            error: None,
        });
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        if let Some(pane) = state.write().core.profile_pane.as_mut() {
            pane.loading = false;
        }
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
    let needs_fields = !state.read().core.profile_fields.contains_key(&team);
    let (profile, extras, fields) = tokio::join!(
        api::fetch_user_profile(&transport, &client, &workspace_session, user.clone()),
        api::fetch_user_profile_extras(&transport, &client, &workspace_session, user.clone()),
        async {
            if needs_fields {
                api::fetch_team_profile_fields(&transport, &client, &workspace_session).await
            } else {
                Ok(Vec::new())
            }
        },
    );
    if state.read().profile_user.as_deref() != Some(&user) {
        return;
    }
    let profile_emojis = profile
        .as_ref()
        .ok()
        .map(profile_emoji_names)
        .unwrap_or_default()
        .into_iter()
        .filter(|name| !super_platinum_core::state::is_standard_emoji(name))
        .filter(|name| {
            state
                .read()
                .core
                .workspaces
                .get(&team)
                .is_some_and(|workspace| !workspace.custom_emoji.contains_key(name))
        })
        .take(100)
        .collect::<Vec<_>>();
    let missing_dm_ids = extras
        .as_ref()
        .map(|extras| {
            extras
                .im_mpim_ids
                .iter()
                .filter(|id| {
                    state
                        .read()
                        .core
                        .workspaces
                        .get(&team)
                        .is_none_or(|workspace| !workspace.channels.contains_key(*id))
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (recent_channels, hydrated_emojis) = tokio::join!(
        async {
            if missing_dm_ids.is_empty() {
                Vec::new()
            } else {
                api::fetch_channels_info(&transport, &client, &workspace_session, missing_dm_ids)
                    .await
                    .unwrap_or_default()
            }
        },
        async {
            if profile_emojis.is_empty() {
                Ok(Vec::new())
            } else {
                api::fetch_emojis_info(
                    &transport,
                    &client,
                    &workspace_session,
                    profile_emojis.clone(),
                )
                .await
            }
        },
    );
    if state.read().profile_user.as_deref() != Some(&user) {
        return;
    }
    let mut shell = state.write();
    let profile_error = profile.as_ref().err().map(ToString::to_string);
    if hydrated_emojis.is_ok() {
        shell.core.emoji_hydrated.extend(
            profile_emojis
                .iter()
                .map(|name| (team.clone(), name.clone())),
        );
    }
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
        for channel in recent_channels {
            workspace.channels.insert(channel.id.clone(), channel);
        }
        if let Ok(emojis) = hydrated_emojis {
            workspace.apply_emojis(emojis);
        }
    }
    if needs_fields && let Ok(fields) = fields {
        shell.core.profile_fields.insert(team.clone(), fields);
    }
    if let Some(pane) = shell.core.profile_pane.as_mut()
        && pane.user == user
    {
        pane.loading = false;
        pane.error = profile_error;
    }
    if let Some(url) = shell
        .core
        .workspaces
        .get(&team)
        .and_then(|workspace| workspace.avatar_url(&user))
    {
        shell.media.register_avatar(&user, &url);
    }
    shell.refresh_from_core();
    drop(shell);
    persist_workspace(&state, &team);
    dioxus::prelude::spawn(refresh_media(state, transport));
}

fn profile_emoji_names(
    profile: &super_platinum_core::slack::models::UserProfile,
) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    let mut collect = |text: &str| {
        names.extend(super_platinum_core::state::emoji_names_in_text(text));
    };
    if let Some(status) = profile.status_emoji.as_deref() {
        collect(status);
    }
    for value in profile.fields.values() {
        if let Some(alt) = value.alt.as_deref() {
            collect(alt);
        }
        visit_profile_field_strings(&value.value, &mut collect);
    }
    names
}

fn visit_profile_field_strings(value: &serde_json::Value, visit: &mut impl FnMut(&str)) {
    match value {
        serde_json::Value::String(value) => visit(value),
        serde_json::Value::Array(values) => {
            for value in values {
                visit_profile_field_strings(value, visit);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                visit_profile_field_strings(value, visit);
            }
        }
        _ => {}
    }
}

pub async fn open_profile_dm(mut state: Signal<ShellState>, user: String) {
    let existing =
        {
            state.read().channels.iter().position(|channel| {
                channel.is_im && channel.user_id.as_deref() == Some(user.as_str())
            })
        };
    if let Some(index) = existing {
        state.write().select_channel(index);
        state.write().close_profile();
        super::history::refresh_selected_channel(state).await;
        return;
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        state.write().toast = Some("No direct message channel is loaded yet.".into());
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
    match api::open_dm(&transport, &client, &workspace_session, user.clone()).await {
        Ok(channel_id) => {
            let mut shell = state.write();
            if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                workspace
                    .channels
                    .entry(channel_id.clone())
                    .or_insert_with(|| super_platinum_core::slack::models::Channel {
                        id: channel_id.clone(),
                        is_im: true,
                        user: Some(user),
                        ..Default::default()
                    });
            }
            shell.refresh_from_core();
            if let Some(index) = shell
                .channels
                .iter()
                .position(|channel| channel.id == channel_id)
            {
                shell.select_channel(index);
                shell.close_profile();
                drop(shell);
                super::history::refresh_selected_channel(state).await;
            } else {
                shell.toast = Some("Slack opened the DM, but it is not available yet.".into());
            }
        }
        Err(error) => state.write().toast = Some(format!("Could not open message: {error}")),
    }
}

pub async fn toggle_profile_vip(mut state: Signal<ShellState>, user: String) {
    if state.read().profile_vip_loading {
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
    let is_vip = state
        .read()
        .core
        .workspaces
        .get(&team)
        .is_some_and(|workspace| workspace.vip_users.contains(&user));
    state.write().profile_vip_loading = true;
    let result = if is_vip {
        api::remove_priority_user(&transport, &client, &workspace_session, user.clone()).await
    } else {
        api::add_priority_user(&transport, &client, &workspace_session, user.clone()).await
    };
    let mut shell = state.write();
    shell.profile_vip_loading = false;
    match result {
        Ok(()) => {
            if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                if is_vip {
                    workspace.vip_users.remove(&user);
                } else {
                    workspace.vip_users.insert(user);
                }
            }
            shell.toast = Some(if is_vip {
                "Removed from VIPs".into()
            } else {
                "Added to VIPs".into()
            });
            drop(shell);
            persist_workspace(&state, &team);
        }
        Err(error) => shell.toast = Some(format!("Could not update VIP: {error}")),
    }
}

pub async fn open_thread(mut state: Signal<ShellState>, channel: String, root_ts: String) {
    state.write().close_profile();
    {
        let mut shell = state.write();
        shell.thread_root = Some(root_ts.clone());
        // A thread always opens parked on its newest reply, like Slack.
        shell.thread_at_bottom = true;
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
    match api::fetch_replies(
        &transport,
        &client,
        &workspace_session,
        super_platinum_core::slack::api::RepliesArgs {
            channel: channel.clone(),
            ts: root_ts.clone(),
            // Slack's own client asks for the *tail* of a thread
            // (`latest` + `inclusive`), not the first page after the root. A
            // long thread otherwise opens on replies from hours ago and the
            // newest reply — the one the notification is about — is missing.
            latest: Some(format!("{}.999999", super_platinum_core::state::now_secs())),
            inclusive: true,
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
            super::history::mark_thread_read(state, channel.clone(), root_ts.clone()).await;
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

/// Clears one Activity item. Slack's client sends `activity.markRead` with the
/// entry type, the item's `feed_ts`, and its key the moment a row is clicked
/// (CDP-verified), so the row and the bell badge stop counting it.
///
/// The local clear is optimistic: a failed call leaves the item read only until
/// the next feed fetch, which is the same self-healing Slack relies on.
pub async fn mark_activity_read(mut state: Signal<ShellState>, key: String) {
    let target = {
        let mut shell = state.write();
        let Some(target) = shell.core.activity.mark_item_read(&key) else {
            return;
        };
        let unread = shell.core.activity.unread_count();
        if let Some(team) = shell.core.active_team.clone()
            && let Some(workspace) = shell.core.workspaces.get_mut(&team)
        {
            workspace.activity_unread_count = Some(unread);
        }
        target
    };
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
    let (kind, feed_ts) = target;
    if let Err(error) =
        api::mark_activity_read(&transport, &client, &workspace_session, kind, feed_ts, key).await
    {
        eprintln!("super-platinum: activity mark read failed: {error}");
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
                            .filter(|item| item.is_pending())
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

#[cfg(test)]
mod tests {
    use super::profile_emoji_names;

    #[test]
    fn profile_emoji_hydration_includes_status_and_nested_custom_fields() {
        let profile = serde_json::from_value(serde_json::json!({
            "status_emoji": ":ship:",
            "fields": {
                "emoji": {"value": [":sob-pray:", {"choice": ":party-parrot:"}]},
                "link": {"value": "https://example.com", "alt": "site :sparkles:"}
            }
        }))
        .expect("profile fixture");

        let names = profile_emoji_names(&profile);
        assert!(names.contains("ship"));
        assert!(names.contains("sob-pray"));
        assert!(names.contains("party-parrot"));
        assert!(names.contains("sparkles"));
    }
}
