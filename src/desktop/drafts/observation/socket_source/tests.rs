use super::*;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::{Transport, realtime::flannel_url},
};

fn fixture() -> (ShellState, ConnectParams) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000071".into());
    shell.core.account_epoch = 7;
    shell.core.transport = Some(Arc::new(Transport::new("fixture-only-cookie").unwrap()));
    shell.core.session = Some(Session {
        d_cookie: "fixture-only-cookie".into(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team.clone(),
                enterprise_id: None,
                user_id: user,
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-only-token".into(),
            },
        )]
        .into(),
    });
    let params = ConnectParams {
        team: team.clone(),
        ws_url: flannel_url("fixture-only-token", &team),
        d_cookie: "fixture-only-cookie".into(),
        user_agent: "fixture-only-agent".into(),
    };
    (shell, params)
}

fn connected(worker: &mut WorkerSource, shell: &mut ShellState, generation: u64) -> Source {
    let team = worker.team.clone();
    assert!(worker.can_accept(shell, &team, generation));
    shell.core.workspaces.get_mut(&team).unwrap().rt_generation = generation;
    worker.accept(shell, &team, generation).unwrap()
}

#[test]
fn capture_precedes_generation_and_consumes_exact_validated_params_once() {
    let (mut shell, params) = fixture();
    shell
        .core
        .workspaces
        .get_mut(&params.team)
        .unwrap()
        .rt_generation = 0;
    let expected = params.clone();
    let mut worker = WorkerSource::capture(&mut shell, params).unwrap();
    assert!(worker.accept(&mut shell, &expected.team, 1).is_err());
    // Boolean assertion deliberately avoids Debug-printing credential params.
    assert!(worker.take_connect_params(&shell).unwrap() == expected);
    assert!(worker.take_connect_params(&shell).is_err());
    assert!(worker.accept(&mut shell, &expected.team, 0).is_err());
    assert!(worker.accept(&mut shell, "OTHER", 1).is_err());
    assert!(worker.accept(&mut shell, &expected.team, 1).is_err());
    let source = connected(&mut worker, &mut shell, 1);
    assert!(source.current(&shell, 1));
    assert!(!source.current(&shell, 2));
    let key = DraftKey {
        team: expected.team.clone(),
        channel: "C1".into(),
        thread: None,
    };
    let raw: RawEvent = serde_json::from_value(serde_json::json!({"type":"draft_delete", "draft_id":"Dr1", "client_msg_id":"00000000-0000-4000-8000-000000000072", "event_ts":"10.000001"})).unwrap();
    assert!(source.prepare(&shell, 1, &key, &raw).is_ok());
    assert!(matches!(
        source.prepare(&shell, 2, &key, &raw),
        Err(Invalid::Source)
    ));
    assert!(worker.accept(&mut shell, &expected.team, 1).is_err());
}

#[test]
fn invalid_socket_parameters_or_user_cannot_replace_existing_worker() {
    for change in ["cookie", "url", "team", "user"] {
        let (mut shell, params) = fixture();
        let worker = WorkerSource::capture(&mut shell, params.clone()).unwrap();
        let mut invalid = params.clone();
        match change {
            "cookie" => invalid.d_cookie = "wrong-fixture-cookie".into(),
            "url" => invalid.ws_url.push_str("&unvalidated=1"),
            "team" => invalid.team = "OTHER".into(),
            "user" => {
                shell
                    .core
                    .workspaces
                    .get_mut(&params.team)
                    .unwrap()
                    .self_user_id = "OTHER".into()
            }
            _ => unreachable!(),
        }
        assert!(WorkerSource::capture(&mut shell, invalid).is_err());
        assert!(
            shell.draft_writes.socket_sources.teams[&params.team]
                .worker
                .same(worker.identity())
        );
    }
}

#[test]
fn same_generation_replacement_invalidates_old_proof_and_stale_cleanup() {
    let (mut shell, params) = fixture();
    let mut first = WorkerSource::capture(&mut shell, params.clone()).unwrap();
    first.take_connect_params(&shell).unwrap();
    let old = connected(&mut first, &mut shell, 1);
    let mut second = WorkerSource::capture(&mut shell, params.clone()).unwrap();
    assert!(!first.current(&shell));
    assert!(!old.current(&shell, 1));
    second.take_connect_params(&shell).unwrap();
    let new = connected(&mut second, &mut shell, 1);
    assert!(!old.identity().same(new.identity()));
    first.disconnected(&mut shell, &params.team, 1);
    first.retire(&mut shell);
    assert!(first.accept(&mut shell, &params.team, 2).is_err());
    assert!(new.current(&shell, 1));
    second.retire(&mut shell);
    assert!(!new.current(&shell, 1));
}

