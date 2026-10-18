//! Read-only bot metadata loading, started only by rendered ordinary buttons.
use super::source::Source;
use crate::{scheduled::Context, state::ShellState};
use dioxus::prelude::*;
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use super_platinum_core::slack::{Error, api};

pub(crate) fn now() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// Cache results belong to their account/workspace/transport even if another
/// channel is visible. They never dispatch an action or modify a message.
fn current(shell: &ShellState, context: &Context) -> bool {
    shell.core.account_epoch == context.epoch
        && shell
            .core
            .transport
            .as_ref()
            .is_some_and(|t| Arc::ptr_eq(t, &context.transport))
        && shell
            .core
            .workspaces
            .get(&context.session.team_id)
            .is_some_and(|w| w.self_user_id == context.session.user_id)
        && shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&context.session.team_id))
            .is_some_and(|w| {
                w.user_id == context.session.user_id && w.token == context.session.token
            })
}

pub(crate) fn pending(shell: &ShellState, source: &Source) -> bool {
    source.lookup_target(shell).is_some_and(|(bot, hint)| {
        shell.core.workspaces[&source.owner().team_id]
            .bots
            .is_pending(&bot, hint.as_deref(), now())
    })
}

pub(crate) fn ensure(state: Signal<ShellState>, source: Source) {
    load(state, source, false);
}

pub(crate) fn retry(state: Signal<ShellState>, source: Source) {
    load(state, source, true);
}

fn load(mut state: Signal<ShellState>, source: Source, explicit: bool) {
    // The component effect must not subscribe to the entire shell; otherwise a
    // cache write would restart it. Source and hydration_revision are explicit.
    let candidate = {
        let shell = state.peek();
        source.lookup_target(&shell)
    };
    let Some((bot, hint)) = candidate else {
        return;
    };
    if super::fixture::hydrate(state, source.clone()) {
        return;
    }
    let Some(context) = Context::capture(&state.peek()) else {
        return;
    };
    if context.session.team_id != source.owner().team_id {
        return;
    }
    let ticket = {
        #[allow(deprecated)]
        let mut shell = state.write_silent();
        if !current(&shell, &context)
            || source.lookup_target(&shell).as_ref() != Some(&(bot.clone(), hint.clone()))
        {
            return;
        }
        let cache = &mut shell
            .core
            .workspaces
            .get_mut(&context.session.team_id)
            .unwrap()
            .bots;
        if explicit {
            cache.begin_retry(&bot, hint.as_deref(), now())
        } else {
            cache.begin(&bot, hint.as_deref(), now())
        }
    };
    let Some(ticket) = ticket else {
        return;
    };
    drop(state.write());
    dioxus::core::spawn_forever(async move {
        if !current(&state.peek(), &context) {
            return;
        }
        let result = match api::bots_info(
            &context.client,
            &context.session,
            ticket.bot_id(),
            ticket.app_id_hint(),
        ) {
            Ok(request) => context.transport.execute(request).await.and_then(|value| {
                api::decode_bots_info(&value, ticket.bot_id(), ticket.app_id_hint())
            }),
            Err(error) => Err(error),
        };
        let mut shell = state.write();
        if !current(&shell, &context) {
            return;
        }
        let cache = &mut shell
            .core
            .workspaces
            .get_mut(&context.session.team_id)
            .unwrap()
            .bots;
        match result {
            Ok(profile) => {
                cache.complete(&ticket, profile, now());
            }
            Err(error) => {
                let cooldown = match error {
                    Error::RateLimited {
                        retry_after_secs: Some(seconds),
                    } => Duration::from_secs(seconds).max(Duration::from_secs(30)),
                    _ => Duration::from_secs(30),
                };
                cache.fail(&ticket, now(), cooldown);
            }
        }
        // Wake rendered candidates that hit the concurrency bound. Backoff and
        // in-flight tickets prevent failures or duplicates causing a read loop.
        shell.message_buttons.hydration_revision =
            shell.message_buttons.hydration_revision.wrapping_add(1);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::Transport,
    };

    #[test]
    fn metadata_completion_survives_navigation_but_not_identity_or_transport_replacement() {
        for change in 0..7 {
            let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
            let workspace = WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: shell.core.workspaces["T1"].self_user_id.clone(),
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: "synthetic-only".into(),
            };
            shell.core.session = Some(Session {
                unified_user_client_enabled: Default::default(),
                d_cookie: "synthetic-only".into(),
                workspaces: [("T1".into(), workspace)].into(),
            });
            shell.core.transport = Some(Arc::new(Transport::new("synthetic-only").unwrap()));
            let context = Context::capture(&shell).unwrap();
            match change {
                0 => {
                    shell.core.active_team = Some("T2".into());
                    shell.core.active_channel = Some("other".into());
                }
                1 => shell.core.account_epoch += 1,
                2 => shell.core.transport = Some(Arc::new(Transport::new("replacement").unwrap())),
                3 => {
                    shell.core.workspaces.remove("T1");
                }
                4 => shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "other".into(),
                5 => {
                    shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .token = "rotated".into()
                }
                _ => shell.core.session = None,
            }
            assert_eq!(current(&shell, &context), change == 0);
        }
    }
}
