use super::*;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{
        Transport,
        realtime::{ConnectParams, flannel_url},
    },
};
const COOKIE: &str = "owned-baseline-disconnected-cookie";
const TOKEN: &str = "owned-baseline-disconnected-token";
pub(super) fn synthetic_session(session: &Session) -> bool {
    session.d_cookie == COOKIE
        && session.workspaces.len() == 1
        && session.workspaces.get("T1").is_some_and(|w| {
            w.team_id == "T1"
                && w.user_id == "U1"
                && w.token == TOKEN
                && w.url == "https://fixture.slack.com"
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
    let target = if config.scenario.ends_with("thread") {
        ComposerTarget::Thread
    } else {
        ComposerTarget::Channel
    };
    let checked = config.clone();
    let (route, fixture, transport) = tokio::task::spawn_blocking(move || {
        isolation::validate(&checked)?;
        let kind = if target == ComposerTarget::Thread {
            o::BaselineFixtureTarget::Thread
        } else {
            o::BaselineFixtureTarget::Channel
        };
        let outcome = match checked.scenario.as_str() {
            "attachment-channel" => o::BaselineFixtureOutcome::ChangedWithAttachment,
            "cooldown-channel" => o::BaselineFixtureOutcome::RateLimitedThenChanged,
            "local-channel" | "local-thread" => o::BaselineFixtureOutcome::Unchanged,
            _ => o::BaselineFixtureOutcome::Changed,
        };
        let (route, fixture) = o::OwnedRoute::fresh_baseline_fixture(kind, outcome)
            .map_err(|e| format!("Seed: {e:?}"))?;
        let transport = Arc::new(Transport::disconnected_fixture(COOKIE));
        Ok::<_, String>((route, Arc::new(fixture), transport))
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
    shell
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Missing team")?
        .self_user_id = "U1".into();
    shell.core.session = Some(Session {
        unified_user_client_enabled: Default::default(),
        d_cookie: COOKIE.into(),
        workspaces: [(
            "T1".into(),
            WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Disconnected baseline fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: TOKEN.into(),
            },
        )]
        .into(),
    });
    shell.thread_root = fixture.scope().key.thread.clone();
    if let Some(thread) = &fixture.scope().key.thread {
        let mut bag = crate::thread_vm::fixture_bag(4);
        bag.messages[0].ts = Some(thread.clone());
        shell
            .core
            .threads
            .insert(("T1".into(), "C1".into(), thread.clone()), bag);
    }
    // Actual configured initialization/load; no legacy Client, seed Review,
    // enrollment or verify_local substitution for the restored remote basis.
    restore(&mut shell)
        .await
        .map_err(|e| format!("Configured restore: {e:?}"))?;
    shell.refresh_thread_from_core();
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
            user_agent: "closed-baseline-fixture".into(),
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
        target,
        epoch: shell.core.account_epoch,
        view: shell.draft_writes.view_id,
        config,
        native: Mutex::new(Vec::new()),
        checkpoint: Mutex::new(None),
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