#[test]
fn reconnect_and_failed_connect_reject_queued_or_duplicate_generations() {
    let (mut shell, params) = fixture();
    let mut worker = WorkerSource::capture(&mut shell, params.clone()).unwrap();
    worker.take_connect_params(&shell).unwrap();
    let old = connected(&mut worker, &mut shell, 1);
    worker.disconnected(&mut shell, &params.team, 1);
    assert!(!old.current(&shell, 1));
    assert!(worker.accept(&mut shell, &params.team, 1).is_err());
    worker.disconnected(&mut shell, &params.team, 2);
    shell
        .core
        .workspaces
        .get_mut(&params.team)
        .unwrap()
        .rt_generation = 2;
    assert!(worker.accept(&mut shell, &params.team, 2).is_err());
    let next = connected(&mut worker, &mut shell, 3);
    assert!(!next.current(&shell, 1));
    assert!(!old.current(&shell, 3));
    worker.disconnected(&mut shell, &params.team, 1);
    assert!(next.current(&shell, 3));
}

#[test]
fn changed_binding_rejects_both_preconnect_acceptance_and_existing_source() {
    for change in [
        "account",
        "epoch",
        "transport",
        "cookie",
        "token",
        "session-user",
        "cached-user",
        "url",
        "enterprise",
        "session-team",
        "cached-team",
        "missing-session",
    ] {
        for before_connect in [true, false] {
            let (mut shell, params) = fixture();
            let mut worker = WorkerSource::capture(&mut shell, params.clone()).unwrap();
            worker.take_connect_params(&shell).unwrap();
            let source = (!before_connect).then(|| connected(&mut worker, &mut shell, 1));
            match change {
                "account" => {
                    shell.core.active_account = Some("00000000-0000-4000-8000-000000000099".into())
                }
                "epoch" => shell.core.account_epoch += 1,
                "transport" => {
                    shell.core.transport =
                        Some(Arc::new(Transport::new("other-fixture-cookie").unwrap()))
                }
                "cookie" => {
                    shell.core.session.as_mut().unwrap().d_cookie = "other-fixture-cookie".into()
                }
                "cached-user" => {
                    shell
                        .core
                        .workspaces
                        .get_mut(&params.team)
                        .unwrap()
                        .self_user_id = "OTHER".into()
                }
                "cached-team" => {
                    shell.core.workspaces.get_mut(&params.team).unwrap().team_id = "OTHER".into()
                }
                "missing-session" => shell.core.session = None,
                field => {
                    let session = shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut(&params.team)
                        .unwrap();
                    match field {
                        "token" => session.token = "other-fixture-token".into(),
                        "session-user" => session.user_id = "OTHER".into(),
                        "session-team" => session.team_id = "OTHER".into(),
                        "url" => session.url = "https://other-fixture.slack.com".into(),
                        "enterprise" => session.enterprise_id = Some("EOTHER".into()),
                        _ => unreachable!(),
                    }
                }
            }
            assert!(
                !worker.current(&shell),
                "mutation {change}, before connect {before_connect}"
            );
            if let Some(source) = source {
                assert!(!source.current(&shell, 1));
            }
            shell
                .core
                .workspaces
                .get_mut(&params.team)
                .unwrap()
                .rt_generation = 2;
            assert!(worker.accept(&mut shell, &params.team, 2).is_err());
        }
    }
}

#[test]
fn dropping_worker_rejects_retained_source_without_shell_cleanup() {
    let (mut shell, params) = fixture();
    let mut worker = WorkerSource::capture(&mut shell, params).unwrap();
    worker.take_connect_params(&shell).unwrap();
    let source = connected(&mut worker, &mut shell, 1);
    assert!(source.current(&shell, 1));
    drop(worker);
    assert!(!source.current(&shell, 1));
}

#[test]
fn connected_preflight_rejects_stale_and_duplicate_before_core_mutation() {
    let (mut shell, params) = fixture();
    let mut worker = WorkerSource::capture(&mut shell, params.clone()).unwrap();
    worker.take_connect_params(&shell).unwrap();
    let source = connected(&mut worker, &mut shell, 2);
    // Model the host order: preflight, then assignment/accept in one shell write.
    for (team, generation) in [
        (&params.team[..], 0),
        (&params.team[..], 1),
        (&params.team[..], 2),
        ("OTHER", 3),
    ] {
        if worker.can_accept(&shell, team, generation) {
            panic!("invalid Connected passed preflight");
        }
        assert_eq!(shell.core.workspaces[&params.team].rt_generation, 2);
        assert!(source.current(&shell, 2));
    }
    assert!(worker.can_accept(&shell, &params.team, 3));
    assert_eq!(shell.core.workspaces[&params.team].rt_generation, 2);
    let next = connected(&mut worker, &mut shell, 3);
    assert!(!source.current(&shell, 2));
    assert!(next.current(&shell, 3));
}

