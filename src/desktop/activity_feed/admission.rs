//! Own the interval between selecting an Activity row and first polling its task.
use std::sync::Arc;

use dioxus::prelude::*;
use serde_json::Value;
use super_platinum_core::{config::WorkspaceSession, slack::Transport};

use super::navigation::{ThreadDestination, thread_destination};
use crate::state::{MainView, Overlay, ShellState};

pub(crate) struct Navigation {
    account: Option<String>,
    epoch: u64,
    team: String,
    user: String,
    session: Option<WorkspaceSession>,
    cookie: Option<String>,
    transport: Option<Arc<Transport>>,
    view: MainView,
    overlay: Option<Overlay>,
    channel_generation: u64,
    thread_generation: u64,
    key: String,
    feed_ts: String,
    item: Value,
    channel: String,
    thread: Option<ThreadDestination>,
}

impl Navigation {
    /// Capture in the successful selection handler, never inside the queued task.
    pub(crate) fn capture(shell: &ShellState, key: &str) -> Option<Self> {
        let item = shell
            .core
            .activity
            .items
            .iter()
            .find(|item| item.key == key)?;
        let channel = item.channel()?.to_owned();
        if shell.core.activity.selected.as_deref() != Some(key)
            || shell.core.active_channel.as_ref() != Some(&channel)
        {
            return None;
        }
        let team = shell.core.active_team.clone()?;
        let user = shell.core.workspaces.get(&team)?.self_user_id.clone();
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&team))
            .cloned();
        if session
            .as_ref()
            .is_some_and(|s| s.team_id != team || s.user_id != user)
        {
            return None;
        }
        Some(Self {
            account: shell.core.active_account.clone(),
            epoch: shell.core.account_epoch,
            team,
            user,
            session,
            cookie: shell.core.session.as_ref().map(|s| s.d_cookie.clone()),
            transport: shell.core.transport.clone(),
            view: shell.main_view,
            overlay: shell.overlay,
            channel_generation: shell.channel_generation,
            thread_generation: shell.thread_generation,
            key: key.into(),
            feed_ts: item.feed_ts.clone(),
            item: serde_json::to_value(&item.item).ok()?,
            channel,
            thread: thread_destination(item),
        })
    }

    fn current(&self, shell: &ShellState) -> bool {
        let session = shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&self.team));
        self.account == shell.core.active_account
            && self.epoch == shell.core.account_epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell
                .core
                .workspaces
                .get(&self.team)
                .is_some_and(|w| w.self_user_id == self.user)
            && self.cookie.as_deref() == shell.core.session.as_ref().map(|s| s.d_cookie.as_str())
            && match (&self.session, session) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.team_id == b.team_id
                        && a.user_id == b.user_id
                        && a.token == b.token
                        && a.url == b.url
                        && a.enterprise_id == b.enterprise_id
                }
                _ => false,
            }
            && match (&self.transport, &shell.core.transport) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
            && shell.main_view == self.view
            && shell.overlay == self.overlay
            && shell.channel_generation == self.channel_generation
            && shell.thread_generation == self.thread_generation
            && shell.core.active_channel.as_ref() == Some(&self.channel)
            && shell.core.activity.selected.as_ref() == Some(&self.key)
            && shell.core.activity.items.iter().any(|item| {
                item.key == self.key
                    && item.feed_ts == self.feed_ts
                    && serde_json::to_value(&item.item).ok().as_ref() == Some(&self.item)
            })
    }

    /// Admission and initial mutation share one borrow. Advancing the existing
    /// thread generation makes even an identical second ticket stale.
    fn admit(&self, shell: &mut ShellState) -> Option<String> {
        if !self.current(shell) {
            return None;
        }
        if let Some(thread) = &self.thread {
            shell.begin_thread(thread.root.clone(), thread.target.clone());
            shell.remember_surface();
        } else {
            // Selection already closed the thread; invalidate duplicate queued
            // non-thread admissions without changing the selected conversation.
            shell.close_thread();
        }
        Some(shell.thread_scope())
    }

    pub(crate) async fn run(self, mut state: Signal<ShellState>) {
        let scope = { self.admit(&mut state.write()) };
        let Some(scope) = scope else {
            return;
        };
        // Both futures begin polling in this same task, after admission. Do not
        // spawn an independently queued unguarded mark-read task.
        let mark = crate::bootstrap::mark_activity_read(state, self.key);
        let open = async {
            if let Some(thread) = self.thread {
                crate::bootstrap::load_started_thread(
                    state,
                    self.channel,
                    thread.root,
                    thread.target,
                    scope,
                )
                .await;
            } else {
                crate::bootstrap::refresh_selected_channel(state).await;
            }
        };
        tokio::join!(mark, open);
    }
}

#[cfg(test)]
mod tests;
