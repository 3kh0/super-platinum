use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use super_platinum_core::slack::api;

#[derive(Debug, Clone)]
pub(crate) struct GroupPanel {
    pub id: String,
    pub loading: bool,
    pub error: bool,
    pub channels: bool,
}

pub(crate) async fn open(mut state: Signal<ShellState>, id: String) {
    let Some(team) = state.read().core.active_team.clone() else {
        return;
    };
    {
        let mut shell = state.write();
        shell.profile_user = None;
        shell.profile_hover = None;
        shell.group_panel = Some(GroupPanel {
            id: id.clone(),
            loading: true,
            error: false,
            channels: false,
        });
    }
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        state.write().group_panel.as_mut().unwrap().loading = false;
        return;
    }
    let result = async {
        let (transport, client, sessions) = crate::bootstrap::credentials(&state)?;
        let session = sessions.into_iter().find(|s| s.team_id == team)?;
        let groups = api::fetch_usergroups_info(&transport, &client, &session, vec![id.clone()])
            .await
            .ok()?;
        let mut group = groups.into_iter().find(|g| g.id == id)?;
        let members = api::fetch_usergroup_members(&transport, &client, &session, &group)
            .await
            .ok()?;
        group.users = members.clone();
        group.members_loaded = true;
        if let Some(workspace) = state.write().core.workspaces.get_mut(&team) {
            workspace.usergroups.insert(id.clone(), group);
        }
        let mut users = Vec::new();
        for chunk in members.chunks(100) {
            users.extend(
                api::fetch_users_info(&transport, &client, &session, chunk.to_vec())
                    .await
                    .ok()?,
            );
        }
        if let Some(workspace) = state.write().core.workspaces.get_mut(&team) {
            workspace
                .users
                .extend(users.into_iter().map(|u| (u.id.clone(), u)));
        }
        crate::bootstrap::persist_workspace(&state, &team);
        Some(())
    }
    .await;
    let mut shell = state.write();
    if shell.core.active_team.as_ref() != Some(&team) {
        return;
    }
    if let Some(panel) = shell.group_panel.as_mut().filter(|p| p.id == id) {
        panel.loading = false;
        panel.error = result.is_none();
    }
    shell.refresh_from_core();
}

/// Resolve only groups seen in cached messages or the signed-in user's boot
/// membership list. Edge metadata omits membership, so preserve that separately.
pub(crate) async fn hydrate(mut state: Signal<ShellState>, team: &str) {
    let ids = {
        let shell = state.read();
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return;
        };
        let mut ids = std::collections::HashSet::new();
        for list in workspace.messages.values() {
            for message in &list.messages {
                super_platinum_core::slack::models::collect_group_ids(message, &mut ids);
            }
        }
        for ((thread_team, _, _), list) in &shell.core.threads {
            if thread_team == team {
                for message in &list.messages {
                    super_platinum_core::slack::models::collect_group_ids(message, &mut ids);
                }
            }
        }
        for message in shell.core.activity.hydrated.values() {
            super_platinum_core::slack::models::collect_group_ids(message, &mut ids);
        }
        ids.extend(workspace.usergroups.keys().cloned());
        ids.into_iter()
            .filter(|id| {
                workspace
                    .usergroups
                    .get(id)
                    .is_none_or(|g| g.handle.is_empty())
            })
            .collect::<Vec<_>>()
    };
    if ids.is_empty() {
        return;
    }
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|s| s.team_id == team) else {
        return;
    };
    for chunk in ids.chunks(100) {
        let Ok(groups) =
            api::fetch_usergroups_info(&transport, &client, &session, chunk.to_vec()).await
        else {
            return;
        };
        let mut shell = state.write();
        let Some(workspace) = shell.core.workspaces.get_mut(team) else {
            return;
        };
        for mut group in groups {
            if let Some(existing) = workspace.usergroups.get(&group.id) {
                group.users = existing.users.clone();
                group.members_loaded = existing.members_loaded;
            }
            workspace.usergroups.insert(group.id.clone(), group);
        }
        shell.refresh_from_core();
    }
    crate::bootstrap::persist_workspace(&state, team);
}