#[test]
fn account_return_requires_new_worker_even_with_original_credentials_and_generation() {
    let (mut shell, params) = fixture();
    let original_account = shell.core.active_account.clone();
    let mut old = WorkerSource::capture(&mut shell, params.clone()).unwrap();
    old.take_connect_params(&shell).unwrap();
    let source = connected(&mut old, &mut shell, 1);
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
    shell.core.account_epoch += 1;
    assert!(!source.current(&shell, 1));
    shell.core.active_account = original_account;
    shell.core.account_epoch += 1;
    assert!(!old.can_accept(&shell, &params.team, 2));
    assert!(!source.current(&shell, 1));
    let mut current = WorkerSource::capture(&mut shell, params.clone()).unwrap();
    current.take_connect_params(&shell).unwrap();
    let next = connected(&mut current, &mut shell, 1);
    old.retire(&mut shell);
    assert!(next.current(&shell, 1));
}

#[test]
fn replacement_shell_or_registry_rejects_old_proof_with_equal_binding_fields() {
    let (mut shell, params) = fixture();
    let mut worker = WorkerSource::capture(&mut shell, params.clone()).unwrap();
    worker.take_connect_params(&shell).unwrap();
    let source = connected(&mut worker, &mut shell, 1);
    let (mut replacement, _) = fixture();
    replacement.core.transport = shell.core.transport.clone();
    replacement
        .core
        .workspaces
        .get_mut(&params.team)
        .unwrap()
        .rt_generation = 1;
    assert!(worker.binding.current(&replacement));
    assert!(!worker.current(&replacement));
    assert!(!source.current(&replacement, 1));
    let mut next = WorkerSource::capture(&mut replacement, params.clone()).unwrap();
    next.take_connect_params(&replacement).unwrap();
    let accepted = connected(&mut next, &mut replacement, 1);
    worker.retire(&mut replacement);
    assert!(accepted.current(&replacement, 1));
    shell.draft_writes.socket_sources = Sources::default();
    assert!(worker.binding.current(&shell));
    assert!(!worker.current(&shell));
    assert!(!source.current(&shell, 1));
}

#[test]
fn background_teams_have_independent_workers_generations_and_retirement() {
    let (mut shell, first_params) = fixture();
    let second_session = WorkspaceSession {
        team_id: "TBACKGROUND".into(),
        enterprise_id: None,
        user_id: "UBACKGROUND".into(),
        name: "Background fixture".into(),
        url: "https://background-fixture.slack.com".into(),
        token: "background-fixture-token".into(),
    };
    shell.core.workspaces.insert(
        second_session.team_id.clone(),
        super_platinum_core::state::Workspace::from_session(&second_session),
    );
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .insert(second_session.team_id.clone(), second_session.clone());
    let second_params = ConnectParams {
        team: second_session.team_id.clone(),
        ws_url: flannel_url(&second_session.token, &second_session.team_id),
        d_cookie: first_params.d_cookie.clone(),
        user_agent: first_params.user_agent.clone(),
    };
    let mut first = WorkerSource::capture(&mut shell, first_params.clone()).unwrap();
    first.take_connect_params(&shell).unwrap();
    let front = connected(&mut first, &mut shell, 1);
    let mut second = WorkerSource::capture(&mut shell, second_params.clone()).unwrap();
    second.take_connect_params(&shell).unwrap();
    let background = connected(&mut second, &mut shell, 4);
    assert_eq!(
        shell.core.active_team.as_deref(),
        Some(first_params.team.as_str())
    );
    assert!(front.current(&shell, 1));
    assert!(background.current(&shell, 4));
    shell.core.active_team = Some(second_params.team.clone());
    assert!(front.current(&shell, 1));
    assert!(background.current(&shell, 4));
    let mut replacement = WorkerSource::capture(&mut shell, first_params.clone()).unwrap();
    replacement.take_connect_params(&shell).unwrap();
    let new_front = connected(&mut replacement, &mut shell, 1);
    assert!(!front.current(&shell, 1));
    assert!(background.current(&shell, 4));
    second.disconnected(&mut shell, &second_params.team, 4);
    assert!(!background.current(&shell, 4));
    assert!(new_front.current(&shell, 1));
}
