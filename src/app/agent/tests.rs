use super::*;

#[test]
fn deserializes_core_commands() {
    let ping: AgentRequest = serde_json::from_str(r#"{"id":1,"cmd":"ping"}"#).unwrap();
    assert!(matches!(ping.cmd, AgentCommand::Ping));

    let query: AgentRequest =
        serde_json::from_str(r#"{"id":2,"cmd":"set-query","query":"dev"}"#).unwrap();
    match query.cmd {
        AgentCommand::SetQuery { query } => assert_eq!(query, "dev"),
        other => panic!("unexpected {other:?}"),
    }

    let mov: AgentRequest = serde_json::from_str(r#"{"id":3,"cmd":"move","delta":-1}"#).unwrap();
    match mov.cmd {
        AgentCommand::Move { delta } => assert_eq!(delta, -1),
        other => panic!("unexpected {other:?}"),
    }

    let shot: AgentRequest =
        serde_json::from_str(r#"{"cmd":"screenshot","path":"tmp/x.png"}"#).unwrap();
    match shot.cmd {
        AgentCommand::Screenshot { path } => {
            assert_eq!(path.as_deref(), Some("tmp/x.png"));
        }
        other => panic!("unexpected {other:?}"),
    }

    let profile: AgentRequest =
        serde_json::from_str(r#"{"cmd":"open-profile","user":"U123"}"#).unwrap();
    match profile.cmd {
        AgentCommand::OpenProfile { user } => assert_eq!(user, "U123"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn dump_state_from_login_fixture_shape() {
    let app = App::empty();
    let state = dump_state(&app);
    assert_eq!(state["screen"], "login");
    assert_eq!(state["signed_in"], false);
    assert!(state.get("agent_socket").is_some());
}