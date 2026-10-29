use super::*;
use std::future::Future;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport, api, models::User},
};
pub(in crate::bootstrap::session) async fn hydrate(
    state: &mut Signal<ShellState>,
    transport: &Transport,
    client: &SlackClient,
    session: &WorkspaceSession,
    team: &str,
    owner: Arc<Owner>,
) {
    if !owner.current(&state.peek().core) {
        return;
    }
    crate::usergroups::hydrate(*state, team).await;
    if !owner.current(&state.peek().core) {
        return;
    }
    hydrate_with(*state, owner, team, |ids| {
        api::fetch_users_info(transport, client, session, ids)
    })
    .await;
}
pub(super) async fn hydrate_with<F, Fut>(
    mut state: Signal<ShellState>,
    owner: Arc<Owner>,
    team: &str,
    mut lookup: F,
) where
    F: FnMut(Vec<String>) -> Fut,
    Fut: Future<Output = Result<Vec<User>, Error>>,
{
    if !owner.current(&state.peek().core) {
        return;
    }
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
        super::super::visit_surface_messages(&shell, team, &mut |message| {
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
            if id.trim().is_empty() {
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

    let Some(lease) = Lease::reserve(&mut state, owner, team, Kind::User, requested) else {
        return;
    };
    let mut loaded = Vec::new();
    let mut failed = false;
    for chunk in lease.ids.chunks(100) {
        if !lease.current(&state.peek().core) {
            return;
        }
        let result = lookup(chunk.to_vec()).await;
        if !lease.current(&state.peek().core) {
            return;
        }
        match result {
            Ok(users) => loaded.extend(users),
            Err(_) => {
                failed = true;
                eprintln!("super-platinum: visible user hydration failed");
            }
        }
    }
    let mut shell = state.write();
    if !lease.current(&shell.core) {
        return;
    }
    // Preserve existing conservative whole-batch retry and successful-negative
    // caching. Flat core flags record accepted facts, never pending authority.
    if !failed {
        lease.complete(&shell.core, &lease.ids);
        shell
            .core
            .avatar_profile_hydrated
            .extend(lease.ids.iter().cloned());
    }
    if let Some(workspace) = shell.core.workspaces.get_mut(team) {
        for user in loaded {
            workspace.users.insert(user.id.clone(), user);
        }
    }
    shell.refresh_from_core();
}
