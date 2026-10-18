//! Recover a saved draft's destination without letting late lookups navigate.
use crate::{
    scheduled::Context,
    state::{ChannelOpen, MainView, Overlay, ShellState},
};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    drafts::DraftKey,
    slack::{
        Transport, api,
        models::{Channel, GenericInfoPage},
    },
};
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct Progress {
    revision: u64,
    pub key: Option<DraftKey>,
    pub loading: bool,
    pub error: Option<&'static str>,
}
impl Progress {
    pub fn fixture() -> Self {
        let mut progress = Self::default();
        if let Ok(name) = std::env::var("SUPER_PLATINUM_FIXTURE") {
            if name.starts_with("local-drafts-") {
                progress.key = Some(DraftKey {
                    team: "T1".into(),
                    channel: "C_COLD".into(),
                    thread: None,
                });
                progress.loading = name == "local-drafts-loading";
                progress.error = name.contains("error").then_some(
                    "Could not open this conversation. Your draft is still saved. Try again.",
                );
            }
        }
        progress
    }
}

struct Visit {
    revision: u64,
    key: DraftKey,
    epoch: u64,
    transport: Option<Arc<Transport>>,
    channel: u64,
    thread: u64,
    overlay: Option<Overlay>,
    shortcut: u64,
    palette: u64,
}
impl Visit {
    fn begin(shell: &ShellState, progress: &mut Progress, key: DraftKey) -> Option<Self> {
        if shell.main_view != MainView::Scheduled
            || shell.core.active_team.as_ref() != Some(&key.team)
            || !shell.core.drafts.items.contains_key(&key)
            || (progress.loading && progress.key.as_ref() == Some(&key))
        {
            return None;
        }
        progress.revision = progress.revision.wrapping_add(1);
        progress.key = Some(key.clone());
        progress.loading = true;
        progress.error = None;
        Some(Self {
            revision: progress.revision,
            key,
            epoch: shell.core.account_epoch,
            transport: shell.core.transport.clone(),
            channel: shell.channel_generation,
            thread: shell.thread_generation,
            overlay: shell.overlay,
            shortcut: shell.shortcut_generation,
            palette: shell.palette_generation,
        })
    }
    fn current(&self, shell: &ShellState, progress: &Progress) -> bool {
        progress.revision == self.revision
            && progress.key.as_ref() == Some(&self.key)
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.key.team)
            && shell.main_view == MainView::Scheduled
            && shell.channel_generation == self.channel
            && shell.thread_generation == self.thread
            && shell.overlay == self.overlay
            && shell.shortcut_generation == self.shortcut
            && shell.palette_generation == self.palette
            && shell.core.drafts.items.contains_key(&self.key)
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    fn resolve(
        &self,
        shell: &mut ShellState,
        progress: &Progress,
        channel: Channel,
        revision: u64,
    ) -> bool {
        if !self.current(shell, progress) || channel.id != self.key.channel {
            return false;
        }
        let Some(workspace) = shell.core.workspaces.get_mut(&self.key.team) else {
            return false;
        };
        workspace.apply_channel_refresh_since(
            GenericInfoPage {
                channels: vec![channel],
                ..Default::default()
            },
            revision,
        );
        true
    }
    fn finish(&self, shell: &mut ShellState, progress: &Progress) -> bool {
        if !self.current(shell, progress) {
            return false;
        }
        let Some(workspace) = shell.core.workspaces.get(&self.key.team) else {
            return false;
        };
        let Some(channel) = workspace.channels.get(&self.key.channel) else {
            return false;
        };
        let row = crate::channel_vm::channel_vm(workspace, channel, &shell.media);
        if let Some(index) = shell
            .channels
            .iter()
            .position(|row| row.id == self.key.channel)
        {
            shell.channels[index] = row;
        } else {
            // Project before selecting: changing active_channel first can rebind
            // the outgoing draft to the destination before it is captured.
            shell.channels.push(row);
        }
        let index = shell
            .channels
            .iter()
            .position(|row| row.id == self.key.channel)
            .unwrap();
        shell.checkpoint_composers();
        shell.select_channel(index, ChannelOpen::Global);
        true
    }
}

pub(super) fn resume(mut state: Signal<ShellState>, mut progress: Signal<Progress>, key: DraftKey) {
    let Some(visit) = Visit::begin(&state.peek(), &mut progress.write(), key.clone()) else {
        return;
    };
    let ctx = Context::capture(&state.peek());
    // Unmounting the draft list cancels a pending lookup. Once navigation has
    // succeeded the history task must survive that deliberate unmount.
    spawn(async move {
        let (known, revision) = {
            let shell = state.peek();
            let Some(workspace) = shell.core.workspaces.get(&key.team) else {
                return;
            };
            (
                workspace.channels.contains_key(&key.channel),
                workspace.channel_lifecycle.revision(),
            )
        };
        let mut error = None;
        if !known {
            if let Some(ctx) = ctx {
                let posting = crate::bootstrap::posting_ingress::capture(
                    &state,
                    &ctx.transport,
                    &ctx.session,
                );
                match api::fetch_conversation_info(
                    &ctx.transport,
                    &ctx.client,
                    &ctx.session,
                    key.channel.clone(),
                )
                .await
                {
                    Ok(channel) => {
                        if channel.id == key.channel
                            && visit.current(&state.peek(), &progress.peek())
                        {
                            posting.channels(&state, std::slice::from_ref(&channel));
                        }
                        if !visit.resolve(&mut state.write(), &progress.peek(), channel, revision) {
                            error = Some(
                                "This conversation is unavailable. Your draft is still saved.",
                            );
                        }
                    }
                    Err(super_platinum_core::slack::Error::Offline(_)) => {
                        error = Some(
                            "Connect to Slack, then continue writing. Your draft is still saved.",
                        )
                    }
                    Err(_) => {
                        error = Some(
                            "Could not open this conversation. Your draft is still saved. Try again.",
                        )
                    }
                }
            } else {
                error = Some("Sign in to load this conversation. Your draft is still saved.");
            }
        }
        let Ok(mut pending) = progress.try_write() else {
            return;
        };
        let mut shell = state.write();
        if !visit.current(&shell, &pending) {
            if pending.revision == visit.revision {
                pending.loading = false;
            }
            return;
        }
        pending.loading = false;
        pending.error = error;
        if error.is_some() {
            return;
        }
        if !visit.finish(&mut shell, &pending) {
            pending.error = Some("This conversation is unavailable. Your draft is still saved.");
            return;
        }
        let scope = (
            shell.core.account_epoch,
            shell.core.active_team.clone(),
            shell.channel_generation,
            shell.thread_generation,
        );
        drop(shell);
        drop(pending);
        dioxus::core::spawn_forever(async move {
            let current = {
                let shell = state.peek();
                (
                    shell.core.account_epoch,
                    shell.core.active_team.clone(),
                    shell.channel_generation,
                    shell.thread_generation,
                ) == scope
            };
            if !current {
                return;
            }
            super::super::composer::focus_field_when_mounted(if key.thread.is_some() {
                "thread-composer"
            } else {
                "channel-composer"
            });
            if let Some(root) = key.thread {
                crate::bootstrap::open_thread(state, key.channel, root).await;
            } else {
                crate::bootstrap::refresh_selected_channel(state).await;
            }
        });
    });
}
