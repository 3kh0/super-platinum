use super::*;
use super_platinum_core::{
    config::Session,
    slack::{
        Transport,
        realtime::{ConnectParams, flannel_url},
    },
};
const COOKIE: &str = "hosted-composer-disconnected-cookie";
const TOKEN: &str = "hosted-composer-disconnected-token";
pub(super) fn synthetic_session(session: &Session) -> bool {
    session.d_cookie == COOKIE
        && session.workspaces.len() == 1
        && session.workspaces.get("T1").is_some_and(|w| {
            w.team_id == "T1"
                && w.user_id == "U1"
                && w.token == TOKEN
                && w.url == "https://fixture.slack.com"
                && w.enterprise_id.is_none()
        })
}
fn isolated(shell: &ShellState) -> bool {
    shell.core.active_account.is_none()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.cache.is_none()
        && shell.core.active_team.as_deref() == Some("T1")
        && shell.core.active_channel.as_deref() == Some("C1")
}
pub(super) async fn install(
    mut state: Signal<ShellState>,
) -> Result<(Arc<Run>, socket::Worker), String> {
    let config = isolation::configuration()?;
    if RUN.get().is_some() || !isolated(&state.peek()) {
        return Err("Initial isolation refused".into());
    }
    let original = (
        state.peek().core.account_epoch,
        state.peek().draft_writes.view_id,
    );
    let checked = config.clone();
    let (route, fixture, transport) = tokio::task::spawn_blocking(move || {
        isolation::validate(&checked)?;
        let case = match checked.scenario.as_str() {
            "file-only" => o::HostedFixtureCase::FileOnly,
            "unknown" => o::HostedFixtureCase::Unknown,
            "storage" => o::HostedFixtureCase::Storage,
            _ => o::HostedFixtureCase::Caption,
        };
        let (route, fixture) =
            o::OwnedRoute::fresh_hosted_fixture(case).map_err(|e| format!("Seed: {e:?}"))?;
        Ok::<_, String>((
            route,
            Arc::new(fixture),
            Arc::new(Transport::disconnected_fixture(COOKIE)),
        ))
    })
    .await
    .map_err(|_| "Seed task lost")??;
    let acquired = root()
        .lock()
        .map_err(|_| "Root lock")?
        .acquire_configured_fixture(route)
        .map_err(|e| format!("Configured acquisition: {e:?}"))?;
    acquired.ready().await;
    let account = acquired
        .account()
        .map_err(|e| format!("Configured initialization: {e:?}"))?;
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    apply(&mut shell);
    shell.core.account_epoch = original.0.checked_add(1).ok_or("Epoch exhausted")?;
    shell.core.active_account = Some(account.id().into());
    shell.core.transport = Some(transport.clone());
    let workspace = shell.core.workspaces.get_mut("T1").ok_or("Missing team")?;
    workspace.self_user_id = "U1".into();
    // Use the real model decoder for the same canonical fixed channel returned
    // by the narrow read oracle; no defaulted permission fact grants admission.
    let channel: super_platinum_core::slack::models::Channel =
        serde_json::from_value(o::HostedFixture::channel()).map_err(|_| "Fixed channel decode")?;
    workspace.channels.insert(channel.id.clone(), channel);
    shell.self_account.user_id = "U1".into();
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: COOKIE.into(),
        workspaces: [("T1".into(), fixture.session())].into(),
    });
    shell.refresh_from_core();
    let index = shell
        .channels
        .iter()
        .position(|c| c.id == "C00000001")
        .ok_or("Fixture channel not projected")?;
    shell.select_channel(index, crate::state::ChannelOpen::Global);
    restore(&mut shell)
        .await
        .map_err(|e| format!("Configured restore: {e:?}"))?;
    if !isolated(&state.peek())
        || (
            state.peek().core.account_epoch,
            state.peek().draft_writes.view_id,
        ) != original
    {
        return Err("Fixture owner replaced during initialization".into());
    }
    let (mut worker, _never_connect) = socket::Worker::capture(
        &mut shell,
        ConnectParams {
            team: "T1".into(),
            ws_url: flannel_url(TOKEN, "T1"),
            d_cookie: COOKIE.into(),
            user_agent: "closed-hosted-composer-fixture".into(),
        },
    )
    .map_err(|e| format!("WorkerSource capture: {e:?}"))?;
    shell
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Missing team")?
        .rt_generation = 9;
    let accepted = worker
        .accept(&mut shell, "T1", 9)
        .map_err(|e| format!("WorkerSource acceptance: {e:?}"))?;
    let run = Arc::new(Run {
        account,
        fixture,
        transport,
        epoch: shell.core.account_epoch,
        view: shell.draft_writes.view_id,
        config,
        native: Mutex::new(vec![]),
        response: response_gate::Gate::new(),
    });
    RUN.set(run.clone())
        .map_err(|_| "Fixture already initialized")?;
    *state.write() = shell;
    worker
        .connected(state, accepted, 9)
        .await
        .map_err(|e| format!("Actual Source opening: {e:?}"))?;
    if !run.current(&state.peek()) {
        return Err("Owner changed after Source opening".into());
    }
    Ok((run, worker))
}
