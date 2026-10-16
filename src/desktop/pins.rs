//! Native orchestration for channel pins; mutations survive their source row.
mod change_ack;

use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    config::WorkspaceSession,
    pins::{PinnedItem, PinsState},
    slack::{SlackClient, Transport, api, models::Message},
};

struct Context {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    epoch: u64,
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
        })
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.session.team_id)
            && shell
                .core
                .pins
                .matches_scope(self.epoch, &self.session.team_id)
            && shell
                .core
                .transport
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(t, &self.transport))
    }
}
pub fn visible_state(shell: &ShellState) -> Option<&PinsState> {
    shell
        .core
        .pins
        .matches_scope(shell.core.account_epoch, shell.core.active_team.as_deref()?)
        .then_some(&shell.core.pins)
}
pub fn is_pinned(shell: &ShellState, channel: &str, message: &crate::state::MessageVm) -> bool {
    let fallback = message.pinned_to.iter().any(|c| c == channel);
    visible_state(shell)
        .map(|p| p.is_pinned(channel, &message.ts, fallback))
        .unwrap_or(fallback)
}

pub async fn load(mut state: Signal<ShellState>, channel: String) {
    if std::env::var_os("SUPER_PLATINUM_FIXTURE").is_some() {
        return;
    }
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    let generation = {
        let mut shell = state.write();
        shell
            .core
            .pins
            .ensure_scope(ctx.epoch, &ctx.session.team_id);
        if shell.core.pins.loading && shell.core.pins.channel.as_ref() == Some(&channel) {
            return;
        }
        shell.core.pins.begin(&channel)
    };
    let file_read = state
        .write()
        .core
        .begin_file_display_read(&ctx.session.team_id);
    let result = api::fetch_pins(&ctx.transport, &ctx.client, &ctx.session, &channel).await;
    if !ctx.current(&state.peek()) {
        return;
    }
    {
        let mut shell = state.write();
        if shell.core.pins.generation != generation {
            return;
        }
        let result = result.and_then(|page| {
            shell
                .core
                .apply_pin_page_since(&ctx.session.team_id, generation, page, file_read)
                .map_err(|reason| super_platinum_core::slack::Error::Api(reason.into()))
        });
        match result {
            Ok(false) => return,
            Ok(true) => {
                shell
                    .core
                    .reconcile_pin_copies(&ctx.session.team_id, &channel);
                shell.refresh_from_core();
            }
            Err(error) => {
                shell.core.pins.loading = false;
                shell.core.pins.error = Some("Pins could not load. Please try again.".into());
                shell.report_failure(&error, "Pins will be available when you reconnect.", || {
                    "Could not load pins.".into()
                });
                return;
            }
        }
    }
    let missing = {
        let shell = state.peek();
        let Some(workspace) = shell.core.workspaces.get(&ctx.session.team_id) else {
            return;
        };
        shell
            .core
            .pins
            .items
            .values()
            .flat_map(|item| {
                item.created_by
                    .iter()
                    .chain(item.message.as_ref().and_then(|m| m.user.as_ref()))
            })
            .filter(|id| !workspace.users.contains_key(*id))
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    if !missing.is_empty() {
        if let Ok(users) =
            api::fetch_users_info(&ctx.transport, &ctx.client, &ctx.session, missing).await
        {
            let mut shell = state.write();
            if !ctx.current(&shell) {
                return;
            }
            if let Some(workspace) = shell.core.workspaces.get_mut(&ctx.session.team_id) {
                for user in users {
                    workspace.apply_user_update(user);
                }
            }
        }
    }
}

fn message(shell: &ShellState, channel: &str, ts: &str) -> Option<Message> {
    let team = shell.core.active_team.as_ref()?;
    shell
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.messages.get(channel))
        .and_then(|bag| bag.messages.iter().find(|m| m.ts.as_deref() == Some(ts)))
        .cloned()
        .or_else(|| {
            shell
                .core
                .threads
                .iter()
                .filter(|((t, c, _), _)| t == team && c == channel)
                .find_map(|(_, bag)| {
                    bag.messages
                        .iter()
                        .find(|m| m.ts.as_deref() == Some(ts))
                        .cloned()
                })
        })
        .or_else(|| {
            visible_state(shell)
                .filter(|p| p.channel.as_deref() == Some(channel))
                .and_then(|p| p.items.get(&format!("message:{ts}")))
                .and_then(|p| p.message.clone())
        })
}

