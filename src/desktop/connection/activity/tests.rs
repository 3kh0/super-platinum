use super::*;
use super_platinum_core::{slack::realtime::Connection, state::RealtimeStatus};

#[test]
fn actual_client_v2_dispatch_cases_match_the_documented_native_adaptation() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("source-cases.json")).unwrap();
    assert_eq!(cases.len(), 65);
    for case in cases {
        let (mut shell, transport, owner) = setup();
        match case["mode"].as_str().unwrap() {
            "online" => {}
            "degraded" => {
                shell.core.workspaces.get_mut(&owner.team_id).unwrap().rt =
                    RealtimeStatus::Disconnected
            }
            "client-offline" => shell.connection.routable = false,
            "service-offline" => transport.set_health(Health::Offline),
            "missing-connectivity" => shell.connection.routed_at = None,
            _ => panic!("unexpected retained source mode"),
        }
        let observation = Observation::capture(
            &shell,
            shell.core.account_epoch,
            &transport,
            &owner,
            case["firstFormatted"].as_str().unwrap(),
        );
        if case["mode"] == "missing-connectivity" {
            // Explicit unavailable native input, not manufactured Slack false.
            assert!(observation.is_none());
        } else {
            let expected = if case["result"]["path"] == "Remote" {
                Path::Remote
            } else {
                Path::Local
            };
            assert_eq!(observation.unwrap().path(), expected);
        }
    }
}

fn setup() -> (ShellState, Arc<Transport>, WorkspaceSession) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000082".into());
    // Existing closed fixture creates only synthetic credentials and transport.
    crate::app_home::fixture::install(&mut shell);
    shell.signed_in = true;
    shell.loading = false;
    shell.connection.routable = true;
    shell.connection.routed_at = Some(Instant::now());
    let team = shell.core.active_team.as_ref().unwrap().clone();
    let (sender, _) = tokio::sync::mpsc::channel(1);
    shell.core.workspaces.get_mut(&team).unwrap().rt =
        RealtimeStatus::Connected(Connection::from_sender(sender));
    let owner = shell.core.session.as_ref().unwrap().workspaces[&team].clone();
    let transport = shell.core.transport.as_ref().unwrap().clone();
    transport.set_health(Health::Online);
    (shell, transport, owner)
}

#[test]
fn raw_native_mapping_preserves_existing_live_conjunction() {
    for route in [false, true] {
        for connected in [false, true] {
            for health in [Health::Unknown, Health::Offline, Health::Online] {
                let (mut shell, transport, owner) = setup();
                shell.connection.routable = route;
                transport.set_health(health);
                if !connected {
                    shell.core.workspaces.get_mut(&owner.team_id).unwrap().rt =
                        RealtimeStatus::Disconnected;
                }
                let expected = if route && connected && health != Health::Offline {
                    Path::Remote
                } else {
                    Path::Local
                };
                let observation =
                    Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a")
                        .unwrap();
                assert_eq!(observation.path(), expected);
                assert!(observation.current(&shell, "a"));
            }
        }
    }
}

#[test]
fn first_formatted_empty_query_is_local_without_trimming_or_second_stage() {
    let (shell, transport, owner) = setup();
    // These are the actual first-format results; Query owns prefix stripping.
    for (text, expected) in [
        ("", Path::Local),
        (" ", Path::Remote),
        (":", Path::Remote),
        ("+", Path::Remote),
        ("＋", Path::Remote),
        ("@", Path::Remote),
    ] {
        let observed =
            Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, text)
                .unwrap();
        assert_eq!(observed.path(), expected);
        assert!(!observed.current(&shell, &format!("{text}x")));
    }
}

#[test]
fn missing_raw_route_sample_is_unavailable_even_if_rail_looks_online() {
    let (mut shell, transport, owner) = setup();
    shell.connection.routed_at = None;
    assert!(
        Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a").is_none()
    );
}

#[test]
fn owner_credentials_transport_and_realtime_changes_retire_observation() {
    for change in 0..12 {
        let (mut shell, transport, owner) = setup();
        let observed =
            Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a")
                .unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_team = Some("TOTHER".into()),
            2 => shell.core.transport = Some(Arc::new(Transport::new("other-synthetic").unwrap())),
            3 => shell.core.session.as_mut().unwrap().d_cookie.push('x'),
            4 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&owner.team_id)
                .unwrap()
                .token
                .push('x'),
            5 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&owner.team_id)
                .unwrap()
                .url
                .push('x'),
            6 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&owner.team_id)
                .unwrap()
                .user_id
                .push('x'),
            7 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&owner.team_id)
                    .unwrap()
                    .enterprise_id = Some("EOTHER".into())
            }
            8 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&owner.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
            9 => shell
                .core
                .workspaces
                .get_mut(&owner.team_id)
                .unwrap()
                .self_user_id
                .push('x'),
            10 => shell.loading = true,
            _ => shell.signed_in = false,
        }
        assert!(
            !observed.current(&shell, "a"),
            "changed owner input {change}"
        );
    }
}

