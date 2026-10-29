use super::*;
use std::future::Future;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, SlackClient, Transport, api, models::Emoji},
};
pub(in crate::bootstrap::session) async fn hydrate(
    state: &mut Signal<ShellState>,
    transport: &Transport,
    client: &SlackClient,
    session: &WorkspaceSession,
    team: &str,
    owner: Arc<Owner>,
) {
    hydrate_with(*state, owner, team, |ids| {
        api::fetch_emojis_info(transport, client, session, ids)
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
    Fut: Future<Output = Result<Vec<Emoji>, Error>>,
{
    if !owner.current(&state.peek().core) {
        return;
    }
    let names = {
        let shell = state.read();
        let Some(workspace) = shell.core.workspaces.get(team) else {
            return;
        };
        let mut names = std::collections::HashSet::new();
        super::super::visit_surface_messages(&shell, team, &mut |message| {
            super_platinum_core::state::collect_message_emoji_names(message, &mut names);
        });
        if let Some(status) = workspace
            .users
            .get(&workspace.self_user_id)
            .and_then(|user| user.profile.as_ref())
            .and_then(|profile| profile.status_emoji.as_deref())
        {
            names.extend(
                super_platinum_core::state::emoji_names_in_text(status)
                    .into_iter()
                    .filter(|name| !super_platinum_core::state::is_standard_emoji(name)),
            );
        }
        names.retain(|name| !workspace.custom_emoji.contains_key(name));
        names.into_iter().collect::<Vec<_>>()
    };

    let Some(lease) = Lease::reserve(&mut state, owner, team, Kind::Emoji, names) else {
        return;
    };
    let mut loaded = Vec::new();
    let mut completed = Vec::new();
    for chunk in lease.ids.chunks(100) {
        if !lease.current(&state.peek().core) {
            return;
        }
        let result = lookup(chunk.to_vec()).await;
        if !lease.current(&state.peek().core) {
            return;
        }
        match result {
            Ok(emojis) => {
                completed.extend_from_slice(chunk);
                loaded.extend(emojis);
            }
            Err(_) => eprintln!("super-platinum: custom emoji hydration failed"),
        }
    }
    let mut shell = state.write();
    if !lease.current(&shell.core) {
        return;
    }
    lease.complete(&shell.core, &completed);
    shell
        .core
        .emoji_hydrated
        .extend(completed.into_iter().map(|id| (team.to_owned(), id)));
    if loaded.is_empty() {
        return;
    }
    if let Some(workspace) = shell.core.workspaces.get_mut(team) {
        workspace.apply_emojis(loaded);
    }
    shell.refresh_from_core();
}
