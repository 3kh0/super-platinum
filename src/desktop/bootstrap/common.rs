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
    let generation = state.read().channel_generation;
    let file_read = state.write().core.begin_file_display_read(team);
    let result = api::fetch_history(
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
    .await;
    // Account reloads replace the transport; a late response belongs to the
    // old account even when the new account has the same team/channel ids.
    if state
        .read()
        .core
        .transport
        .as_ref()
        .is_none_or(|current| !std::ptr::eq(current.as_ref(), transport))
    {
        return;
    }
    let result = result.and_then(|page| {
        state
            .read()
            .core
            .finish_file_display_read(team, file_read)
            .map(|files| (page, files))
            .map_err(|reason| super_platinum_core::slack::Error::Api(reason.into()))
    });
    match result {
        Ok((page, files)) => {
            let mut shell = state.write();
            let upload_ids =
                crate::messaging::upload_recovery::incoming(&shell, team, &channel, &page.messages);
            if let Some(workspace) = shell.core.workspaces.get_mut(team) {
                let messages = workspace.messages.entry(channel.clone()).or_default();
                for message in page.messages {
                    let message = files.message(message);
                    if super_platinum_core::state::is_channel_timeline_visible(&message) {
                        messages.upsert(message);
                    }
                }
                messages.loaded = true;
                messages.history_failed = false;
                messages.history_refreshing = false;
                messages.has_more_older = page.has_more;
            }
            crate::messaging::upload_recovery::reconcile(
                &mut shell,
                team,
                &channel,
                Some(&upload_ids),
            );
            shell.refresh_from_core();
            drop(shell);
            dioxus::prelude::spawn(crate::performance::mark_history_painted(
                *state,
                team.to_owned(),
                channel,
                generation,
            ));
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
    persist_snapshot(state, team, false);
}

pub(crate) fn persist_workspace_metadata(state: &Signal<ShellState>, team: &str) {
    persist_snapshot(state, team, true);
}

fn persist_snapshot(state: &Signal<ShellState>, team: &str, metadata_only: bool) {
    let state = state.read();
    if state.core.cache.is_none() {
        return;
    }
    let (Some(account), Some(workspace)) = (
        state.core.active_account.clone(),
        state.core.workspaces.get(team),
    ) else {
        return;
    };
    let snapshot = if metadata_only {
        super_platinum_core::cache::WorkspaceSnapshot::metadata(workspace)
    } else {
        super_platinum_core::cache::WorkspaceSnapshot::full(workspace)
    };
    drop(state);
    persistence_worker().enqueue(account, snapshot);
}

// Only the newest unsaved snapshot per account/team is needed. Never discard a
// new snapshot just because a previous save is still on disk.
struct PersistenceQueue {
    pending: std::sync::Mutex<
        std::collections::HashMap<(String, String), super_platinum_core::cache::WorkspaceSnapshot>,
    >,
    ready: std::sync::Condvar,
}

impl PersistenceQueue {
    fn enqueue(
        &self,
        account: String,
        mut snapshot: super_platinum_core::cache::WorkspaceSnapshot,
    ) {
        let mut pending = self.pending.lock().unwrap();
        let key = (account, snapshot.team_id().to_owned());
        if let Some(older) = pending.remove(&key) {
            snapshot.retain_older_transcripts(older);
        }
        pending.insert(key, snapshot);
        self.ready.notify_one();
    }

    fn next(&self) -> (String, super_platinum_core::cache::WorkspaceSnapshot) {
        let mut pending = self.pending.lock().unwrap();
        loop {
            if let Some(key) = pending.keys().next().cloned() {
                let workspace = pending.remove(&key).unwrap();
                return (key.0, workspace);
            }
            pending = self.ready.wait(pending).unwrap();
        }
    }
}

fn persistence_worker() -> &'static PersistenceQueue {
    static WORKER: std::sync::OnceLock<PersistenceQueue> = std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        let queue = PersistenceQueue {
            pending: std::sync::Mutex::new(std::collections::HashMap::new()),
            ready: std::sync::Condvar::new(),
        };
        std::thread::Builder::new()
            .name("super-platinum-cache".into())
            .spawn(|| {
                loop {
                    let (account, workspace) = persistence_worker().next();
                    let result = super_platinum_core::cache::Cache::open_default(&account, false)
                        .and_then(|cache| workspace.save(&cache));
                    if let Err(error) = result {
                        eprintln!(
                            "super-platinum: cache save failed for {}: {error}",
                            workspace.team_id()
                        );
                    }
                }
            })
            .expect("cache persistence worker starts");
        queue
    })
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn metadata_enqueue_keeps_earlier_unsaved_transcript() {
        use super_platinum_core::{
            cache::{Cache, WorkspaceSnapshot},
            config::WorkspaceSession,
            slack::models::{Channel, Message},
            state::{ChannelMessages, Workspace},
        };
        let session = WorkspaceSession {
            team_id: "T".into(),
            enterprise_id: None,
            user_id: "U".into(),
            name: "old".into(),
            url: "https://example.slack.com".into(),
            token: String::new(),
        };
        let mut ws = Workspace::from_session(&session);
        ws.channels.insert(
            "C".into(),
            Channel {
                id: "C".into(),
                ..Default::default()
            },
        );
        ws.messages.insert(
            "C".into(),
            ChannelMessages {
                loaded: true,
                messages: vec![Message {
                    ts: Some("1.000001".into()),
                    text: Some("unsaved".into()),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let queue = PersistenceQueue {
            pending: Default::default(),
            ready: std::sync::Condvar::new(),
        };
        queue.enqueue("account".into(), WorkspaceSnapshot::full(&ws));
        ws.name = "new metadata".into();
        ws.messages.clear();
        queue.enqueue("account".into(), WorkspaceSnapshot::metadata(&ws));
        assert_eq!(queue.pending.lock().unwrap().len(), 1);
        let (_, snapshot) = queue.next();
        let cache = Cache::open(":memory:").unwrap();
        snapshot.save(&cache).unwrap();
        let loaded = cache.load_workspace(&session).unwrap().unwrap();
        assert_eq!(loaded.name, "new metadata");
        assert_eq!(
            loaded.messages["C"].messages[0].text.as_deref(),
            Some("unsaved")
        );
    }

    #[test]
    fn pending_snapshots_keep_latest_per_account_and_team() {
        let queue = PersistenceQueue {
            pending: std::sync::Mutex::new(std::collections::HashMap::new()),
            ready: std::sync::Condvar::new(),
        };
        let mut first = super_platinum_core::state::Workspace::from_session(
            &super_platinum_core::config::WorkspaceSession {
                team_id: "T".into(),
                enterprise_id: None,
                user_id: "U".into(),
                name: "old".into(),
                url: "https://example.slack.com".into(),
                token: String::new(),
            },
        );
        queue.enqueue(
            "account".into(),
            super_platinum_core::cache::WorkspaceSnapshot::full(&first),
        );
        first.name = "new".into();
        queue.enqueue(
            "account".into(),
            super_platinum_core::cache::WorkspaceSnapshot::full(&first),
        );
        queue.enqueue(
            "other".into(),
            super_platinum_core::cache::WorkspaceSnapshot::full(&first),
        );
        assert_eq!(queue.pending.lock().unwrap().len(), 2);
        let jobs = [queue.next(), queue.next()];
        for (_, snapshot) in &jobs {
            let cache = super_platinum_core::cache::Cache::open(":memory:").unwrap();
            snapshot.save(&cache).unwrap();
            let session = super_platinum_core::config::WorkspaceSession {
                team_id: "T".into(),
                enterprise_id: None,
                user_id: "U".into(),
                name: String::new(),
                url: String::new(),
                token: String::new(),
            };
            assert_eq!(cache.load_workspace(&session).unwrap().unwrap().name, "new");
        }
        assert!(
            jobs.iter()
                .any(|(account, ws)| account == "account" && ws.team_id() == "T")
        );
        assert!(
            jobs.iter()
                .any(|(account, ws)| account == "other" && ws.team_id() == "T")
        );
    }
}

/// Re-drives whatever the reader is looking at, after the link comes back.
///
/// Every load that fired during an outage failed, and nothing else would ever
/// ask again: the channel keeps whatever the cache held, the thread pane keeps
/// saying "Loading replies…", and the reader is left with a screen that quietly
/// stopped being true. Only reached on the transition back to connected, so
/// this costs one round of the same calls the surface makes on arrival.
pub async fn reload_after_outage(mut state: Signal<ShellState>) {
    crate::slack_lists::invalidate_visible_for_reconnect(&mut state.write());
    crate::slack_lists::refresh(state);
    let resources_visible = {
        let shell = state.peek();
        shell.detail_open() && shell.channel_tab != crate::state::ChannelTab::Messages
    };
    if resources_visible {
        crate::channel_resources::refresh(state).await;
        return;
    }
    let (view, thread) = {
        let shell = state.read();
        (
            shell.main_view,
            shell
                .thread_root
                .clone()
                .zip(shell.core.active_channel.clone()),
        )
    };
    super::load_main_view(state, view).await;
    super::refresh_selected_channel(state).await;
    if let Some((root, channel)) = thread {
        super::open_thread(state, channel, root).await;
    }
}