pub(crate) fn pane(mut state: Signal<ShellState>, snapshot: &ShellState) -> Element {
    let Some(panel) = snapshot.group_panel.as_ref() else {
        return rsx! {};
    };
    let Some(workspace) = snapshot
        .core
        .active_team
        .as_ref()
        .and_then(|t| snapshot.core.workspaces.get(t))
    else {
        return rsx! {};
    };
    let group = workspace.usergroups.get(&panel.id);
    let mut members = group
        .filter(|g| g.members_loaded)
        .map(|g| g.users.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    members.sort_by_cached_key(|id| workspace.display_name(id).to_lowercase());
    let retry = panel.id.clone();
    let member_count = group
        .filter(|g| g.members_loaded)
        .map(|g| g.users.len().to_string())
        .unwrap_or_else(|| "…".into());
    rsx! {
        aside { class: "profile-pane usergroup-pane", role: "dialog", "aria-label": "User group",
            header { class: "profile-pane-header",
                h2 { "User group" }
                button { "aria-label": "Close user group", onclick: move |_| state.write().group_panel = None, "×" }
            }
            if panel.loading { p { class: "group-notice", role: "status", "Loading group…" } }
            if panel.error {
                div { class: "group-notice", role: "status", "Could not refresh this group."
                    button { onclick: move |_| { spawn(open(state, retry.clone())); }, "Retry" }
                }
            }
            if let Some(group) = group {
                div { class: "group-summary",
                    h2 { "{group.name}" }
                    p { "{group.description}" }
                    p { class: "muted", "@{group.handle}" }
                    if group.date_delete != 0 { p { "This group is deactivated." } }
                    else if group.includes(&workspace.self_user_id) { p { class: "muted", "You’re a member" } }
                }
                div { class: "group-tabs", role: "tablist",
                    button { role: "tab", "aria-selected": (!panel.channels).to_string(), onclick: move |_| { if let Some(p) = state.write().group_panel.as_mut() { p.channels = false; } }, "Members {member_count}" }
                    button { role: "tab", "aria-selected": panel.channels.to_string(), onclick: move |_| { if let Some(p) = state.write().group_panel.as_mut() { p.channels = true; } }, "Channels {group.prefs.channels.len() + group.prefs.groups.len()}" }
                }
                div { class: "group-list", role: "tabpanel",
                    if panel.channels {
                        if group.prefs.channels.is_empty() && group.prefs.groups.is_empty() { p { class: "group-notice", "No default channels." } }
                        for channel in group.prefs.channels.iter().chain(&group.prefs.groups) {
                            {let id = channel.clone(); let label = workspace.channels.get(channel).and_then(|c| c.name.clone()).unwrap_or_else(|| "Private or unavailable channel".into());
                            rsx! { button { class: "group-row", onclick: move |_| {
                                let index = state.read().channels.iter().position(|c| c.id == id);
                                if let Some(index) = index {
                                    state.write().select_channel(index, ChannelOpen::Global);
                                    spawn(crate::bootstrap::refresh_selected_channel(state));
                                }
                            }, "#{label}" } }}
                        }
                    } else {
                        if group.members_loaded && group.users.is_empty() { p { class: "group-notice", "No members." } }
                        for user in members {
                            {let id = user.clone(); let name = workspace.display_name(user); let profile = workspace.users.get(user).and_then(|u| u.profile.as_ref());
                            let title = profile.and_then(|p| p.title.as_deref()).unwrap_or_default();
                            let avatar = workspace.users.get(user).and_then(super_platinum_core::state::user_avatar_url).map(|url| snapshot.media.register_avatar(user, &url));
                            rsx! { button { class: "group-row", onclick: move |_| { spawn(crate::bootstrap::open_profile(state, id.clone())); },
                                span { class: "group-avatar",
                                    if let Some(avatar) = avatar.filter(|a| snapshot.media.is_ready(a)) { img { src: "{avatar.uri_at(snapshot.media_epoch)}", alt: "" } }
                                    else { "{name.chars().next().unwrap_or('?')}" }
                                }
                                span { class: "group-identity", strong { "{name}" } span { "{title}" } }
                            } }}
                        }
                    }
                }
            } else if !panel.loading && !panel.error { p { class: "group-notice", "This group is unavailable." } }
        }
    }
}
