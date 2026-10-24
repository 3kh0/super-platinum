use dioxus::prelude::{ReadableExt, Signal, WritableExt, spawn};
use super_platinum_core::slack::api;

use super::common::{credentials, persist_workspace};
use super::session::refresh_media;
use crate::state::MainView;
use crate::state::ShellState;
mod activity_read_guard;

pub async fn open_profile(mut state: Signal<ShellState>, user: String) {
    show_profile(&mut state.write(), user.clone());
    refresh_open_profile(state, user).await;
}

/// Establish the pane synchronously so a link can commit its DM and profile
/// together before either independent hydration request yields.
pub(crate) fn show_profile(shell: &mut ShellState, user: String) {
    shell.close_profile();
    shell.profile_user = Some(user.clone());
    shell.group_panel = None;
    shell.profile_hover = None;
    shell.profile_menu_open = false;
    shell.close_thread();
    shell.core.profile_pane = Some(super_platinum_core::domain::ProfilePaneState {
        user,
        loading: true,
        error: None,
    });
}

pub(crate) async fn refresh_open_profile(mut state: Signal<ShellState>, user: String) {
    if state.peek().profile_user.as_ref() != Some(&user) {
        return;
    }
    let Some(visit) = super::profile_navigation::ProfileVisit::capture(&state.read()) else {
        return;
    };
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        if let Some(pane) = state.write().core.profile_pane.as_mut() {
            pane.loading = false;
        }
        return;
    }
    let Some((transport, client, workspaces)) = credentials(&state) else {
        visit.unavailable(&mut state.write());
        return;
    };
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        visit.unavailable(&mut state.write());
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
    // `users.profile.get` only resolves members of the current workspace. Slack
    // Connect members are already represented in the workspace user map, but
    // must be refreshed through the org-aware edge endpoint instead.
    let use_users_info = state
        .read()
        .core
        .workspaces
        .get(&team)
        .and_then(|workspace| workspace.users.get(&user))
        .is_some_and(profile_uses_users_info);
    let needs_fields = !use_users_info && !state.read().core.profile_fields.contains_key(&team);
    let (profile, extras, fields) = tokio::join!(
        async {
            if use_users_info {
                api::fetch_users_info(&transport, &client, &workspace_session, vec![user.clone()])
                    .await?
                    .into_iter()
                    .find(|member| member.id == user)
                    .and_then(|member| member.profile)
                    .ok_or_else(|| super_platinum_core::slack::Error::Api("user_not_found".into()))
            } else {
                api::fetch_user_profile(&transport, &client, &workspace_session, user.clone()).await
            }
        },
        async {
            if use_users_info {
                Ok(None)
            } else {
                api::fetch_user_profile_extras(
                    &transport,
                    &client,
                    &workspace_session,
                    user.clone(),
                )
                .await
                .map(Some)
            }
        },
        async {
            if needs_fields {
                api::fetch_team_profile_fields(&transport, &client, &workspace_session).await
            } else {
                Ok(Vec::new())
            }
        },
    );
    if !visit.current(&state.read()) {
        visit.retire(&mut state.write());
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
        .ok()
        .and_then(Option::as_ref)
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
    let channel_revision = state
        .read()
        .core
        .workspaces
        .get(&team)
        .map(|w| w.channel_lifecycle.revision())
        .unwrap_or(0);
    let posting =
        crate::bootstrap::posting_ingress::capture(&state, &transport, &workspace_session)
            .for_channels(&missing_dm_ids);
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
    if !visit.current(&state.read()) {
        visit.retire(&mut state.write());
        return;
    }
    posting.channels(&state, &recent_channels);
    let mut shell = state.write();
    let profile_error = profile
        .as_ref()
        .err()
        .map(|_| "Try opening this profile again.".to_owned());
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
        if let Ok(Some(extras)) = extras {
            member.im_mpim_ids = extras.im_mpim_ids;
            member.has_more_mpims = extras.has_more_mpims;
        }
        workspace.apply_channel_refresh_since(
            super_platinum_core::slack::models::GenericInfoPage {
                channels: recent_channels,
                ..Default::default()
            },
            channel_revision,
        );
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

fn profile_uses_users_info(user: &super_platinum_core::slack::models::User) -> bool {
    user.is_stranger
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

/// Clears one Activity item. Slack's client sends `activity.markRead` with the
/// entry type, the item's `feed_ts`, and its key the moment a row is clicked
/// (CDP-verified). Inbox rows clear immediately; their canonical category badge
/// waits for Slack's count update, matching QDAL.m in the official client.
///
/// The local clear is optimistic: a failed call leaves the item read only until
/// the next feed fetch, which is the same self-healing Slack relies on.
pub async fn mark_activity_read(mut state: Signal<ShellState>, key: String) {
    let (target, reservation) = {
        let mut shell = state.write();
        let Some(reservation) = crate::read_shortcuts::bulk::begin_individual(&mut shell) else {
            return;
        };
        let Some(target) = crate::realtime::activity_counts::mark_item_read(&mut shell.core, &key)
        else {
            crate::read_shortcuts::bulk::end_individual(&mut shell, reservation);
            return;
        };
        (target, reservation)
    };
    let _reservation = activity_read_guard::Reservation {
        state,
        id: reservation,
    };
    // Keep an individual mark admitted before Clear/Undo visible until its
    // request completes. The inverse admission guard blocks a new optimistic
    // Activity clear while the workspace-wide mutation/readback is pending.
    async {
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
            api::mark_activity_read(&transport, &client, &workspace_session, kind, feed_ts, key)
                .await
        {
            eprintln!("super-platinum: activity mark read failed: {error}");
        }
    }
    .await;
}

pub async fn load_more_dms(mut state: Signal<ShellState>) {
    let Some((transport, client, workspaces)) = credentials(&state) else {
        return;
    };
    let Some((team, cursor, load_seq)) = ({
        let mut shell = state.write();
        if shell.main_view != MainView::Dms || shell.core.dms.loading {
            None
        } else if let (Some(team), Some(cursor)) = (
            shell.core.active_team.clone(),
            shell.core.dms.next_cursor.take(),
        ) {
            shell.core.dms.loading = true;
            shell.core.dms.load_seq = shell.core.dms.load_seq.wrapping_add(1);
            Some((team, cursor, shell.core.dms.load_seq))
        } else {
            None
        }
    }) else {
        return;
    };
    let Some(workspace_session) = workspaces
        .into_iter()
        .find(|workspace| workspace.team_id == team)
    else {
        let mut shell = state.write();
        if shell.core.dms.load_seq == load_seq {
            shell.core.dms.loading = false;
            shell.core.dms.next_cursor = Some(cursor);
        }
        return;
    };

    match api::fetch_client_dms(
        &transport,
        &client,
        &workspace_session,
        100,
        Some(cursor.clone()),
    )
    .await
    {
        Ok(page) => {
            let mut shell = state.write();
            if shell.core.dms.load_seq != load_seq {
                return;
            }
            for entry in page.dms {
                shell.core.dms.upsert(entry);
            }
            shell.core.dms.next_cursor = page
                .response_metadata
                .and_then(|metadata| metadata.next_cursor)
                .filter(|cursor| !cursor.is_empty());
            shell.core.dms.loading = false;
        }
        Err(error) => {
            let mut shell = state.write();
            if shell.core.dms.load_seq != load_seq {
                return;
            }
            shell.core.dms.loading = false;
            shell.core.dms.next_cursor = Some(cursor);
            shell.report_failure(&error, "", || {
                format!("More direct messages failed: {error}")
            });
        }
    }
}

pub async fn load_main_view(mut state: Signal<ShellState>, target: MainView) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::Surface(target),
    ) {
        return;
    }
    // Each rail tab keeps its own conversation, the way Slack's does: leaving
    // Activity for a channel and coming back shows the item that was being
    // read, not the channel it was left for.
    let restore = {
        let mut shell = state.write();
        shell.remember_surface();
        shell.main_view = target;
        shell.profile_hover = None;
        if matches!(
            target,
            MainView::Later | MainView::Files | MainView::Scheduled
        ) {
            shell.close_profile();
        }
        shell.close_thread();
        let restore = shell.surfaces.get(&target).cloned();
        if let Some(surface) = restore.as_ref()
            && let Some(index) = shell
                .channels
                .iter()
                .position(|channel| channel.id == surface.channel)
        {
            shell.select_channel(index, crate::state::ChannelOpen::InSurface);
        }
        restore
    };
    if let Some(surface) = restore {
        let signal = state;
        spawn(super::history::refresh_selected_channel(signal));
        if let Some(root) = surface.thread_root {
            spawn(super::open_thread(signal, surface.channel, root));
        }
    }
    refresh_main_view(state, target, None).await;
}