#[test]
fn decision_downgrade_refuses_remote_and_positive_refresh_does_not_lose_reply() {
    let (mut shell, transport, owner) = setup();
    transport.set_health(Health::Unknown);
    let observed =
        Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a").unwrap();
    transport.set_health(Health::Online);
    shell.connection.routed_at = Some(Instant::now());
    assert!(observed.current(&shell, "a"));
    transport.set_health(Health::Offline);
    assert!(!observed.current(&shell, "a"));
    transport.set_health(Health::Online);
    shell.connection.routable = false;
    assert!(!observed.current(&shell, "a"));
    shell.connection.routable = true;
    shell.core.workspaces.get_mut(&owner.team_id).unwrap().rt = RealtimeStatus::Disconnected;
    assert!(!observed.current(&shell, "a"));
}

#[test]
fn rail_grace_state_does_not_affect_raw_decision() {
    let (mut shell, transport, owner) = setup();
    shell.connection.status = crate::state::ConnectionStatus::NoNetwork;
    assert_eq!(
        Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a")
            .unwrap()
            .path(),
        Path::Remote
    );
    shell.connection.status = crate::state::ConnectionStatus::default();
    shell.connection.routable = false;
    assert_eq!(
        Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a")
            .unwrap()
            .path(),
        Path::Local
    );
}

#[test]
fn complete_auth_session_and_account_identity_retire_pending_observation() {
    for change in 0..9 {
        let (mut shell, transport, owner) = setup();
        let mut other = owner.clone();
        other.team_id = "TOTHER".into();
        shell
            .core
            .session
            .as_mut()
            .unwrap()
            .workspaces
            .insert("TOTHER".into(), other);
        let observed =
            Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a")
                .unwrap();
        match change {
            0 => shell.core.active_account = Some("00000000-0000-4000-8000-000000000099".into()),
            1 => shell.core.active_account = None,
            2 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&owner.team_id)
                .unwrap()
                .name
                .push('x'),
            3 => shell
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("TOTHER")
                .unwrap()
                .token
                .push('x'),
            4 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .remove("TOTHER");
            }
            5 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .unified_user_client_enabled
                    .insert(owner.team_id.clone(), true);
            }
            6 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&owner.team_id)
                    .unwrap()
                    .team_id = "TFOREIGN".into()
            }
            7 => shell.core.session = None,
            _ => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&owner.team_id)
                    .unwrap()
                    .team_id = "TFOREIGN".into()
            }
        }
        assert!(!observed.current(&shell, "a"), "full owner change {change}");
    }
}

#[test]
fn capture_requires_complete_current_owner_and_available_raw_inputs() {
    for change in 0..9 {
        let (mut shell, transport, mut owner) = setup();
        let epoch = shell.core.account_epoch;
        match change {
            0 => shell.core.active_account = None,
            1 => shell.core.session = None,
            2 => owner.name.push('x'),
            3 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&owner.team_id)
                    .unwrap()
                    .team_id = "TFOREIGN".into()
            }
            4 => shell.core.transport = None,
            5 => shell.signed_in = false,
            6 => shell.loading = true,
            7 => shell.core.account_epoch += 1,
            _ => shell.connection.routed_at = None,
        }
        assert!(
            Observation::capture(&shell, epoch, &transport, &owner, "a").is_none(),
            "invalid capture {change}"
        );
    }
}

#[test]
fn route_sample_cannot_disappear_or_move_backwards_while_query_is_pending() {
    let (mut shell, transport, owner) = setup();
    let observed =
        Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a").unwrap();
    let at = shell.connection.routed_at.unwrap();
    shell.connection.routed_at = None;
    assert!(!observed.current(&shell, "a"));
    shell.connection.routed_at = at.checked_sub(std::time::Duration::from_millis(1));
    assert!(!observed.current(&shell, "a"));
    shell.connection.routed_at = Some(at + std::time::Duration::from_millis(1));
    assert!(observed.current(&shell, "a"));
}

#[test]
fn local_decision_becoming_remote_requires_new_observation() {
    let (mut shell, transport, owner) = setup();
    shell.connection.routable = false;
    let observed =
        Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a").unwrap();
    assert_eq!(observed.path(), Path::Local);
    shell.connection.routable = true;
    assert!(!observed.current(&shell, "a"));
    assert_eq!(
        Observation::capture(&shell, shell.core.account_epoch, &transport, &owner, "a")
            .unwrap()
            .path(),
        Path::Remote
    );
}
