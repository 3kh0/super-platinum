//! Native async orchestration for Slack's Later surface.
pub(crate) mod file_actions;
mod file_check;
pub(crate) mod file_fixture;
mod files;
mod navigation;
use crate::state::{MainView, ShellState};
use dioxus::prelude::*;
pub use navigation::open;
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{
    config::WorkspaceSession,
    later::{LaterFilter, SavedAction, SavedKey},
    slack::{SlackClient, Transport, api},
};

struct Context {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    epoch: u64,
    account: Option<String>,
}
impl Context {
    fn get(state: &Signal<ShellState>) -> Option<Self> {
        let (transport, client, sessions) = crate::bootstrap::credentials(state)?;
        let shell = state.peek();
        let team = shell.core.active_team.as_ref()?;
        Some(Self {
            transport,
            client,
            session: sessions.into_iter().find(|s| &s.team_id == team)?,
            epoch: shell.core.account_epoch,
            account: shell.core.active_account.clone(),
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account == self.account
            && shell.core.account_epoch == self.epoch
            && shell
                .core
                .later
                .matches_scope(self.epoch, &self.session.team_id)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
    }
}

pub fn visible_state(shell: &ShellState) -> Option<&super_platinum_core::later::LaterState> {
    shell
        .core
        .later
        .matches_scope(shell.core.account_epoch, shell.core.active_team.as_deref()?)
        .then_some(&shell.core.later)
}

pub async fn load(mut state: Signal<ShellState>, filter: LaterFilter, append: bool) {
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    let (generation, cursor, dock_revision) = {
        let mut shell = state.write();
        shell
            .core
            .later
            .ensure_scope(ctx.epoch, &ctx.session.team_id);
        let dock_revision = shell
            .core
            .workspaces
            .get(&ctx.session.team_id)
            .map(|w| w.dock_counts.revision());
        let later = &mut shell.core.later;
        if append && (later.loading || later.next_cursor.is_none()) {
            return;
        }
        let cursor = if append {
            later.next_cursor.clone()
        } else {
            None
        };
        (later.begin(filter, append), cursor, dock_revision)
    };
    let result = api::fetch_saved(
        &ctx.transport,
        &ctx.client,
        &ctx.session,
        filter,
        cursor.clone(),
    )
    .await;
    if !ctx.current(&state.peek()) {
        return;
    }
    let applied = {
        let mut shell = state.write();
        if shell.core.later.generation != generation {
            return;
        }
        match result {
            Ok(page) => {
                let overdue = page.counts.uncompleted_overdue_count;
                let applied =
                    shell
                        .core
                        .later
                        .apply_page(generation, page, append, cursor.as_deref());
                if applied {
                    if let (Some(count), Some(workspace)) =
                        (overdue, shell.core.workspaces.get_mut(&ctx.session.team_id))
                    {
                        // A saved RT event or fresher client.counts snapshot wins
                        // over this page, even when its list generation matches.
                        if Some(workspace.dock_counts.revision()) == dock_revision {
                            workspace.dock_counts.apply_overdue_count(count);
                        }
                    }
                    shell.refresh_dock_badge_for(&ctx.session.team_id);
                }
                applied
            }
            Err(error) => {
                shell.core.later.loading = false;
                shell.core.later.error = Some(format!("Could not load Later: {error}"));
                false
            }
        }
    };
    if applied && state.peek().main_view == MainView::Later {
        hydrate(state, &ctx, generation).await;
    }
}

async fn hydrate(mut state: Signal<ShellState>, ctx: &Context, generation: u64) {
    let groups = {
        let shell = state.peek();
        let later = &shell.core.later;
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for key in &later.visible {
            if key.item_type == "message" && !later.hydrated.contains_key(key) {
                groups
                    .entry(key.item_id.clone())
                    .or_default()
                    .push(key.ts.clone());
            }
        }
        groups.into_iter().collect::<Vec<_>>()
    };
    for chunk in groups.chunks(20) {
        let hydration_revision = {
            let mut shell = state.write();
            if !ctx.current(&shell) || shell.core.later.generation != generation {
                return;
            }
            shell.core.later.begin_hydration()
        };
        let result =
            api::fetch_messages_list(&ctx.transport, &ctx.client, &ctx.session, chunk.to_vec())
                .await;
        if !ctx.current(&state.peek()) || state.peek().core.later.generation != generation {
            return;
        }
        match result {
            Ok(page) => {
                let mut shell = state.write();
                for (channel, data) in page.messages_data {
                    for message in data.messages {
                        if let Some(ts) = message.ts.clone() {
                            shell.core.hydrate_later_message(
                                &ctx.session.team_id,
                                SavedKey::message(channel.clone(), ts),
                                message,
                                hydration_revision,
                            );
                        }
                    }
                }
            }
            Err(error) => {
                state.write().core.later.error = Some(format!(
                    "Saved items loaded; message previews failed: {error}"
                ));
            }
        }
    }
    files::hydrate(state, Some(ctx), generation).await;
    if !ctx.current(&state.peek()) || state.peek().core.later.generation != generation {
        return;
    }
    // Hydrate names for saved messages from conversations not visited this run.
    let (users, channels) = {
        let shell = state.peek();
        let Some(workspace) = shell.core.workspaces.get(&ctx.session.team_id) else {
            return;
        };
        let mut users = std::collections::HashSet::new();
        let mut channels = std::collections::HashSet::new();
        for key in &shell.core.later.visible {
            if key.item_type != "message" {
                continue;
            }
            if !workspace.channels.contains_key(&key.item_id) {
                channels.insert(key.item_id.clone());
            }
            if let Some(user) = shell
                .core
                .later
                .hydrated
                .get(key)
                .and_then(|m| m.user.as_ref())
            {
                if !workspace.users.contains_key(user) {
                    users.insert(user.clone());
                }
            }
        }
        for file in shell.core.later.files.values() {
            if let Some(user) = super_platinum_core::state::file_uploader_id(file) {
                if !workspace.users.contains_key(user) {
                    users.insert(user.to_owned());
                }
            }
        }
        (
            users.into_iter().collect::<Vec<_>>(),
            channels.into_iter().collect::<Vec<_>>(),
        )
    };
    if !users.is_empty() {
        if let Ok(users) =
            api::fetch_users_info(&ctx.transport, &ctx.client, &ctx.session, users).await
        {
            if !ctx.current(&state.peek()) {
                return;
            }
            if let Some(workspace) = state.write().core.workspaces.get_mut(&ctx.session.team_id) {
                for user in users {
                    workspace.apply_user_update(user);
                }
            }
        }
    }
    if !channels.is_empty() {
        if let Ok(channels) =
            api::fetch_channels_info(&ctx.transport, &ctx.client, &ctx.session, channels).await
        {
            if !ctx.current(&state.peek()) {
                return;
            }
            if let Some(workspace) = state.write().core.workspaces.get_mut(&ctx.session.team_id) {
                for channel in channels {
                    workspace.insert_channel_metadata(channel);
                }
            }
        }
    }
}

pub async fn hydrate_visible(state: Signal<ShellState>) {
    if state.peek().main_view != MainView::Later {
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    if !ctx.current(&state.peek()) {
        return;
    }
    let generation = state.peek().core.later.generation;
    hydrate(state, &ctx, generation).await;
}

pub fn change(state: Signal<ShellState>, key: SavedKey, action: SavedAction) {
    if key.item_type == "file" {
        file_actions::change_from_later(state, key, action);
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    dioxus::core::spawn_forever(change_async(state, ctx, key, action));
}

pub fn save_reminder(
    mut state: Signal<ShellState>,
    item: Option<SavedKey>,
    description: Vec<serde_json::Value>,
    due: i64,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let Some(ctx) = Context::get(&state) else {
        done(Err("Connect to a workspace to create a reminder.".into()));
        return;
    };
    if description.is_empty()
        || (item.is_none() && due <= super_platinum_core::state::now_secs())
        || item.as_ref().is_some_and(|key| key.item_type != "reminder")
    {
        done(Err(
            "Add a description and choose a time in the future.".into()
        ));
        return;
    }
    state
        .write()
        .core
        .later
        .ensure_scope(ctx.epoch, &ctx.session.team_id);
    dioxus::core::spawn_forever(async move {
        let result = api::save_saved_reminder(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            item.as_ref(),
            &description,
            due,
        )
        .await;
        if !ctx.current(&state.peek()) {
            done(Err(
                "The active account changed. Refresh Later to check the reminder.".into(),
            ));
            return;
        }
        match result {
            Ok(item) => {
                state
                    .write()
                    .core
                    .later
                    .updated(&item.key(), Some(item.clone()));
                done(Ok(()));
                if state.peek().core.active_team.as_ref() == Some(&ctx.session.team_id) {
                    let filter = state.peek().core.later.filter;
                    load(state, filter, false).await;
                }
            }
            Err(error) => done(Err(if error.is_offline() {
                "Reconnect to create this reminder. Your draft is still here.".into()
            } else {
                format!("Could not create reminder: {error}")
            })),
        }
    });
}

async fn change_async(
    mut state: Signal<ShellState>,
    ctx: Context,
    key: SavedKey,
    action: SavedAction,
) {
    {
        let mut shell = state.write();
        if shell.core.account_epoch != ctx.epoch
            || shell.core.active_team.as_ref() != Some(&ctx.session.team_id)
            || shell
                .core
                .transport
                .as_ref()
                .is_none_or(|transport| !Arc::ptr_eq(transport, &ctx.transport))
        {
            return;
        }
        shell
            .core
            .later
            .ensure_scope(ctx.epoch, &ctx.session.team_id);
        if !shell.core.later.busy.insert(key.clone()) {
            return;
        }
        shell.core.later.error = None;
    }
    let result = api::change_saved(&ctx.transport, &ctx.client, &ctx.session, &key, action).await;
    if !ctx.current(&state.peek()) {
        return;
    }
    let success = {
        let mut shell = state.write();
        shell.core.later.busy.remove(&key);
        match result {
            Ok(item) => {
                shell.core.later.updated(&key, item);
                true
            }
            Err(error) => {
                shell.report_failure(&error, "Later needs a connection", || {
                    format!("Could not update Later: {error}")
                });
                false
            }
        }
    };
    if success && state.peek().core.active_team.as_ref() == Some(&ctx.session.team_id) {
        let filter = state.peek().core.later.filter;
        load(state, filter, false).await;
    }
}