/// Refresh a surface's data without performing rail navigation.
pub(super) async fn refresh_main_view(
    mut state: Signal<ShellState>,
    target: MainView,
    reconnect: Option<&super::reconnect::Visit>,
) {
    if reconnect.is_some_and(|visit| !visit.current(&state.peek())) {
        return;
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
        MainView::Scheduled => crate::scheduled::load(state, false).await,
        MainView::Files => {
            let (query, page) = crate::files::visible_state(&state.peek())
                .map(|f| (f.query.clone(), f.page))
                .unwrap_or_default();
            crate::files::load(state, query, page).await;
        }
        MainView::Later => {
            let filter = crate::later::visible_state(&state.peek())
                .map(|later| later.filter)
                .unwrap_or_default();
            crate::later::load(state, filter, false).await;
        }
        MainView::Unreads => {}
        MainView::Dms => {
            let load_seq = {
                let mut shell = state.write();
                if reconnect.is_none() {
                    shell.core.dms.loading = true;
                }
                shell.core.dms.load_seq = shell.core.dms.load_seq.wrapping_add(1);
                shell.core.dms.load_seq
            };
            match api::fetch_client_dms(&transport, &client, &workspace_session, 100, None).await {
                Ok(page) => {
                    let mut shell = state.write();
                    if shell.core.dms.load_seq != load_seq
                        || reconnect.is_some_and(|visit| !visit.current(&shell))
                    {
                        return;
                    }
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
                    if shell.core.dms.load_seq != load_seq
                        || reconnect.is_some_and(|visit| !visit.current(&shell))
                    {
                        return;
                    }
                    shell.core.dms.loading = false;
                    shell.report_failure(&error, "", || format!("Direct messages failed: {error}"));
                }
            }
        }
        MainView::Threads => crate::thread_feed::load(state, false).await,
        MainView::Activity => return crate::activity_feed::load(state, false).await,
    }
    if reconnect.is_some_and(|visit| !visit.current(&state.peek())) {
        return;
    }
    super::session::hydrate_current_surface(state).await;
}

#[cfg(test)]
mod tests {
    use super::{profile_emoji_names, profile_uses_users_info};

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

    #[test]
    fn slack_connect_profiles_use_the_org_aware_user_endpoint() {
        let external = serde_json::from_value(serde_json::json!({
            "id": "U_EXTERNAL",
            "is_stranger": true,
            "profile": {"display_name": "External teammate"}
        }))
        .expect("external user fixture");
        let local = serde_json::from_value(serde_json::json!({
            "id": "U_LOCAL",
            "profile": {"display_name": "Local teammate"}
        }))
        .expect("local user fixture");

        assert!(profile_uses_users_info(&external));
        assert!(!profile_uses_users_info(&local));
    }
}