pub fn change(mut state: Signal<ShellState>, channel: String, ts: String, pinned: bool) {
    let Some(ctx) = Context::get(&state) else {
        return;
    };
    let (item, version) = {
        let mut shell = state.write();
        shell
            .core
            .pins
            .ensure_scope(ctx.epoch, &ctx.session.team_id);
        if !shell.core.pins.busy.insert((channel.clone(), ts.clone())) {
            return;
        }
        let version = shell
            .core
            .pins
            .version(&channel, &format!("message:{ts}"))
            .map(str::to_owned);
        (
            PinnedItem {
                kind: "message".into(),
                channel: Some(channel.clone()),
                created: Some(super_platinum_core::state::now_secs() as i64),
                created_by: Some(ctx.session.user_id.clone()),
                ..Default::default()
            },
            version,
        )
    };
    dioxus::core::spawn_forever(async move {
        let result = api::change_pin(
            &ctx.transport,
            &ctx.client,
            &ctx.session,
            &channel,
            &ts,
            pinned,
        )
        .await;
        if !ctx.current(&state.peek()) {
            return;
        }
        let success = result.is_ok();
        {
            let mut shell = state.write();
            shell.core.pins.busy.remove(&(channel.clone(), ts.clone()));
            match result {
                Ok(()) => {
                    change_ack::apply(
                        &mut shell,
                        &ctx,
                        &channel,
                        &ts,
                        item,
                        pinned,
                        version.as_deref(),
                    );
                    shell.show_toast(if pinned {
                        "Message pinned to this conversation."
                    } else {
                        "Message unpinned."
                    });
                    shell.refresh_from_core();
                }
                Err(error) => shell.report_failure(
                    &error,
                    "Connect to Slack to change pins.",
                    || match &error {
                        super_platinum_core::slack::Error::Api(code) if code == "not_pinnable" => {
                            "This message cannot be pinned.".into()
                        }
                        super_platinum_core::slack::Error::Api(code)
                            if code == "permission_denied" || code == "restricted_action" =>
                        {
                            "You do not have permission to change this pin.".into()
                        }
                        _ => "The pin could not be changed. Please try again.".into(),
                    },
                ),
            }
        }
        if success && state.peek().core.active_channel.as_ref() == Some(&channel) {
            load(state, channel).await;
        }
    });
}

pub fn open(mut state: Signal<ShellState>, channel: String, message: Message) {
    let Some(ts) = message.ts.clone() else {
        return;
    };
    let root = message.thread_ts.clone().filter(|root| root != &ts);
    if !state
        .write()
        .open_search_result(&channel, root.as_deref().unwrap_or(&ts))
    {
        state
            .write()
            .show_toast("This conversation is unavailable.");
        return;
    }
    let (epoch, team, generation) = {
        let shell = state.peek();
        (
            shell.core.account_epoch,
            shell.core.active_team.clone(),
            shell.channel_generation,
        )
    };
    dioxus::core::spawn_forever(async move {
        crate::bootstrap::refresh_selected_channel(state).await;
        let current = {
            let shell = state.peek();
            shell.core.account_epoch == epoch
                && shell.core.active_team == team
                && shell.channel_generation == generation
        };
        if current {
            if let Some(root) = root {
                crate::bootstrap::open_thread_at(state, channel, root, Some(ts)).await;
            }
        }
    });
}

#[cfg(test)]
mod ingress_tests;
