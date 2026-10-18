use crate::media::MediaUri;
use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use super_platinum_core::slack::api;

mod events;
mod hydration;
mod ownership;
pub(crate) mod recovery;
pub(crate) mod source;
pub(crate) use events::prepare as prepare_event;

use crate::directory_member_fetches::Context;
use ownership::{GroupRead, UsersRead};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct GroupPanel {
    pub id: String,
    pub(crate) attempt: Arc<()>,
    pub loading: bool,
    pub error: bool,
    pub channels: bool,
}

pub(crate) async fn open(mut state: Signal<ShellState>, id: String) {
    let attempt = Arc::new(());
    let (team, owner) = {
        let mut shell = state.write();
        let Some(team) = shell.core.active_team.clone() else {
            return;
        };
        let owner = Context::capture(&shell, &team).ok().map(Arc::new);
        shell.close_profile();
        shell.profile_hover = None;
        shell.group_panel = Some(GroupPanel {
            id: id.clone(),
            attempt: attempt.clone(),
            loading: true,
            error: false,
            channels: false,
        });
        (team, owner)
    };
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        if let Some(panel) = state
            .write()
            .group_panel
            .as_mut()
            .filter(|p| ownership::same_panel(p, &id, &attempt))
        {
            panel.loading = false;
        }
        return;
    }
    let Some(owner) = owner else {
        if let Some(panel) = state
            .write()
            .group_panel
            .as_mut()
            .filter(|p| ownership::same_panel(p, &id, &attempt))
        {
            panel.loading = false;
            panel.error = true;
        }
        return;
    };
    let current = |shell: &ShellState| ownership::panel_current(shell, &owner, &id, &attempt);
    let result = async {
        let request = GroupRead::capture(&state.peek(), owner.clone(), std::slice::from_ref(&id))?;
        if !current(&state.peek()) {
            return None;
        }
        let groups = api::fetch_usergroups_info(
            &owner.transport,
            &owner.client,
            &owner.session,
            vec![id.clone()],
        )
        .await
        .ok()?;
        if !current(&state.peek()) || !request.current(&state.peek()) || !request.accepts(&groups) {
            return None;
        }
        let mut group = groups.into_iter().find(|g| g.id == id)?;
        let members =
            api::fetch_usergroup_members(&owner.transport, &owner.client, &owner.session, &group)
                .await
                .ok()?;
        let mut progress = hydration::Progress::default();
        if !progress
            .receive_observed(state, &request, Ok(vec![group.clone()]), None, &current)
            .await
        {
            return None;
        }
        let installed =
            GroupRead::capture(&state.peek(), owner.clone(), std::slice::from_ref(&id))?;
        let membership = {
            let mut shell = state.write();
            if !current(&shell) || !installed.current(&shell) {
                return None;
            }
            source::enqueue(
                &mut shell,
                owner.clone(),
                super_platinum_core::state::directory_usergroups::Operation::Members {
                    id: id.clone(),
                    users: members.clone(),
                    append: false,
                    skip_org_count: false,
                },
            )
        };
        let tracked = match membership {
            source::Admission::Tracked(work) => {
                work.run(state, |shell| current(shell) && installed.current(shell))
                    .await
                    .ok()?;
                true
            }
            source::Admission::Unavailable => false,
        };
        group.users = members.clone();
        group.members_loaded = true;
        let users_request = {
            let mut shell = state.write();
            if !current(&shell) || (!tracked && !installed.current(&shell)) {
                return None;
            }
            if !tracked {
                shell
                    .core
                    .workspaces
                    .get_mut(&team)?
                    .usergroups
                    .insert(id.clone(), group);
            }
            let installed = GroupRead::capture(&shell, owner.clone(), std::slice::from_ref(&id))?;
            UsersRead::capture(&shell, installed, &members)?
        };
        let mut users = Vec::new();
        for chunk in members.chunks(100) {
            if !current(&state.peek()) || !users_request.current(&state.peek()) {
                return None;
            }
            let page = api::fetch_users_info(
                &owner.transport,
                &owner.client,
                &owner.session,
                chunk.to_vec(),
            )
            .await
            .ok()?;
            if !current(&state.peek())
                || !users_request.current(&state.peek())
                || page.iter().any(|user| !chunk.contains(&user.id))
            {
                return None;
            }
            users.extend(page);
        }
        {
            let mut shell = state.write();
            if !current(&shell) || !users_request.current(&shell) || !users_request.accepts(&users)
            {
                return None;
            }
            shell
                .core
                .workspaces
                .get_mut(&team)?
                .users
                .extend(users.into_iter().map(|u| (u.id.clone(), u)));
        }
        if !current(&state.peek()) {
            return None;
        }
        crate::bootstrap::persist_workspace_metadata(&state, &team);
        Some(())
    }
    .await;
    let mut shell = state.write();
    if !ownership::finish(&mut shell, &owner, &id, &attempt, result.is_some()) {
        return;
    }
    shell.refresh_from_core();
}

/// Resolve only groups seen in cached messages or the signed-in user's boot
/// membership list. Edge metadata omits membership, so preserve that separately.
pub(crate) async fn hydrate(mut state: Signal<ShellState>, team: &str) {
    let (owner, ids) = {
        let shell = state.read();
        let Ok(owner) = Context::capture(&shell, team) else {
            return;
        };
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
        let ids = ids
            .into_iter()
            .filter(|id| {
                workspace
                    .usergroups
                    .get(id)
                    .is_none_or(|g| g.label_source.is_none() || g.handle.is_empty())
            })
            .collect::<Vec<_>>();
        (Arc::new(owner), ids)
    };
    if ids.is_empty() {
        return;
    }
    let mut hydration = hydration::Progress::default();
    for chunk in ids.chunks(100) {
        let Some(request) = GroupRead::capture(&state.peek(), owner.clone(), chunk) else {
            return;
        };
        let result = api::fetch_usergroups_info(
            &owner.transport,
            &owner.client,
            &owner.session,
            chunk.to_vec(),
        )
        .await;
        // No write borrow (and thus no render notification) for an empty page.
        if result.as_ref().is_ok_and(Vec::is_empty) {
            if !request.current(&state.peek()) {
                return;
            }
            continue;
        }
        if !hydration
            .receive_observed(state, &request, result, None, |_| true)
            .await
        {
            return;
        }
    }
    if hydration.should_persist() && owner.current(&state.peek()) {
        crate::bootstrap::persist_workspace_metadata(&state, team);
    }
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
                button { "aria-label": "Close user group", onclick: move |_| state.write().group_panel = None, {crate::icons::icon(crate::icons::Icon::Close, "icon")} }
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
                                    if let Some(avatar) = avatar.filter(|a| snapshot.media.is_ready(a)) { img { src: "{avatar.webview_uri_at(snapshot.media_epoch)}", alt: "" } }
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
