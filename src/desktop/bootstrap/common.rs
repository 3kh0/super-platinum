use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::api::{self, HistoryArgs};
use super_platinum_core::slack::models::ChannelId;

use crate::state::ShellState;

pub(crate) fn credentials(
    state: &Signal<ShellState>,
) -> Option<(
    std::sync::Arc<super_platinum_core::slack::Transport>,
    super_platinum_core::slack::SlackClient,
    Vec<super_platinum_core::config::WorkspaceSession>,
)> {
    let state = state.read();
    Some((
        state.core.transport.clone()?,
        state.core.client.clone(),
        state
            .core
            .session
            .as_ref()?
            .workspaces
            .values()
            .cloned()
            .collect(),
    ))
}

pub(crate) async fn refresh_history(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    team: &str,
    channel: ChannelId,
) {
    refresh_history_at(
        state,
        transport,
        client,
        workspace_session,
        team,
        channel,
        None,
    )
    .await;
}

pub(super) async fn refresh_history_at(
    state: &mut Signal<ShellState>,
    transport: &super_platinum_core::slack::Transport,
    client: &super_platinum_core::slack::SlackClient,
    workspace_session: &super_platinum_core::config::WorkspaceSession,
    team: &str,
    channel: ChannelId,
    latest: Option<String>,
) {
    match api::fetch_history(
        transport,
        client,
        workspace_session,
        HistoryArgs {
            channel: channel.clone(),
            inclusive: latest.is_some(),
            latest,
            limit: Some(100),
            ..Default::default()
        },
    )
    .await
    {
        Ok(page) => {
            let mut shell = state.write();
            if let Some(workspace) = shell.core.workspaces.get_mut(team) {
                let messages = workspace.messages.entry(channel).or_default();
                for message in page.messages {
                    let message = super_platinum_core::state::visible_message(message);
                    if super_platinum_core::state::is_channel_timeline_visible(&message) {
                        messages.upsert(message);
                    }
                }
                messages.loaded = true;
                messages.history_failed = false;
                messages.history_refreshing = false;
                messages.has_more_older = page.has_more;
            }
            shell.refresh_from_core();
        }
        Err(error) => {
            let mut shell = state.write();
            if let Some(workspace) = shell.core.workspaces.get_mut(team)
                && let Some(messages) = workspace.messages.get_mut(&channel)
            {
                messages.history_failed = true;
                messages.history_refreshing = false;
            }
            // Silent while offline: the rail already says so, and the cached
            // transcript the reader is looking at is still the right one.
            shell.report_failure(&error, "", || format!("History refresh failed: {error}"));
        }
    }
}

pub(crate) fn persist_workspace(state: &Signal<ShellState>, team: &str) {
    let state = state.read();
    if state.core.cache.is_none() {
        return;
    }
    let (Some(account), Some(workspace)) = (
        state.core.active_account.clone(),
        state.core.workspaces.get(team).cloned(),
    ) else {
        return;
    };
    drop(state);
    let _ = persistence_worker().try_send((account, workspace));
}

type PersistenceJob = (String, super_platinum_core::state::Workspace);

fn persistence_worker() -> &'static std::sync::mpsc::SyncSender<PersistenceJob> {
    static WORKER: std::sync::OnceLock<std::sync::mpsc::SyncSender<PersistenceJob>> =
        std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        // Keep at most one follow-up snapshot while a save is active. Realtime
        // bursts then coalesce instead of building an unbounded disk queue.
        let (sender, receiver) = std::sync::mpsc::sync_channel::<PersistenceJob>(1);
        std::thread::Builder::new()
            .name("super-platinum-cache".into())
            .spawn(move || {
                while let Ok((account, workspace)) = receiver.recv() {
                    let result = super_platinum_core::cache::Cache::open_default(&account, false)
                        .and_then(|cache| cache.save_workspace(&workspace));
                    if let Err(error) = result {
                        eprintln!(
                            "super-platinum: cache save failed for {}: {error}",
                            workspace.team_id
                        );
                    }
                }
            })
            .expect("cache persistence worker starts");
        sender
    })
}
