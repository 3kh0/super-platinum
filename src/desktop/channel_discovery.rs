//! Hydrate self-join hints without navigating away from the current conversation.
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{
    CoreAppState,
    config::WorkspaceSession,
    slack::{SlackClient, Transport, api, events::RtEvent},
};

use crate::state::ShellState;

struct Scope {
    transport: Arc<Transport>,
    client: SlackClient,
    session: WorkspaceSession,
    epoch: u64,
    generation: u64,
}

pub(crate) struct Read {
    scope: Scope,
    channel: String,
    revision: u64,
}

impl Scope {
    fn capture(core: &CoreAppState, team: &str, generation: u64) -> Option<Self> {
        let transport = core.transport.clone()?;
        let session = core.session.as_ref()?.workspaces.get(team)?.clone();
        if core.workspaces.get(team)?.rt_generation != generation {
            return None;
        }
        Some(Self {
            transport,
            client: core.client.clone(),
            session,
            epoch: core.account_epoch,
            generation,
        })
    }
    fn current(&self, core: &CoreAppState) -> bool {
        core.account_epoch == self.epoch
            && core
                .transport
                .as_ref()
                .is_some_and(|transport| Arc::ptr_eq(transport, &self.transport))
            && core
                .workspaces
                .get(&self.session.team_id)
                .is_some_and(|workspace| workspace.rt_generation == self.generation)
    }
}

pub(crate) fn prepare(
    core: &mut CoreAppState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> Option<Read> {
    let scope = Scope::capture(core, team, generation)?;
    let (channel, revision) = core
        .workspaces
        .get_mut(team)?
        .begin_channel_discovery(event)?;
    Some(Read {
        scope,
        channel,
        revision,
    })
}

impl Read {
    fn current(&self, core: &CoreAppState) -> bool {
        self.scope.current(core)
    }

    pub(crate) async fn run(self, mut state: Signal<ShellState>) {
        if !self.current(&state.peek().core) {
            return;
        }
        let result = api::fetch_conversation_details(
            &self.scope.transport,
            &self.scope.client,
            &self.scope.session,
            &self.channel,
        )
        .await;
        let changed = {
            let mut shell = state.write();
            if !self.current(&shell.core) {
                return;
            }
            let workspace = shell
                .core
                .workspaces
                .get_mut(&self.scope.session.team_id)
                .unwrap();
            let changed =
                workspace.finish_channel_discovery(&self.channel, self.revision, result.ok());
            if changed && shell.core.active_team.as_deref() == Some(&self.scope.session.team_id) {
                shell.refresh_from_core();
            }
            changed
        };
        if changed {
            crate::bootstrap::persist_workspace(&state, &self.scope.session.team_id);
        }
    }
}

/// Reconcile the complete channel inventory once per reconnect, including
/// background workspaces. Does not boot the session or start another socket.
pub(crate) async fn reconcile(mut state: Signal<ShellState>, team: String, generation: u64) {
    let Some(scope) = Scope::capture(&state.peek().core, &team, generation) else {
        return;
    };
    let revision = state.peek().core.workspaces[&team]
        .channel_lifecycle
        .revision();
    let thread_revision = state.peek().core.workspaces[&team].thread_counts.revision();
    let counts = api::fetch_counts(&scope.transport, &scope.client, &scope.session).await;
    if !scope.current(&state.peek().core) {
        return;
    }
    let counts = match counts {
        Ok(counts) => counts,
        Err(error) => {
            report_failure(state, &scope, &error);
            return;
        }
    };
    let batches = {
        let mut shell = state.write();
        let workspace = shell.core.workspaces.get_mut(&team).unwrap();
        workspace.apply_counts_snapshot(counts, revision, thread_revision);
        let batches = workspace.channels_to_refresh();
        if shell.core.active_team.as_deref() == Some(&team) {
            shell.refresh_from_core();
        }
        batches
    };
    for batch in batches {
        if !scope.current(&state.peek().core) {
            return;
        }
        let revision = state.peek().core.workspaces[&team]
            .channel_lifecycle
            .revision();
        let result =
            api::fetch_generic_info(&scope.transport, &scope.client, &scope.session, &batch).await;
        if !scope.current(&state.peek().core) {
            return;
        }
        match result {
            Ok(page) if !page.channels.is_empty() => {
                let mut shell = state.write();
                shell
                    .core
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .apply_channel_refresh_since(page, revision);
                if shell.core.active_team.as_deref() == Some(&team) {
                    shell.refresh_from_core();
                }
            }
            Ok(_) => {}
            Err(error) => {
                report_failure(state, &scope, &error);
                break;
            }
        }
    }
    crate::bootstrap::persist_workspace(&state, &team);
}

fn report_failure(
    mut state: Signal<ShellState>,
    scope: &Scope,
    error: &super_platinum_core::slack::Error,
) {
    if scope.current(&state.peek().core)
        && state.peek().core.active_team.as_deref() == Some(&scope.session.team_id)
    {
        state
            .write()
            .report_failure(error, "Channel sync needs a connection", || {
                "Could not refresh the channel list after reconnecting.".into()
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_is_account_and_socket_scoped_but_survives_navigation() {
        let mut core = crate::fixture::fixture_core();
        core.session = Some(super_platinum_core::config::Session {
            d_cookie: "fixture-cookie".into(),
            workspaces: [(
                "T1".into(),
                WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: "U0".into(),
                    name: "Fixture".into(),
                    url: "https://example.invalid".into(),
                    token: String::new(),
                },
            )]
            .into(),
        });
        core.transport = Some(Arc::new(Transport::new("fixture-cookie").unwrap()));
        core.workspaces.get_mut("T1").unwrap().rt_generation = 4;
        let event = super_platinum_core::slack::realtime::parse_event(
            r#"{"type":"channel_joined","channel":{"id":"C_NEW"}}"#,
        )
        .unwrap();
        assert!(prepare(&mut core, "T1", 3, &event).is_none());
        let read = prepare(&mut core, "T1", 4, &event).unwrap();
        assert!(read.current(&core));
        assert!(prepare(&mut core, "T1", 4, &event).is_none());
        core.active_team = Some("T2".into());
        assert!(
            read.current(&core),
            "background workspace still receives discoveries"
        );
        core.workspaces.get_mut("T1").unwrap().rt_generation = 5;
        assert!(!read.current(&core));
        core.workspaces.get_mut("T1").unwrap().rt_generation = 4;
        core.account_epoch += 1;
        assert!(!read.current(&core));
        core.account_epoch -= 1;
        core.transport = Some(Arc::new(
            Transport::new("replacement-fixture-cookie").unwrap(),
        ));
        assert!(!read.current(&core));
    }
}
